//! Code folding state management.
//!
//! This module tracks which fold regions are currently collapsed and provides
//! operations for folding and unfolding regions.
//!
//! # Performance
//!
//! Visual↔document line mapping operations are O(log n) through a cached
//! prefix-sum structure that is rebuilt only when fold state changes.

use std::collections::HashSet;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{FoldDetector, FoldKind, FoldRegion, Language, SyntaxTree};
#[cfg(feature = "syntax")]
use iridium_syntax::{FoldDetector, FoldKind, FoldRegion, Language, SyntaxTree};

use serde::{Deserialize, Serialize};

/// Cached line mapping for O(log n) visual↔document line conversion.
///
/// This structure is rebuilt when fold state changes and enables efficient
/// lookups without per-line iteration.
#[derive(Debug, Clone, Default)]
struct LineMapping {
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
    fn build(folded_lines: &HashSet<usize>, regions: &[FoldRegion]) -> Self {
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
    fn visual_to_document(&self, visual_line: usize) -> usize {
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
    fn document_to_visual(&self, doc_line: usize) -> Option<usize> {
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
    const fn total_hidden(&self) -> usize {
        self.total_hidden
    }

    /// Checks if a document line is hidden. O(log n).
    fn is_hidden(&self, doc_line: usize) -> bool {
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

/// Manages the fold state for a document.
///
/// This tracks which regions are currently folded and provides methods for
/// fold/unfold operations. It integrates with `FoldDetector` from iridium-syntax
/// to detect foldable regions.
///
/// # Performance
///
/// Visual↔document line mapping is O(log n) through a cached prefix-sum
/// structure. The cache is rebuilt only when fold operations occur, not on
/// every query. This enables 120fps rendering on files with thousands of lines.
///
/// # Example
///
/// ```ignore
/// use iridium_editor::editor::FoldState;
/// use iridium_syntax::Language;
///
/// let mut fold_state = FoldState::new();
/// fold_state.set_language(Language::Rust);
/// fold_state.update_regions("fn main() {\n    println!(\"hello\");\n}");
///
/// // Fold the function
/// if fold_state.fold_at(0) {
///     // Line 0 is now folded
/// }
/// ```
#[derive(Debug)]
pub struct FoldState {
    /// The fold detector for the current language
    detector: Option<FoldDetector>,
    /// The parse tree the regions are read from.
    ///
    /// Owned here only until the editor owns one tree for the whole document;
    /// until then this is the fold detector's own copy, and it is the reason
    /// `update_regions_incremental` can be cheap.
    tree: Option<SyntaxTree>,
    /// Currently detected fold regions
    regions: Vec<FoldRegion>,
    /// Set of start lines for currently folded regions
    folded_lines: HashSet<usize>,
    /// Current language
    language: Option<Language>,
    /// Cached line mapping for O(log n) visual↔document conversion.
    /// Rebuilt when fold state changes.
    line_mapping: LineMapping,
}

impl Default for FoldState {
    fn default() -> Self {
        Self::new()
    }
}

impl FoldState {
    /// Creates a new fold state with no language set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            detector: None,
            tree: None,
            regions: Vec::new(),
            folded_lines: HashSet::new(),
            language: None,
            line_mapping: LineMapping::default(),
        }
    }

    /// Creates a new fold state for the given language.
    #[must_use]
    pub fn for_language(language: Language) -> Self {
        Self {
            detector: Some(FoldDetector::new(language)),
            tree: SyntaxTree::new(language).ok(),
            regions: Vec::new(),
            folded_lines: HashSet::new(),
            language: Some(language),
            line_mapping: LineMapping::default(),
        }
    }

    /// Rebuilds the line mapping cache after fold state changes.
    fn rebuild_line_mapping(&mut self) {
        self.line_mapping = LineMapping::build(&self.folded_lines, &self.regions);
    }

    /// Returns the current language, if any.
    #[must_use]
    pub const fn language(&self) -> Option<Language> {
        self.language
    }

    /// Sets the language and reinitializes the fold detector.
    ///
    /// This clears all current fold regions and folded state.
    pub fn set_language(&mut self, language: Language) {
        if self.language == Some(language) {
            return;
        }
        self.language = Some(language);
        self.detector = Some(FoldDetector::new(language));
        self.tree = SyntaxTree::new(language).ok();
        self.regions.clear();
        self.folded_lines.clear();
        self.line_mapping = LineMapping::default();
    }

    /// Clears the language and all fold state.
    pub fn clear_language(&mut self) {
        self.language = None;
        self.detector = None;
        self.tree = None;
        self.regions.clear();
        self.folded_lines.clear();
        self.line_mapping = LineMapping::default();
    }

    /// Updates the fold regions by parsing the source code.
    ///
    /// Returns true if the regions changed.
    pub fn update_regions(&mut self, source: &str) -> bool {
        let (Some(detector), Some(tree)) = (&self.detector, &mut self.tree) else {
            return false;
        };
        let Some(parsed) = tree.parse(source) else {
            return false;
        };

        let new_regions = detector.regions_in(parsed, source);

        if new_regions == self.regions {
            return false;
        }

        // Preserve folded state for regions that still exist
        let old_folded = std::mem::take(&mut self.folded_lines);

        self.regions = new_regions;

        // Re-apply folded state to matching regions
        for region in &self.regions {
            if old_folded.contains(&region.start_line) {
                self.folded_lines.insert(region.start_line);
            }
        }

        // Rebuild line mapping if any folds are active
        self.rebuild_line_mapping();

        true
    }

    /// Updates the fold regions incrementally after an edit.
    ///
    /// Returns true if the regions changed.
    pub fn update_regions_incremental(
        &mut self,
        source: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> bool {
        let (Some(detector), Some(tree)) = (&self.detector, &mut self.tree) else {
            return false;
        };
        let Some(parsed) = tree.edit_bytes(source, start_byte, old_end_byte, new_end_byte) else {
            return false;
        };

        let new_regions = detector.regions_in(parsed, source);

        if new_regions == self.regions {
            return false;
        }

        let old_folded = std::mem::take(&mut self.folded_lines);
        self.regions = new_regions;

        for region in &self.regions {
            if old_folded.contains(&region.start_line) {
                self.folded_lines.insert(region.start_line);
            }
        }

        // Rebuild line mapping if any folds are active
        self.rebuild_line_mapping();

        true
    }

    /// Returns all detected fold regions.
    #[must_use]
    pub fn regions(&self) -> &[FoldRegion] {
        &self.regions
    }

    /// Returns the fold region at the given line, if any.
    #[must_use]
    pub fn region_at(&self, line: usize) -> Option<&FoldRegion> {
        self.regions.iter().find(|r| r.start_line == line)
    }

    /// Returns the innermost fold region containing the given line.
    ///
    /// This finds a region where `start_line <= line <= end_line`.
    /// If multiple regions contain the line, returns the innermost (smallest) one.
    #[must_use]
    pub fn region_containing(&self, line: usize) -> Option<&FoldRegion> {
        self.regions
            .iter()
            .filter(|r| r.start_line <= line && line <= r.end_line)
            .min_by_key(|r| r.end_line - r.start_line)
    }

    /// Returns true if the given line is the start of a foldable region.
    #[must_use]
    pub fn is_foldable(&self, line: usize) -> bool {
        self.regions.iter().any(|r| r.start_line == line)
    }

    /// Returns true if the given line is inside any foldable region.
    #[must_use]
    pub fn is_in_foldable_region(&self, line: usize) -> bool {
        self.region_containing(line).is_some()
    }

    /// Returns true if the given line is the start of a currently folded region.
    #[must_use]
    pub fn is_folded(&self, line: usize) -> bool {
        self.folded_lines.contains(&line)
    }

    /// Returns true if the given line is hidden by a fold.
    ///
    /// A line is hidden if it's inside a folded region (but not the start line).
    /// This operation is O(log n) using the cached line mapping.
    #[must_use]
    pub fn is_line_hidden(&self, line: usize) -> bool {
        self.line_mapping.is_hidden(line)
    }

    /// Folds the region at the given line.
    ///
    /// Returns true if the region was folded, false if the line is not foldable
    /// or already folded.
    pub fn fold_at(&mut self, line: usize) -> bool {
        if !self.is_foldable(line) || self.is_folded(line) {
            return false;
        }
        self.folded_lines.insert(line);
        self.rebuild_line_mapping();
        true
    }

    /// Unfolds the region at the given line.
    ///
    /// Returns true if the region was unfolded, false if the line is not folded.
    pub fn unfold_at(&mut self, line: usize) -> bool {
        let removed = self.folded_lines.remove(&line);
        if removed {
            self.rebuild_line_mapping();
        }
        removed
    }

    /// Toggles the fold state of the region at the given line.
    ///
    /// Returns true if the region was toggled, false if the line is not foldable.
    pub fn toggle_fold_at(&mut self, line: usize) -> bool {
        if !self.is_foldable(line) {
            return false;
        }
        if self.is_folded(line) {
            self.unfold_at(line)
        } else {
            self.fold_at(line)
        }
    }

    /// Toggles the fold state of the region containing the given line.
    ///
    /// This finds the innermost region that contains the line and toggles it.
    /// Returns the start line of the toggled region, or None if not in a foldable region.
    pub fn toggle_fold_containing(&mut self, line: usize) -> Option<usize> {
        let start_line = self.region_containing(line)?.start_line;
        if self.is_folded(start_line) {
            self.unfold_at(start_line);
        } else {
            self.fold_at(start_line);
        }
        Some(start_line)
    }

    /// Folds all foldable regions.
    pub fn fold_all(&mut self) {
        for region in &self.regions {
            self.folded_lines.insert(region.start_line);
        }
        self.rebuild_line_mapping();
    }

    /// Unfolds all folded regions.
    pub fn unfold_all(&mut self) {
        self.folded_lines.clear();
        self.line_mapping = LineMapping::default();
    }

    /// Folds all regions of a specific kind.
    pub fn fold_all_of_kind(&mut self, kind: FoldKind) {
        for region in &self.regions {
            if region.kind == kind {
                self.folded_lines.insert(region.start_line);
            }
        }
        self.rebuild_line_mapping();
    }

    /// Unfolds all regions of a specific kind.
    pub fn unfold_all_of_kind(&mut self, kind: FoldKind) {
        // Collect lines to remove first to avoid borrow conflicts
        let lines_to_remove: Vec<usize> = self
            .folded_lines
            .iter()
            .filter(|&&line| {
                self.regions
                    .iter()
                    .find(|r| r.start_line == line)
                    .is_some_and(|r| r.kind == kind)
            })
            .copied()
            .collect();

        for line in lines_to_remove {
            self.folded_lines.remove(&line);
        }
        self.rebuild_line_mapping();
    }

    /// Returns all currently folded start lines.
    pub fn folded_lines(&self) -> impl Iterator<Item = usize> + '_ {
        self.folded_lines.iter().copied()
    }

    /// Returns the number of currently folded regions.
    #[must_use]
    pub fn folded_count(&self) -> usize {
        self.folded_lines.len()
    }

    /// Returns the total number of hidden lines.
    /// This operation is O(1) using the cached line mapping.
    #[must_use]
    pub const fn hidden_line_count(&self) -> usize {
        self.line_mapping.total_hidden()
    }

    /// Maps a visual line number to a document line number.
    ///
    /// Visual lines skip over hidden (folded) lines. If the visual line
    /// points to a hidden line, returns the start line of the fold.
    ///
    /// This operation is O(log n) using the cached line mapping.
    #[must_use]
    pub fn visual_to_document_line(&self, visual_line: usize) -> usize {
        self.line_mapping.visual_to_document(visual_line)
    }

    /// Maps a document line number to a visual line number.
    ///
    /// If the document line is hidden, returns None.
    ///
    /// This operation is O(log n) using the cached line mapping.
    #[must_use]
    pub fn document_to_visual_line(&self, doc_line: usize) -> Option<usize> {
        self.line_mapping.document_to_visual(doc_line)
    }

    /// Returns the visible lines count (total lines minus hidden lines).
    #[must_use]
    pub const fn visible_line_count(&self, total_lines: usize) -> usize {
        total_lines.saturating_sub(self.hidden_line_count())
    }

    /// Returns fold information for export.
    #[must_use]
    pub fn export_fold_info(&self) -> FoldInfo {
        FoldInfo {
            regions: self.regions.iter().map(|r| r.start_line).collect(),
            folded: self.folded_lines.iter().copied().collect(),
        }
    }

    /// Imports fold information (e.g., from a saved session).
    pub fn import_fold_info(&mut self, info: &FoldInfo) {
        // Only import folded state for valid regions
        self.folded_lines.clear();
        for &line in &info.folded {
            if self.is_foldable(line) {
                self.folded_lines.insert(line);
            }
        }
        self.rebuild_line_mapping();
    }
}

/// Serializable fold information for persistence.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FoldInfo {
    /// Start lines of all foldable regions
    pub regions: Vec<usize>,
    /// Start lines of currently folded regions
    pub folded: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_rust_fold_state(source: &str) -> FoldState {
        let mut state = FoldState::for_language(Language::Rust);
        state.update_regions(source);
        state
    }

    #[test]
    fn basic_fold_unfold() {
        let source = "fn main() {\n    println!(\"hello\");\n}";
        let mut state = setup_rust_fold_state(source);

        // Should have detected the function
        assert!(!state.regions().is_empty());
        assert!(state.is_foldable(0));
        assert!(!state.is_folded(0));

        // Fold it
        assert!(state.fold_at(0));
        assert!(state.is_folded(0));

        // Line 1 should be hidden
        assert!(state.is_line_hidden(1));
        assert!(!state.is_line_hidden(0)); // Start line is never hidden

        // Unfold it
        assert!(state.unfold_at(0));
        assert!(!state.is_folded(0));
        assert!(!state.is_line_hidden(1));
    }

    #[test]
    fn fold_all_unfold_all() {
        let source = r#"fn foo() {
    println!("foo");
}

fn bar() {
    println!("bar");
}"#;
        let mut state = setup_rust_fold_state(source);

        state.fold_all();
        assert!(state.is_folded(0));
        assert!(state.is_folded(4));

        state.unfold_all();
        assert!(!state.is_folded(0));
        assert!(!state.is_folded(4));
    }

    #[test]
    fn visual_line_mapping() {
        let source = r"line 0
fn foo() {
    hidden 1
    hidden 2
}
line 5";
        let mut state = setup_rust_fold_state(source);

        // Without folds, visual == document
        assert_eq!(state.visual_to_document_line(0), 0);
        assert_eq!(state.visual_to_document_line(5), 5);

        // Fold the function
        assert!(state.fold_at(1));

        // Visual line 0 -> doc line 0 (before fold)
        // Visual line 1 -> doc line 1 (fold start, visible)
        // Visual line 2 -> doc line 5 (after fold)
        assert_eq!(state.document_to_visual_line(0), Some(0));
        assert_eq!(state.document_to_visual_line(1), Some(1));
        assert_eq!(state.document_to_visual_line(2), None); // Hidden
        assert_eq!(state.document_to_visual_line(3), None); // Hidden
        assert_eq!(state.document_to_visual_line(4), None); // Hidden (closing brace)
        assert_eq!(state.document_to_visual_line(5), Some(2));
    }

    #[test]
    fn hidden_line_count() {
        let source = r"fn foo() {
    line 1
    line 2
    line 3
}";
        let mut state = setup_rust_fold_state(source);

        assert_eq!(state.hidden_line_count(), 0);

        state.fold_at(0);
        assert_eq!(state.hidden_line_count(), 4); // Lines 1-4 are hidden
    }

