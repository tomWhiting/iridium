//! The one place that knows how fold regions are produced.
//!
//! Two things produce them, and they need different inputs. With the `syntax`
//! feature the regions come off the parse tree, and an edit only has to touch
//! the part of that tree tree-sitter says moved — the document text is never
//! read. Without it there is no tree at all, and the stand-in scans the text for
//! matching braces, which is the one thing that *does* need the whole document.
//!
//! Rather than push that split out to every caller as a `cfg`, it is settled
//! once here behind a single method. The parameter each side ignores is bound to
//! `_` in that side's body, so an unused argument is stated in the code rather
//! than silenced by an attribute.
//!
//! The text arrives as a closure for the same reason. Producing it from a rope
//! costs a full copy of the document — 1.25 ms on a 100,000-line file — and the
//! tree-sitter path never looks at it. A closure lets the caller that already
//! holds the text hand it over for nothing, and the caller that would have to
//! build it not build it at all.

use std::borrow::Cow;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{FoldDetector, FoldRegion, Language, Tree};
#[cfg(feature = "syntax")]
use iridium_syntax::{FoldCache, FoldRegion, Language, Tree};

use super::SyntaxDelta;

/// Fold regions for one document, and whatever state their producer needs.
#[derive(Debug)]
pub(super) struct Folds {
    /// The incremental cache, which owns both the rules and the regions.
    #[cfg(feature = "syntax")]
    cache: FoldCache,
    /// The brace scanner, which is stateless.
    #[cfg(not(feature = "syntax"))]
    detector: FoldDetector,
    /// The regions the scanner last produced.
    #[cfg(not(feature = "syntax"))]
    regions: Vec<FoldRegion>,
}

impl Folds {
    /// Creates an empty producer for `language`.
    #[cfg(feature = "syntax")]
    pub(super) const fn new(language: Language) -> Self {
        Self {
            cache: FoldCache::new(language),
        }
    }

    /// Creates an empty producer for `language`.
    #[cfg(not(feature = "syntax"))]
    pub(super) fn new(language: Language) -> Self {
        Self {
            detector: FoldDetector::new(language),
            regions: Vec::new(),
        }
    }

    /// The regions currently held: the complete, whole-document list.
    #[cfg(feature = "syntax")]
    pub(super) fn regions(&self) -> &[FoldRegion] {
        self.cache.regions()
    }

    /// The regions currently held: the complete, whole-document list.
    #[cfg(not(feature = "syntax"))]
    pub(super) fn regions(&self) -> &[FoldRegion] {
        &self.regions
    }

    /// Recomputes the regions, returning true if they changed.
    ///
    /// `delta` says what happened to `tree` since the last call; `source`
    /// produces the text those two describe, and is called only if the region
    /// producer in this configuration actually needs it.
    #[cfg(feature = "syntax")]
    pub(super) fn refresh<'text>(
        &mut self,
        tree: &Tree,
        delta: &SyntaxDelta,
        source: impl FnOnce() -> Cow<'text, str>,
    ) -> bool {
        // Every fold the tree-sitter detector recognises is decided by node kind
        // and position, so the text is not read here — and, because it arrives
        // as a closure, not built either.
        let _ = source;

        match delta {
            // Nothing moved, so nothing derived from the tree can have moved
            // either — unless this cache has never seen a tree, in which case
            // "nothing changed since" describes a comparison that never
            // happened.
            SyntaxDelta::Unchanged => !self.cache.is_primed() && self.cache.rebuild(tree),
            SyntaxDelta::Full => self.cache.rebuild(tree),
            SyntaxDelta::Incremental { edit, changed } => self.cache.update(tree, edit, changed),
        }
    }

    /// Recomputes the regions, returning true if they changed.
    ///
    /// `delta` says what happened to `tree` since the last call; `source`
    /// produces the text those two describe, and is called only if the region
    /// producer in this configuration actually needs it.
    #[cfg(not(feature = "syntax"))]
    pub(super) fn refresh<'text>(
        &mut self,
        tree: &Tree,
        delta: &SyntaxDelta,
        source: impl FnOnce() -> Cow<'text, str>,
    ) -> bool {
        // The brace scanner reads text and nothing else. It has no tree to
        // compare against a previous one, so there is no cheaper answer than
        // rescanning, and the delta describes a distinction it cannot use.
        let _ = delta;

        let regions = self.detector.regions_in(tree, &source());
        if regions == self.regions {
            return false;
        }
        self.regions = regions;
        true
    }

    /// How many tree nodes have been examined for folds since this was created.
    ///
    /// Only meaningful with the `syntax` feature, and deliberately absent
    /// without it: the brace scanner walks text, not nodes, and a zero reported
    /// here would look like a claim rather than an absence.
    #[cfg(feature = "syntax")]
    pub(super) const fn nodes_visited(&self) -> u64 {
        self.cache.nodes_visited()
    }
}
