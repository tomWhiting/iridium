//! The panel's verbs, as a type the dispatch can match exhaustively.
//!
//! A resolved binding names a [`CommandId`], which is a string. Dispatching on
//! the string directly would mean writing every id a second time, in a second
//! file, with nothing to keep the two spellings in step.
//!
//! [`Verb`] is the one-to-one mapping. Every variant knows its id, the id is the
//! kernel constant and never a literal retyped here, and the dispatch matches on
//! the enum — so a verb added to the kernel table and forgotten in the panel is
//! a **compile error** rather than a key that quietly does nothing.
//!
//! # This enum is the terminal's, and the desktop has its own
//!
//! The two faces answer the same eight kernel commands and each own a `Verb`
//! over them. That is duplication with eyes open: the alternative is a shared
//! enum in the kernel, which is a better shape and a worse thing to attempt half
//! way through converting seven panels. What keeps the copies honest is
//! [`super::keymap_tests`], where a verb added to the kernel's `history` table
//! and missed here fails this face's own ratchet.
//!
//! # ⚠️ `history.togglePanel` is deliberately absent
//!
//! It is the mode-free host command that *opens* the tree, so the panel is not
//! on screen when it runs and cannot be what answers it. [`HISTORY_DISMISS`] is
//! the verb this panel owns, and the two share a chord without sharing an id —
//! which is what lets a user rebind either one alone.

use iridium_editor::CommandId;
use iridium_editor::commands::builtin::{
    HISTORY_DISMISS, HISTORY_JUMP, HISTORY_SELECT_FIRST, HISTORY_SELECT_LAST, HISTORY_SELECT_NEXT,
    HISTORY_SELECT_PAGE_DOWN, HISTORY_SELECT_PAGE_UP, HISTORY_SELECT_PREVIOUS,
};

/// One thing the undo-tree panel can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    /// Close the panel.
    Dismiss,
    /// Move the document to the selected state, leaving the panel open.
    Jump,
    /// Move the selection one row up.
    SelectPrevious,
    /// Move the selection one row down.
    SelectNext,
    /// Move the selection up by one windowful.
    SelectPageUp,
    /// Move the selection down by one windowful.
    SelectPageDown,
    /// Move the selection to the oldest state.
    SelectFirst,
    /// Move the selection to the newest state.
    SelectLast,
}

impl Verb {
    /// Every verb, in the order the panel's documentation table lists them.
    ///
    /// The single place the set is enumerated: [`Self::id`] is matched
    /// exhaustively against it, and the panel's tests walk it to prove every
    /// verb is both bound by the default keymap and reachable from a
    /// configuration file.
    pub(super) const ALL: &'static [Self] = &[
        Self::Dismiss,
        Self::Jump,
        Self::SelectPrevious,
        Self::SelectNext,
        Self::SelectPageUp,
        Self::SelectPageDown,
        Self::SelectFirst,
        Self::SelectLast,
    ];

    /// The command id this verb is.
    ///
    /// Every arm answers with the kernel's own constant, so the id has exactly
    /// one spelling in the repository and this file cannot drift from it.
    pub(super) const fn id(self) -> CommandId {
        match self {
            Self::Dismiss => HISTORY_DISMISS,
            Self::Jump => HISTORY_JUMP,
            Self::SelectPrevious => HISTORY_SELECT_PREVIOUS,
            Self::SelectNext => HISTORY_SELECT_NEXT,
            Self::SelectPageUp => HISTORY_SELECT_PAGE_UP,
            Self::SelectPageDown => HISTORY_SELECT_PAGE_DOWN,
            Self::SelectFirst => HISTORY_SELECT_FIRST,
            Self::SelectLast => HISTORY_SELECT_LAST,
        }
    }

    /// The verb `id` names, or `None` when it is not one of this panel's.
    ///
    /// `None` is routine rather than exceptional: a user's layer may name a
    /// command the panel has never heard of.
    pub(super) fn from_id(id: &CommandId) -> Option<Self> {
        Self::ALL.iter().copied().find(|verb| &verb.id() == id)
    }
}
