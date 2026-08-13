//! The overlay's verbs, as a type the dispatch can match exhaustively.
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
//! # ⚠️ Two of these verbs are borrowed document commands
//!
//! [`SEARCH_NEXT_MATCH`] and [`SEARCH_PREVIOUS_MATCH`] are **mode-free editor
//! commands**, not members of the kernel's `search` panel table. The overlay's
//! `Enter` calls [`Editor::goto_next_match`](iridium_editor::Editor::goto_next_match),
//! which *is* that command — so it names it rather than minting a scoped twin
//! that would give a user two ids for one behaviour.
//!
//! They are listed in [`Verb::BORROWED`] so the panel's tests can assert the two
//! halves of the vocabulary separately: everything in the kernel's `search`
//! table is answered here, and each borrowed id is registered and is mode-free.
//! A completeness test that enumerates "everything" breaks the moment there is a
//! second everything.

use iridium_editor::CommandId;
use iridium_editor::commands::builtin::{
    SEARCH_CARET_END, SEARCH_CARET_HOME, SEARCH_CARET_LEFT, SEARCH_CARET_RIGHT, SEARCH_DISMISS,
    SEARCH_FIELD_BACKSPACE, SEARCH_FIELD_DELETE, SEARCH_NEXT_MATCH, SEARCH_PREVIOUS_MATCH,
    SEARCH_REPLACE_ALL, SEARCH_REPLACE_CURRENT, SEARCH_TOGGLE_CASE_SENSITIVE, SEARCH_TOGGLE_FIELD,
    SEARCH_TOGGLE_REGEX, SEARCH_TOGGLE_WHOLE_WORD,
};

/// One thing the search overlay can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    /// Close the overlay, ending the search.
    Dismiss,
    /// Move the document to the next match. ⚠️ A borrowed document command.
    NextMatch,
    /// Move the document to the previous match. ⚠️ A borrowed document command.
    PreviousMatch,
    /// Move the focus between the query and the replacement.
    ToggleField,
    /// Move the caret one character left within the focused field.
    CaretLeft,
    /// Move the caret one character right within the focused field.
    CaretRight,
    /// Move the caret to the start of the focused field.
    CaretHome,
    /// Move the caret to the end of the focused field.
    CaretEnd,
    /// Delete the character before the caret in the focused field.
    FieldBackspace,
    /// Delete the character after the caret in the focused field.
    FieldDelete,
    /// Replace the match the document is currently sitting on.
    ReplaceCurrent,
    /// Replace every match in the document.
    ReplaceAll,
    /// Turn case-sensitive matching on or off.
    ToggleCaseSensitive,
    /// Turn whole-word matching on or off.
    ToggleWholeWord,
    /// Turn regular-expression matching on or off.
    ToggleRegex,
}

impl Verb {
    /// Every verb, in the order the overlay's documentation table lists them.
    ///
    /// The single place the set is enumerated: [`Self::id`] is matched
    /// exhaustively against it, and the panel's tests walk it to prove every
    /// verb is both bound by the default keymap and reachable from a
    /// configuration file.
    pub(super) const ALL: &'static [Self] = &[
        Self::Dismiss,
        Self::NextMatch,
        Self::PreviousMatch,
        Self::ToggleField,
        Self::CaretLeft,
        Self::CaretRight,
        Self::CaretHome,
        Self::CaretEnd,
        Self::FieldBackspace,
        Self::FieldDelete,
        Self::ReplaceCurrent,
        Self::ReplaceAll,
        Self::ToggleCaseSensitive,
        Self::ToggleWholeWord,
        Self::ToggleRegex,
    ];

    /// The verbs whose ids belong to the **editor**, not to the kernel's
    /// `search` panel table.
    ///
    /// Named here rather than left implicit so the vocabulary ratchet can
    /// subtract them by name instead of by a count that would drift. See the
    /// module documentation for why they are borrowed rather than duplicated.
    #[cfg(test)]
    pub(super) const BORROWED: &'static [Self] = &[Self::NextMatch, Self::PreviousMatch];

    /// The command id this verb is.
    ///
    /// Every arm answers with the kernel's own constant, so the id has exactly
    /// one spelling in the repository and this file cannot drift from it.
    pub(super) const fn id(self) -> CommandId {
        match self {
            Self::Dismiss => SEARCH_DISMISS,
            Self::NextMatch => SEARCH_NEXT_MATCH,
            Self::PreviousMatch => SEARCH_PREVIOUS_MATCH,
            Self::ToggleField => SEARCH_TOGGLE_FIELD,
            Self::CaretLeft => SEARCH_CARET_LEFT,
            Self::CaretRight => SEARCH_CARET_RIGHT,
            Self::CaretHome => SEARCH_CARET_HOME,
            Self::CaretEnd => SEARCH_CARET_END,
            Self::FieldBackspace => SEARCH_FIELD_BACKSPACE,
            Self::FieldDelete => SEARCH_FIELD_DELETE,
            Self::ReplaceCurrent => SEARCH_REPLACE_CURRENT,
            Self::ReplaceAll => SEARCH_REPLACE_ALL,
            Self::ToggleCaseSensitive => SEARCH_TOGGLE_CASE_SENSITIVE,
            Self::ToggleWholeWord => SEARCH_TOGGLE_WHOLE_WORD,
            Self::ToggleRegex => SEARCH_TOGGLE_REGEX,
        }
    }

    /// The verb `id` names, or `None` when it is not one of this overlay's.
    ///
    /// `None` is routine rather than exceptional: a user's layer may name a
    /// command the overlay has never heard of.
    pub(super) fn from_id(id: &CommandId) -> Option<Self> {
        Self::ALL.iter().copied().find(|verb| &verb.id() == id)
    }
}
