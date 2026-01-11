//! Code folding state management.
//!
//! This module tracks which fold regions are currently collapsed and provides
//! operations for folding and unfolding regions.

use std::collections::HashSet;

use iridium_syntax::{FoldDetector, FoldKind, FoldRegion, Language};
use serde::{Deserialize, Serialize};

/// Manages the fold state for a document.
///
/// This tracks which regions are currently folded and provides methods for
/// fold/unfold operations. It integrates with `FoldDetector` from iridium-syntax
/// to detect foldable regions.
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
    /// Currently detected fold regions
    regions: Vec<FoldRegion>,
    /// Set of start lines for currently folded regions
    folded_lines: HashSet<usize>,
    /// Current language
    language: Option<Language>,
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
            regions: Vec::new(),
            folded_lines: HashSet::new(),
            language: None,
        }
    }

    /// Creates a new fold state for the given language.
    #[must_use]
    pub fn for_language(language: Language) -> Self {
        let detector = FoldDetector::new(language);
        Self {
            detector,
            regions: Vec::new(),
            folded_lines: HashSet::new(),
            language: Some(language),
        }
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
        self.detector = FoldDetector::new(language);
        self.regions.clear();
        self.folded_lines.clear();
    }

    /// Clears the language and all fold state.
    pub fn clear_language(&mut self) {
        self.language = None;
        self.detector = None;
        self.regions.clear();
        self.folded_lines.clear();
    }

    /// Updates the fold regions by parsing the source code.
    ///
    /// Returns true if the regions changed.
    pub fn update_regions(&mut self, source: &str) -> bool {
        let Some(detector) = &mut self.detector else {
            return false;
        };

        let new_regions = detector.detect(source);

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
        let Some(detector) = &mut self.detector else {
            return false;
        };

        let new_regions = detector.update(source, start_byte, old_end_byte, new_end_byte);

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

    /// Returns true if the given line is the start of a foldable region.
    #[must_use]
    pub fn is_foldable(&self, line: usize) -> bool {
        self.regions.iter().any(|r| r.start_line == line)
    }

    /// Returns true if the given line is the start of a currently folded region.
    #[must_use]
    pub fn is_folded(&self, line: usize) -> bool {
        self.folded_lines.contains(&line)
    }

    /// Returns true if the given line is hidden by a fold.
    ///
    /// A line is hidden if it's inside a folded region (but not the start line).
    #[must_use]
    pub fn is_line_hidden(&self, line: usize) -> bool {
        for start_line in &self.folded_lines {
            if let Some(region) = self.region_at(*start_line) {
                // Line is hidden if it's after the start line and within the region
                if line > region.start_line && line <= region.end_line {
                    return true;
                }
            }
        }
        false
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
        true
    }

    /// Unfolds the region at the given line.
    ///
    /// Returns true if the region was unfolded, false if the line is not folded.
    pub fn unfold_at(&mut self, line: usize) -> bool {
        self.folded_lines.remove(&line)
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

    /// Folds all foldable regions.
    pub fn fold_all(&mut self) {
        for region in &self.regions {
            self.folded_lines.insert(region.start_line);
        }
    }

    /// Unfolds all folded regions.
    pub fn unfold_all(&mut self) {
        self.folded_lines.clear();
    }

    /// Folds all regions of a specific kind.
    pub fn fold_all_of_kind(&mut self, kind: FoldKind) {
        for region in &self.regions {
            if region.kind == kind {
                self.folded_lines.insert(region.start_line);
            }
        }
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
                    .map_or(false, |r| r.kind == kind)
            })
            .copied()
            .collect();

        for line in lines_to_remove {
            self.folded_lines.remove(&line);
        }
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
    #[must_use]
    pub fn hidden_line_count(&self) -> usize {
        let mut count = 0;
        for start_line in &self.folded_lines {
            if let Some(region) = self.region_at(*start_line) {
                count += region.hidden_line_count();
            }
        }
        count
    }

    /// Maps a visual line number to a document line number.
    ///
    /// Visual lines skip over hidden (folded) lines. If the visual line
    /// points to a hidden line, returns the start line of the fold.
    #[must_use]
    pub fn visual_to_document_line(&self, visual_line: usize) -> usize {
        if self.folded_lines.is_empty() {
            return visual_line;
        }

        let mut doc_line = 0;
        let mut vis_line = 0;

        while vis_line < visual_line {
            if self.is_line_hidden(doc_line) {
                // Skip hidden lines (they don't count as visual lines)
                doc_line += 1;
                continue;
            }

            // Check if this is a folded line (start of fold)
            if self.is_folded(doc_line) {
                // The fold start line is visible, skip the hidden content
                if let Some(region) = self.region_at(doc_line) {
                    vis_line += 1;
                    doc_line = region.end_line + 1;
                    continue;
                }
            }

            vis_line += 1;
            doc_line += 1;
        }

        // Handle final hidden lines
        while self.is_line_hidden(doc_line) {
            doc_line += 1;
        }

        doc_line
    }

    /// Maps a document line number to a visual line number.
    ///
    /// If the document line is hidden, returns None.
    #[must_use]
    pub fn document_to_visual_line(&self, doc_line: usize) -> Option<usize> {
        if self.is_line_hidden(doc_line) {
            return None;
        }

        if self.folded_lines.is_empty() {
            return Some(doc_line);
        }

        let mut visual_line = 0;
        let mut current_doc = 0;

        while current_doc < doc_line {
            if self.is_line_hidden(current_doc) {
                current_doc += 1;
                continue;
            }

            if self.is_folded(current_doc) {
                if let Some(region) = self.region_at(current_doc) {
                    visual_line += 1;
                    current_doc = region.end_line + 1;
                    continue;
                }
            }

            visual_line += 1;
            current_doc += 1;
        }

        Some(visual_line)
    }

    /// Returns the visible lines count (total lines minus hidden lines).
    #[must_use]
    pub fn visible_line_count(&self, total_lines: usize) -> usize {
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
        let source = r#"line 0
fn foo() {
    hidden 1
    hidden 2
}
line 5"#;
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
        let source = r#"fn foo() {
    line 1
    line 2
    line 3
}"#;
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
}
