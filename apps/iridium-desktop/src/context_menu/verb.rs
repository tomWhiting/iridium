//! The menu's verbs, as a type the dispatch can match exhaustively.
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
//! # ⚠️ These are not the verbs the menu *offers*
//!
//! The menu's rows run Cut, Copy, Paste, Select All and `palette.open`. Those
//! are its content, resolved against the registry when it opens, and they leave
//! through [`MenuOutcome::Run`](super::MenuOutcome::Run). What is here is the
//! six things a user does *to the menu*, which are the only things a `[keys]`
//! line could sensibly rebind inside it.

use iridium_editor::CommandId;
use iridium_editor::commands::builtin::{
    CONTEXT_MENU_ACCEPT, CONTEXT_MENU_DISMISS, CONTEXT_MENU_SELECT_FIRST, CONTEXT_MENU_SELECT_LAST,
    CONTEXT_MENU_SELECT_NEXT, CONTEXT_MENU_SELECT_PREVIOUS,
};

/// One thing the context menu can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    /// Close the menu without running anything.
    Dismiss,
    /// Run the highlighted row and close.
    Accept,
    /// Move the highlight one runnable row up.
    SelectPrevious,
    /// Move the highlight one runnable row down.
    SelectNext,
    /// Move the highlight to the first row that can run.
    SelectFirst,
    /// Move the highlight to the last row that can run.
    SelectLast,
}

impl Verb {
    /// Every verb, in the order the menu's documentation table lists them.
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
        Self::SelectFirst,
        Self::SelectLast,
    ];

    /// The command id this verb is.
    ///
    /// Every arm answers with the kernel's own constant, so the id has exactly
    /// one spelling in the repository and this file cannot drift from it.
    pub(super) const fn id(self) -> CommandId {
        match self {
            Self::Dismiss => CONTEXT_MENU_DISMISS,
            Self::Accept => CONTEXT_MENU_ACCEPT,
            Self::SelectPrevious => CONTEXT_MENU_SELECT_PREVIOUS,
            Self::SelectNext => CONTEXT_MENU_SELECT_NEXT,
            Self::SelectFirst => CONTEXT_MENU_SELECT_FIRST,
            Self::SelectLast => CONTEXT_MENU_SELECT_LAST,
        }
    }

    /// The verb `id` names, or `None` when it is not one of this menu's.
    ///
    /// `None` is routine rather than exceptional: a user's layer may name a
    /// command the menu has never heard of — including one of the commands the
    /// menu itself *offers*, which it lists but does not answer as a key.
    pub(super) fn from_id(id: &CommandId) -> Option<Self> {
        Self::ALL.iter().copied().find(|verb| &verb.id() == id)
    }
}
