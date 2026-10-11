//! Visual↔document line mapping across collapsed folds.
//!
//! Rebuilt when fold state changes, so a query costs a lookup rather than a
//! walk over every fold.

use std::collections::HashSet;
use std::sync::Arc;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::FoldRegion;
#[cfg(feature = "syntax")]
use iridium_syntax::FoldRegion;

/// Cached line mapping for O(log n) visual↔document line conversion.
///
/// This structure is rebuilt when fold state changes and enables efficient
/// lookups without per-line iteration.
#[derive(Debug, Clone, Default)]
pub struct LineMapping {
    /// Sorted list of fold boundaries: (`doc_line`, `cumulative_hidden_before`, `hidden_in_fold`)
    /// Each entry represents a fold start line and the total hidden lines before it,
    /// plus the number of lines hidden by this specific fold.
    boundaries: Arc<[FoldBoundary]>,
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
            boundaries: boundaries.into(),
            total_hidden: cumulative_hidden,
        }
    }

    /// Converts a visual line to a document line in O(log n).
    pub fn visual_to_document(&self, visual_line: usize) -> usize {
        let index = self
            .boundaries
            .partition_point(|boundary| boundary.start_line - boundary.hidden_before < visual_line);
        let hidden = index.checked_sub(1).map_or(0, |index| {
            let boundary = &self.boundaries[index];
            boundary.hidden_before + boundary.hidden_count
        });
        visual_line.saturating_add(hidden)
    }

    /// The final fold whose header precedes this document line.
    fn boundary_before(&self, line: usize) -> Option<&FoldBoundary> {
        let index = self
            .boundaries
            .partition_point(|boundary| boundary.start_line < line);
        index
            .checked_sub(1)
            .and_then(|index| self.boundaries.get(index))
    }

    /// Converts a document line, returning None for a hidden line.
    pub(crate) fn document_to_visual(&self, line: usize) -> Option<usize> {
        (!self.is_hidden(line)).then(|| self.project_document(line))
    }

    /// Visual row of this line or the visible header of the fold hiding it.
    pub fn project_document(&self, line: usize) -> usize {
        let Some(boundary) = self.boundary_before(line) else {
            return line;
        };
        if line <= boundary.end_line {
            boundary.start_line - boundary.hidden_before
        } else {
            line - boundary.hidden_before - boundary.hidden_count
        }
    }

    /// Returns total hidden line count in O(1).
    pub(crate) const fn total_hidden(&self) -> usize {
        self.total_hidden
    }

    /// Checks whether a document line is hidden in O(log n).
    pub(crate) fn is_hidden(&self, line: usize) -> bool {
        self.boundary_before(line)
            .is_some_and(|boundary| line <= boundary.end_line)
    }
}
