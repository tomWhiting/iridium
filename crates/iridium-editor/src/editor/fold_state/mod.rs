//! Code folding state management.
//!
//! This module tracks which fold regions are currently collapsed and provides
//! operations for folding and unfolding regions.
//!
//! # Performance
//!
//! Visual↔document line mapping operations are O(log n) through a cached
//! prefix-sum structure that is rebuilt only when fold state changes.
//!
//! Detecting the regions themselves is the other half, and the expensive one:
//! it happens on every content change, so it must not cost a walk of the
//! document. See [`detect`] for how that is arranged, and why it looks
//! different with and without the `syntax` feature.

mod detect;
mod line_mapping;

use std::borrow::Cow;
use std::collections::HashSet;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{FoldKind, FoldRegion, Tree};
use iridium_lang::Language;
#[cfg(feature = "syntax")]
use iridium_syntax::{FoldKind, FoldRegion, Tree};

use serde::{Deserialize, Serialize};

use self::detect::Folds;
use self::line_mapping::LineMapping;
use super::SyntaxDelta;

/// Manages the fold state for a document.
///
/// This tracks which regions are currently folded and provides methods for
/// fold/unfold operations. Detecting the regions is delegated to this module's
/// private `detect`, which reads the document's one parse tree rather than
/// parsing its own.
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
/// // `update_regions` is driven by the editor, which owns the parse tree.
///
/// // Fold the function
/// if fold_state.fold_at(0) {
///     // Line 0 is now folded
/// }
/// ```
#[derive(Debug)]
pub struct FoldState {
    /// The region producer for the current language, and the regions it holds.
    folds: Option<Folds>,
    /// Set of start lines for currently folded regions
    folded_lines: HashSet<usize>,
    /// Current language
    language: Option<Language>,
    /// Cached line mapping for O(log n) visual↔document conversion.
    /// Rebuilt when fold state changes.
    line_mapping: LineMapping,
    /// Monotonic fold-state generation, moved by every mutation that can
    /// change what folding makes visible. Folding does not touch the
    /// document revision, so without this counter a retained frame could
    /// keep showing unfolded text after a fold — see
    /// [`Self::generation`].
    generation: u64,
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
            folds: None,
            folded_lines: HashSet::new(),
            language: None,
            line_mapping: LineMapping::default(),
            generation: 0,
        }
    }

    /// Creates a new fold state for the given language.
    #[must_use]
    pub fn for_language(language: Language) -> Self {
        Self {
            folds: Some(Folds::new(language)),
            folded_lines: HashSet::new(),
            language: Some(language),
            line_mapping: LineMapping::default(),
            generation: 0,
        }
    }

    /// Rebuilds the line mapping cache after fold state changes.
    fn rebuild_line_mapping(&mut self) {
        self.line_mapping = LineMapping::build(&self.folded_lines, self.regions());
    }

    /// The fold-state generation: moves on every mutation that can change
    /// which lines folding hides or how a folded line renders.
    ///
    /// Fold operations deliberately do not bump the document revision — the
    /// text is untouched — so a consumer caching anything derived from the
    /// visible content must key on this counter as well. Two equal values
    /// from the same instance guarantee the fold state is unchanged between
    /// them; the counter wraps, which is harmless for equality comparison.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Bumps [`Self::generation`]; called by every mutating operation that
    /// actually changed observable fold state.
    const fn bump_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
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
        self.folds = Some(Folds::new(language));
        self.folded_lines.clear();
        self.line_mapping = LineMapping::default();
        self.bump_generation();
    }

    /// Clears the language and all fold state.
    pub fn clear_language(&mut self) {
        self.language = None;
        self.folds = None;
        self.folded_lines.clear();
        self.line_mapping = LineMapping::default();
        self.bump_generation();
    }

    /// Updates the fold regions from a parse tree.
    ///
    /// `tree`, `source` and `delta` must all describe one document at one
    /// moment: the tree as it is now, the text it was parsed from, and what
    /// happened to it since folds were last refreshed. The caller owns the tree
    /// precisely so that folds, highlights and structural navigation all read
    /// one parse rather than three.
    ///
    /// `delta` is what keeps this off the O(document) path. A
    /// [`SyntaxDelta::Incremental`] lets the detector recompute only the part of
    /// the tree that moved; anything else recomputes the lot, which is correct
    /// and merely slower. Passing [`SyntaxDelta::Full`] is therefore always
    /// safe, and is the right answer for a caller that reparsed from scratch.
    ///
    /// `source` is a closure rather than a string because the tree-sitter
    /// detector never reads the text, and building it from a rope copies the
    /// whole document. A caller that already holds the text passes it back for
    /// nothing; a caller that would have to materialise it does not have to.
    ///
    /// Returns true if the regions changed.
    pub fn update_regions<'text>(
        &mut self,
        tree: &Tree,
        delta: &SyntaxDelta,
        source: impl FnOnce() -> Cow<'text, str>,
    ) -> bool {
        let Some(folds) = self.folds.as_mut() else {
            return false;
        };

        if !folds.refresh(tree, delta, source) {
            return false;
        }

        // Preserve folded state only for regions that still exist. A fold whose
        // start line no longer begins a region has nothing left to collapse.
        let old_folded = std::mem::take(&mut self.folded_lines);
        for region in folds.regions() {
            if old_folded.contains(&region.start_line) {
                self.folded_lines.insert(region.start_line);
            }
        }

        // Rebuild line mapping if any folds are active
        self.rebuild_line_mapping();
        self.bump_generation();

        true
    }

    /// Returns all detected fold regions.
    #[must_use]
    pub fn regions(&self) -> &[FoldRegion] {
        self.folds.as_ref().map_or(&[], Folds::regions)
    }

    /// How many tree nodes fold detection has examined since this state was
    /// created.
    ///
    /// The observable form of the claim that typing does not walk the document.
    /// A keystroke on a 100,000-line file moved this by millions before fold
    /// detection became incremental; the assertion that it no longer does is
    /// what stops that from coming back, and unlike a wall-clock bound it
    /// measures the algorithm rather than the machine.
    #[cfg(feature = "syntax")]
    #[must_use]
    pub fn fold_nodes_visited(&self) -> u64 {
        self.folds.as_ref().map_or(0, Folds::nodes_visited)
    }

    /// How many lines the brace scanner has read since this state was created.
    ///
    /// The same claim as [`FoldState::fold_nodes_visited`], counted in the unit
    /// the configuration without a parser works in. Deliberately absent with the
    /// `syntax` feature: there is no brace scanner then, and a zero reported
    /// here would look like a claim rather than an absence.
    #[cfg(not(feature = "syntax"))]
    #[must_use]
    pub fn fold_lines_scanned(&self) -> u64 {
        self.folds.as_ref().map_or(0, Folds::lines_scanned)
    }

    /// Returns the fold region at the given line, if any.
    #[must_use]
    pub fn region_at(&self, line: usize) -> Option<&FoldRegion> {
        self.regions().iter().find(|r| r.start_line == line)
    }

    /// Returns the innermost fold region containing the given line.
    ///
    /// This finds a region where `start_line <= line <= end_line`.
    /// If multiple regions contain the line, returns the innermost (smallest) one.
    #[must_use]
    pub fn region_containing(&self, line: usize) -> Option<&FoldRegion> {
        self.regions()
            .iter()
            .filter(|r| r.start_line <= line && line <= r.end_line)
            .min_by_key(|r| r.end_line - r.start_line)
    }

    /// Returns true if the given line is the start of a foldable region.
    #[must_use]
    pub fn is_foldable(&self, line: usize) -> bool {
        self.regions().iter().any(|r| r.start_line == line)
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
        self.bump_generation();
        true
    }

    /// Unfolds the region at the given line.
    ///
    /// Returns true if the region was unfolded, false if the line is not folded.
    pub fn unfold_at(&mut self, line: usize) -> bool {
        let removed = self.folded_lines.remove(&line);
        if removed {
            self.rebuild_line_mapping();
            self.bump_generation();
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
        let starts: Vec<usize> = self.regions().iter().map(|r| r.start_line).collect();
        self.folded_lines.extend(starts);
        self.rebuild_line_mapping();
        self.bump_generation();
    }

    /// Unfolds all folded regions.
    pub fn unfold_all(&mut self) {
        self.folded_lines.clear();
        self.line_mapping = LineMapping::default();
        self.bump_generation();
    }

    /// Folds all regions of a specific kind.
    pub fn fold_all_of_kind(&mut self, kind: FoldKind) {
        let starts: Vec<usize> = self
            .regions()
            .iter()
            .filter(|region| region.kind == kind)
            .map(|region| region.start_line)
            .collect();
        self.folded_lines.extend(starts);
        self.rebuild_line_mapping();
        self.bump_generation();
    }

    /// Unfolds all regions of a specific kind.
    pub fn unfold_all_of_kind(&mut self, kind: FoldKind) {
        // Collect lines to remove first to avoid borrow conflicts
        let lines_to_remove: Vec<usize> = self
            .folded_lines
            .iter()
            .filter(|&&line| {
                self.regions()
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
        self.bump_generation();
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
            regions: self.regions().iter().map(|r| r.start_line).collect(),
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
        self.bump_generation();
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
mod tests;