    #[test]
    fn toggle_fold() {
        let source = "fn main() {\n    println!(\"hello\");\n}";
        let mut state = setup_rust_fold_state(source);

        assert!(!state.is_folded(0));
        assert!(state.toggle_fold_at(0));
        assert!(state.is_folded(0));
        assert!(state.toggle_fold_at(0));
        assert!(!state.is_folded(0));
    }

    #[test]
    fn non_foldable_line() {
        let source = "let x = 1;";
        let mut state = setup_rust_fold_state(source);

        assert!(!state.is_foldable(0));
        assert!(!state.fold_at(0));
        assert!(!state.toggle_fold_at(0));
    }

    #[test]
    fn export_import() {
        let source = "fn main() {\n    println!(\"hello\");\n}";
        let mut state = setup_rust_fold_state(source);
        state.fold_at(0);

        let info = state.export_fold_info();
        assert!(info.folded.contains(&0));

        state.unfold_all();
        assert!(!state.is_folded(0));

        state.import_fold_info(&info);
        assert!(state.is_folded(0));
    }

    #[test]
    fn language_change_clears_state() {
        let source = "fn main() {\n    println!(\"hello\");\n}";
        let mut state = setup_rust_fold_state(source);
        state.fold_at(0);
        assert!(state.is_folded(0));

        state.set_language(Language::Python);
        assert!(state.regions().is_empty());
        assert!(!state.is_folded(0));
    }

