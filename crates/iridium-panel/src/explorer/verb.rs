//! The explorer's verbs, as a type the dispatch can match exhaustively.
//!
//! A resolved binding names a [`CommandId`], which is a string. Dispatching on
//! the string directly would mean writing every id a second time, in a second
//! file, with nothing to keep the two spellings in step — and this repository
//! has been bitten by exactly that shape before.
//!
//! [`Verb`] is the one-to-one mapping. Every variant knows its id, the id is the
//! kernel constant and never a literal retyped here, and the dispatch matches on
//! the enum — so a verb added to the kernel table and forgotten in the panel is
//! a **compile error** rather than a key that quietly does nothing.
//!
//! # ⚠️ The two toggles are deliberately absent
//!
//! `explorer.togglePanel` and `explorer.toggleSidebar` are editor-wide host
//! commands the panel merely *answers* while it has focus, not verbs it owns.
//! They map onto [`ExplorerOutcome`](super::panel::ExplorerOutcome) variants the
//! face acts on, and are handled beside this enum rather than inside it, because
//! putting them here would claim the panel owns two ids the palette also offers.

use iridium_editor::CommandId;
use iridium_editor::commands::builtin::{
    EXPLORER_ACTIVATE, EXPLORER_BEGIN_EDIT, EXPLORER_COLLAPSE, EXPLORER_CONFIRM_APPLY,
    EXPLORER_CONFIRM_CANCEL, EXPLORER_DISMISS, EXPLORER_EDIT_ASK_APPLY, EXPLORER_EDIT_BACKSPACE,
    EXPLORER_EDIT_CARET_END, EXPLORER_EDIT_CARET_HOME, EXPLORER_EDIT_CARET_LEFT,
    EXPLORER_EDIT_CARET_RIGHT, EXPLORER_EDIT_CURSOR_DOWN, EXPLORER_EDIT_CURSOR_UP,
    EXPLORER_EDIT_DELETE, EXPLORER_EDIT_LEAVE, EXPLORER_EDIT_NEW_ROW, EXPLORER_EDIT_STRIKE_ROW,
    EXPLORER_EXPAND, EXPLORER_MOVE_DOWN, EXPLORER_MOVE_TO_FIRST, EXPLORER_MOVE_TO_LAST,
    EXPLORER_MOVE_UP, EXPLORER_QUERY_BACKSPACE, EXPLORER_REFUSED_DISMISS, EXPLORER_ROOT_ABOVE,
    EXPLORER_ROOT_AT_SELECTION, EXPLORER_TOGGLE_HIDDEN,
};

/// One thing the explorer can be asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Verb {
    /// Clear the query, or close the panel when there is none.
    Dismiss,
    /// Open the selected file, or toggle the selected folder.
    Activate,
    /// Begin editing the rows as text.
    BeginEdit,
    /// Move the selection one row up.
    MoveUp,
    /// Move the selection one row down.
    MoveDown,
    /// Select the first selectable row.
    MoveToFirst,
    /// Select the last selectable row.
    MoveToLast,
    /// Open the selected folder, or step into an open one.
    Expand,
    /// Close the selected folder, or step out to its parent.
    Collapse,
    /// Make the selected folder the root.
    RootAtSelection,
    /// Make the folder above the root the root.
    RootAbove,
    /// Show dot-prefixed entries, or stop showing them.
    ToggleHidden,
    /// Delete the character before the caret in the query.
    QueryBackspace,
    /// Stop editing, asking about unapplied work first.
    EditLeave,
    /// Strike the row under the cursor through, or unstrike it.
    EditStrikeRow,
    /// Type a new, empty row below the cursor.
    EditNewRow,
    /// Work out what the rows say, and show it for confirmation.
    EditAskApply,
    /// Move the editing cursor one row up.
    EditCursorUp,
    /// Move the editing cursor one row down.
    EditCursorDown,
    /// Move the caret one character left within the row.
    EditCaretLeft,
    /// Move the caret one character right within the row.
    EditCaretRight,
    /// Move the caret to the start of the row.
    EditCaretHome,
    /// Move the caret to the end of the row.
    EditCaretEnd,
    /// Delete before the caret, or take back an emptied typed row.
    EditBackspace,
    /// Delete after the caret within the row.
    EditDelete,
    /// Carry out the confirmed plan.
    ConfirmApply,
    /// Go back to the rows without applying.
    ConfirmCancel,
    /// Dismiss the refusals and go back to the rows.
    RefusedDismiss,
}

