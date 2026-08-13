//! The palette's verbs, as a type the dispatch can match exhaustively.
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
//! The two faces answer the same twelve kernel commands and each own a `Verb`
//! over them. That is duplication with eyes open: the alternative is a shared
//! enum in the kernel, which is a better shape and a worse thing to attempt
//! half way through converting seven panels. What keeps the copies honest is
//! [`super::keymap_tests`], where a verb added to the kernel's `palette` table
//! and missed here fails this face's own ratchet.
//!
//! # ⚠️ `palette.open` is deliberately absent
//!
//! It is the mode-free host command that *opens* the palette, so the panel is
//! not on screen when it runs and cannot be what answers it. `palette.dismiss`
//! is the verb this panel owns, and the two share a chord without sharing an id
//! — which is what lets a user rebind either one alone.

use iridium_editor::CommandId;
use iridium_editor::commands::builtin::{
    PALETTE_ACCEPT, PALETTE_CARET_END, PALETTE_CARET_HOME, PALETTE_CARET_LEFT, PALETTE_CARET_RIGHT,
    PALETTE_DISMISS, PALETTE_QUERY_BACKSPACE, PALETTE_QUERY_DELETE, PALETTE_SELECT_NEXT,
    PALETTE_SELECT_PAGE_DOWN, PALETTE_SELECT_PAGE_UP, PALETTE_SELECT_PREVIOUS,
};

/// One thing the command palette can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    /// Close the palette without running anything.
    Dismiss,
    /// Run the selected command and close.
    Accept,
    /// Move the selection one entry up.
    SelectPrevious,
    /// Move the selection one entry down.
    SelectNext,
    /// Move the selection up by one windowful.
    SelectPageUp,
    /// Move the selection down by one windowful.
    SelectPageDown,
    /// Move the caret one character left within the query.
    CaretLeft,
    /// Move the caret one character right within the query.
    CaretRight,
    /// Move the caret to the start of the query.
    CaretHome,
    /// Move the caret to the end of the query.
    CaretEnd,
    /// Delete the character before the caret in the query.
    QueryBackspace,
    /// Delete the character after the caret in the query.
    QueryDelete,
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
        Self::Accept,
        Self::SelectPrevious,
        Self::SelectNext,
        Self::SelectPageUp,
        Self::SelectPageDown,
        Self::CaretLeft,
        Self::CaretRight,
        Self::CaretHome,
        Self::CaretEnd,
        Self::QueryBackspace,
        Self::QueryDelete,
    ];

    /// The command id this verb is.
    ///
    /// Every arm answers with the kernel's own constant, so the id has exactly
    /// one spelling in the repository and this file cannot drift from it.
    pub(super) const fn id(self) -> CommandId {
        match self {
            Self::Dismiss => PALETTE_DISMISS,
            Self::Accept => PALETTE_ACCEPT,
            Self::SelectPrevious => PALETTE_SELECT_PREVIOUS,
            Self::SelectNext => PALETTE_SELECT_NEXT,
            Self::SelectPageUp => PALETTE_SELECT_PAGE_UP,
            Self::SelectPageDown => PALETTE_SELECT_PAGE_DOWN,
            Self::CaretLeft => PALETTE_CARET_LEFT,
            Self::CaretRight => PALETTE_CARET_RIGHT,
            Self::CaretHome => PALETTE_CARET_HOME,
            Self::CaretEnd => PALETTE_CARET_END,
            Self::QueryBackspace => PALETTE_QUERY_BACKSPACE,
            Self::QueryDelete => PALETTE_QUERY_DELETE,
        }
    }

    /// The verb `id` names, or `None` when it is not one of this panel's.
    ///
    /// `None` is routine rather than exceptional: a user's layer may name a
    /// command the palette has never heard of.
    pub(super) fn from_id(id: &CommandId) -> Option<Self> {
        Self::ALL.iter().copied().find(|verb| &verb.id() == id)
    }
}