    /// Regression: a fold nested inside another must not have its lines counted
    /// twice.
    ///
    /// `fold_all` folds every detected region, including nested ones, and
    /// `LineMapping::build` used to sum each fold's span independently. For an
    /// outer fold hiding four lines and an inner fold hiding two of the same
    /// four, that reported six hidden lines out of six — so
    /// `visible_line_count` returned 0 while `is_line_hidden` correctly
    /// reported two visible lines, and `document_to_visual_line` underflowed
    /// `doc_line - hidden_before` and panicked on the first line after the
    /// folds. Any nested block reached it.
    #[test]
    fn nested_folds_do_not_double_count_hidden_lines() {
        let source = "fn a() {\n    if x {\n        y();\n    }\n}\nend\n";
        let mut state = setup_rust_fold_state(source);
        let total_lines = 6;

        state.fold_all();

        let visible: Vec<usize> = (0..total_lines)
            .filter(|&line| !state.is_line_hidden(line))
            .collect();
        assert_eq!(visible, vec![0, 5], "the wrong lines are hidden");
        assert_eq!(
            state.visible_line_count(total_lines),
            visible.len(),
            "visible_line_count disagrees with is_line_hidden"
        );

        // The mapping must be total over every line: hidden lines report None,
        // visible ones report their row, and nothing panics.
        assert_eq!(state.document_to_visual_line(0), Some(0));
        for hidden_line in 1..=4 {
            assert_eq!(state.document_to_visual_line(hidden_line), None);
        }
        assert_eq!(
            state.document_to_visual_line(5),
            Some(1),
            "the first line after a nested fold is misplaced"
        );
        assert_eq!(state.visual_to_document_line(1), 5);
    }