impl Verb {
    /// Every verb, in the order the panel's documentation tables list them.
    ///
    /// The single place the set is enumerated: [`Self::id`] is matched
    /// exhaustively against it, and the panel's tests walk it to prove every
    /// verb is both bound by the default keymap and reachable from a
    /// configuration file.
    pub(super) const ALL: &'static [Self] = &[
        Self::Dismiss,
        Self::Activate,
        Self::BeginEdit,
        Self::MoveUp,
        Self::MoveDown,
        Self::MoveToFirst,
        Self::MoveToLast,
        Self::Expand,
        Self::Collapse,
        Self::RootAtSelection,
        Self::RootAbove,
        Self::ToggleHidden,
        Self::QueryBackspace,
        Self::EditLeave,
        Self::EditStrikeRow,
        Self::EditNewRow,
        Self::EditAskApply,
        Self::EditCursorUp,
        Self::EditCursorDown,
        Self::EditCaretLeft,
        Self::EditCaretRight,
        Self::EditCaretHome,
        Self::EditCaretEnd,
        Self::EditBackspace,
        Self::EditDelete,
        Self::ConfirmApply,
        Self::ConfirmCancel,
        Self::RefusedDismiss,
    ];

    /// The command id this verb is.
    ///
    /// Every arm answers with the kernel's own constant, so the id has exactly
    /// one spelling in the repository and this file cannot drift from it.
    pub(super) const fn id(self) -> CommandId {
        match self {
            Self::Dismiss => EXPLORER_DISMISS,
            Self::Activate => EXPLORER_ACTIVATE,
            Self::BeginEdit => EXPLORER_BEGIN_EDIT,
            Self::MoveUp => EXPLORER_MOVE_UP,
            Self::MoveDown => EXPLORER_MOVE_DOWN,
            Self::MoveToFirst => EXPLORER_MOVE_TO_FIRST,
            Self::MoveToLast => EXPLORER_MOVE_TO_LAST,
            Self::Expand => EXPLORER_EXPAND,
            Self::Collapse => EXPLORER_COLLAPSE,
            Self::RootAtSelection => EXPLORER_ROOT_AT_SELECTION,
            Self::RootAbove => EXPLORER_ROOT_ABOVE,
            Self::ToggleHidden => EXPLORER_TOGGLE_HIDDEN,
            Self::QueryBackspace => EXPLORER_QUERY_BACKSPACE,
            Self::EditLeave => EXPLORER_EDIT_LEAVE,
            Self::EditStrikeRow => EXPLORER_EDIT_STRIKE_ROW,
            Self::EditNewRow => EXPLORER_EDIT_NEW_ROW,
            Self::EditAskApply => EXPLORER_EDIT_ASK_APPLY,
            Self::EditCursorUp => EXPLORER_EDIT_CURSOR_UP,
            Self::EditCursorDown => EXPLORER_EDIT_CURSOR_DOWN,
            Self::EditCaretLeft => EXPLORER_EDIT_CARET_LEFT,
            Self::EditCaretRight => EXPLORER_EDIT_CARET_RIGHT,
            Self::EditCaretHome => EXPLORER_EDIT_CARET_HOME,
            Self::EditCaretEnd => EXPLORER_EDIT_CARET_END,
            Self::EditBackspace => EXPLORER_EDIT_BACKSPACE,
            Self::EditDelete => EXPLORER_EDIT_DELETE,
            Self::ConfirmApply => EXPLORER_CONFIRM_APPLY,
            Self::ConfirmCancel => EXPLORER_CONFIRM_CANCEL,
            Self::RefusedDismiss => EXPLORER_REFUSED_DISMISS,
        }
    }

    /// The verb `id` names, or `None` when it is not one of this panel's.
    ///
    /// `None` is routine rather than exceptional: the panel's keymap can hold
    /// the two editor-wide toggle commands, and a user's layer may name a
    /// command the panel has never heard of.
    pub(super) fn from_id(id: &CommandId) -> Option<Self> {
        Self::ALL.iter().copied().find(|verb| &verb.id() == id)
    }
}
