//! The background parser where there is none.
//!
//! Without the `syntax` feature there is no parser to move off the keystroke,
//! and on `wasm32` there is no thread to move it to. Both builds keep the same
//! shape as the threaded one so that the syntax state reads identically in
//! every configuration: [`Background`] holds an [`Infallible`], so no value of
//! it exists, every path that would use one is statically unreachable, and
//! asking for background parsing simply leaves parsing inline.

use std::convert::Infallible;

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{InputEdit, Tree};
#[cfg(feature = "syntax")]
use iridium_syntax::{InputEdit, Tree};

use iridium_lang::Language;

use super::state::SyntaxWake;
use crate::document::Document;

/// Never constructed: there is no background parser in this build.
#[derive(Debug)]
pub(super) struct Background(Infallible);

/// A finished job, which this build never produces.
pub(super) struct Landed(Infallible);

impl Landed {
    /// Unreachable: no value of this type exists in this build.
    pub(super) const fn unpack(self) -> Option<(Tree, LandedFolds, bool)> {
        match self.0 {}
    }
}

/// Fold regions from a background parse, which this build never produces.
#[derive(Debug)]
pub struct LandedFolds(Infallible);

impl LandedFolds {
    /// Unreachable: no value of this type exists in this build.
    pub const fn adopt_into(self, _fold_state: &mut crate::editor::FoldState) -> Option<bool> {
        match self.0 {}
    }
}

/// Why the worker can no longer be reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Gone;

impl Background {
    /// Always `None`: there is no thread to start in this build.
    pub(super) fn spawn(_wake: Option<SyntaxWake>, _current: bool) -> Option<Self> {
        None
    }

    /// Unreachable.
    pub(super) fn set_wake(&self, _wake: Option<SyntaxWake>) -> Result<(), Gone> {
        match self.0 {}
    }

    /// Unreachable.
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "mirrors the threaded parser's signature, which mutates"
    )]
    pub(super) const fn restart(&mut self) {
        match self.0 {}
    }

    /// Unreachable.
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "mirrors the threaded parser's signature, which mutates"
    )]
    pub(super) const fn note_edit(&mut self, _edit: InputEdit) {
        match self.0 {}
    }

    /// Unreachable.
    pub(super) const fn edits(&self) -> &[InputEdit] {
        match self.0 {}
    }

    /// Unreachable.
    pub(super) const fn behind(&self) -> bool {
        match self.0 {}
    }

    /// Unreachable.
    pub(super) const fn in_flight(&self) -> bool {
        match self.0 {}
    }

    /// Unreachable.
    #[cfg_attr(
        not(feature = "syntax"),
        expect(
            clippy::missing_const_for_fn,
            reason = "the stub tree is `Copy`; the real one has a destructor, which \
            a `const fn` cannot run, and this signature serves both"
        )
    )]
    pub(super) fn dispose(&self, _tree: Tree) {
        match self.0 {}
    }

    /// Unreachable.
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "mirrors the threaded parser's signature, which mutates"
    )]
    pub(super) const fn dispatch(
        &mut self,
        _language: Language,
        _document: &Document,
        _tree: Option<&Tree>,
    ) -> Result<(), Gone> {
        match self.0 {}
    }

    /// Unreachable.
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "mirrors the threaded parser's signature, which mutates"
    )]
    pub(super) const fn try_landed(&mut self) -> Result<Option<Landed>, Gone> {
        match self.0 {}
    }

    /// Unreachable.
    #[expect(
        clippy::needless_pass_by_ref_mut,
        reason = "mirrors the threaded parser's signature, which mutates"
    )]
    pub(super) const fn wait_landed(&mut self) -> Result<Landed, Gone> {
        match self.0 {}
    }
}