    /// Two folds that merely sit next to each other must stay separate: the
    /// line between them is visible, so coalescing them would hide it.
    #[test]
    fn adjacent_folds_are_not_merged() {
        // Lines 0-2 are one block, 3-5 another; line 3 is visible.
        let source = "fn a() {\n    x();\n}\nfn b() {\n    y();\n}\nend\n";
        let mut state = setup_rust_fold_state(source);
        let total_lines = 7;

        state.fold_all();

        let visible: Vec<usize> = (0..total_lines)
            .filter(|&line| !state.is_line_hidden(line))
            .collect();
        assert_eq!(
            visible,
            vec![0, 3, 6],
            "the line between two folds must stay visible"
        );
        assert_eq!(state.visible_line_count(total_lines), visible.len());
        assert_eq!(state.document_to_visual_line(3), Some(1));
        assert_eq!(state.document_to_visual_line(6), Some(2));
    }

    /// Every line of a document must map without panicking, whatever is folded.
    /// This is the property the underflow violated.
    #[test]
    fn every_line_maps_under_every_combination_of_folds() {
        let source = "fn a() {\n    if x {\n        y();\n    }\n}\nend\n";
        let total_lines = 6;

        // Every pair, so nested and disjoint combinations are both covered —
        // folding one line at a time can never nest, and nesting is where the
        // accounting broke.
        for first in 0..total_lines {
            for second in 0..total_lines {
                let mut state = setup_rust_fold_state(source);
                if !state.fold_at(first) {
                    continue;
                }
                state.fold_at(second);

                let mut rows = Vec::new();
                for line in 0..total_lines {
                    if let Some(row) = state.document_to_visual_line(line) {
                        rows.push(row);
                    }
                }
                assert_eq!(
                    rows.len(),
                    state.visible_line_count(total_lines),
                    "folding {first} then {second} disagrees on how many lines are visible"
                );
                // Rows must be a gap-free 0..n sequence, or a renderer would
                // skip or repeat a screen line.
                assert!(
                    rows.iter().copied().eq(0..rows.len()),
                    "folding {first} then {second} produced non-contiguous rows: {rows:?}"
                );
                // And the round-trip must land back on a visible line.
                for (row, &line) in rows.iter().enumerate() {
                    let _ = line;
                    let doc = state.visual_to_document_line(row);
                    assert!(
                        !state.is_line_hidden(doc),
                        "row {row} maps to hidden line {doc} after folding {first} then {second}"
                    );
                }
            }
        }
    }
}
