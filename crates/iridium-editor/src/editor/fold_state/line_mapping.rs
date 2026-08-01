//! Visual↔document line mapping across collapsed folds.
//!
//! Rebuilt when fold state changes, so a query costs a lookup rather than a
//! walk over every fold.

use std::collections::HashSet;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::FoldRegion;
#[cfg(feature = "syntax")]
use iridium_syntax::FoldRegion;

/// Cached line mapping for O(log n) visual↔document line conversion.
///
/// This structure is rebuilt when fold state changes and enables efficient
/// lookups without per-line iteration.
#[derive(Debug, Clone, Default)]
pub(super) struct LineMapping {
    /// Sorted list of fold boundaries: (`doc_line`, `cumulative_hidden_before`, `hidden_in_fold`)
    /// Each entry represents a fold start line and the total hidden lines before it,
    /// plus the number of lines hidden by this specific fold.
    boundaries: Vec<FoldBoundary>,
    /// Total number of hidden lines across all folds.
    total_hidden: usize,
}

/// A fold boundary entry for the line mapping cache.
#[derive(Debug, Clone, Copy)]
struct FoldBoundary {
    /// Document line where the fold starts (this line is visible).
    start_line: usize,
    /// Document line where the fold ends (this line is hidden).
    end_line: usize,
    /// Cumulative count of hidden lines before this fold.
    hidden_before: usize,
    /// Number of lines hidden by this fold (`end_line` - `start_line`).
    hidden_count: usize,
}

impl LineMapping {
    /// Builds the line mapping from current fold state.
    pub(super) fn build(folded_lines: &HashSet<usize>, regions: &[FoldRegion]) -> Self {
        if folded_lines.is_empty() {
            return Self::default();
        }

        // Collect all active folds with their regions, sorted by start line
        let mut active_folds: Vec<_> = folded_lines
            .iter()
            .filter_map(|&start| {
                regions
                    .iter()
                    .find(|r| r.start_line == start)
                    .map(|r| (start, r.end_line))
            })
            .collect();
        active_folds.sort_by_key(|(start, _)| *start);

        // Coalesce folds that overlap or nest before counting anything.
        //
        // A fold hides the lines `(start_line, end_line]`, so two folds whose
        // spans touch hide an overlapping set of lines. Summing their spans
        // independently counts the shared lines twice: fold `0..=10` with a
        // nested fold `3..=5` hides ten lines, not twelve. That overcount is
        // not merely cosmetic — `document_to_visual` subtracts
        // `hidden_before` from a document line, so an inflated total made the
        // subtraction underflow and panic on the first visible line after the
        // folds. `fold_all` folds every region including nested ones, so any
        // nested block reached it.
        //
        // Merging is sound because the outermost fold's `start_line` is
        // visible and every line it hides is also hidden by the merged
        // interval: a fold strictly inside another contributes no new hidden
        // line, and two partially overlapping folds hide exactly the union of
        // their spans.
        let mut merged: Vec<(usize, usize)> = Vec::with_capacity(active_folds.len());
        for (start_line, end_line) in active_folds {
            match merged.last_mut() {
                // `start_line <= current_end` means this fold begins at or
                // inside the hidden run already accumulated, so it extends it
                // rather than starting a new one. A fold beginning exactly at
                // `current_end + 1` does NOT merge: that line is visible.
                Some((_, current_end)) if start_line <= *current_end => {
                    *current_end = (*current_end).max(end_line);
                },
                _ => merged.push((start_line, end_line)),
            }
        }

        // Build boundaries with cumulative hidden counts
        let mut boundaries = Vec::with_capacity(merged.len());
        let mut cumulative_hidden = 0;

        for (start_line, end_line) in merged {
            // Lines after start, up to and including end. `end_line` is always
            // greater than `start_line` for a real region, and `saturating_sub`
            // keeps a malformed one from wrapping instead of producing zero.
            let hidden_count = end_line.saturating_sub(start_line);
            boundaries.push(FoldBoundary {
                start_line,
                end_line,
                hidden_before: cumulative_hidden,
                hidden_count,
            });
            cumulative_hidden += hidden_count;
        }

        Self {
            boundaries,
            total_hidden: cumulative_hidden,
        }
    }

    /// Converts a visual line to a document line. O(log n).
    pub(super) fn visual_to_document(&self, visual_line: usize) -> usize {
        if self.boundaries.is_empty() {
            return visual_line;
        }

        // Visual line V maps to document line D where:
        // D = V + (total hidden lines before document line D)
        //
        // We iterate through boundaries to find how many hidden lines
        // come before the visual position.
        let mut hidden_so_far = 0;

        for boundary in &self.boundaries {
            // The visual line of this fold's start is: start_line - hidden_before
            let fold_visual_start = boundary.start_line - boundary.hidden_before;

            if visual_line < fold_visual_start {
                // Visual line is before this fold
                break;
            }

            // If the visual line is exactly at fold start, return the fold start
            if visual_line == fold_visual_start {
                return boundary.start_line;
            }

            // Visual line is after this fold start
            // Account for all lines hidden by this fold
            hidden_so_far = boundary.hidden_before + boundary.hidden_count;
        }

        visual_line + hidden_so_far
    }

    /// Converts a document line to a visual line. O(log n).
    /// Returns None if the document line is hidden.
    pub(super) fn document_to_visual(&self, doc_line: usize) -> Option<usize> {
        if self.boundaries.is_empty() {
            return Some(doc_line);
        }

        // Check if this line is hidden by any fold
        for boundary in &self.boundaries {
            if doc_line > boundary.start_line && doc_line <= boundary.end_line {
                // Line is inside a fold (hidden)
                return None;
            }
        }

        // Binary search for the last boundary with start_line <= doc_line
        let hidden_before = match self
            .boundaries
            .binary_search_by_key(&doc_line, |b| b.start_line)
        {
            Ok(idx) => {
                // Exact match - doc_line is a fold start (visible)
                self.boundaries[idx].hidden_before
            },
            Err(idx) => {
                if idx == 0 {
                    // Before all folds
                    0
                } else {
                    // After boundary at idx-1
                    let prev = &self.boundaries[idx - 1];
                    if doc_line <= prev.end_line {
                        // Inside the fold (this case handled above, but defensive)
                        return None;
                    }
                    // After the fold
                    prev.hidden_before + prev.hidden_count
                }
            },
        };

        Some(doc_line - hidden_before)
    }

    /// Returns total hidden line count. O(1).
    pub(super) const fn total_hidden(&self) -> usize {
        self.total_hidden
    }

    /// Checks if a document line is hidden. O(log n).
    pub(super) fn is_hidden(&self, doc_line: usize) -> bool {
        if self.boundaries.is_empty() {
            return false;
        }

        // Binary search for a boundary that might contain this line
        for boundary in &self.boundaries {
            if doc_line > boundary.start_line && doc_line <= boundary.end_line {
                return true;
            }
            if boundary.start_line > doc_line {
                break;
            }
        }
        false
    }
}
