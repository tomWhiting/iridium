//! Keyboard input types: key codes, modifiers, events, and handler results.

use serde::{Deserialize, Serialize};

use crate::commands::{CommandArgs, CommandId};
use crate::history::Command;

/// Key codes for keyboard input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyCode {
    /// Alphanumeric and symbol keys
    Char(char),

    /// Left arrow key
    Left,
    /// Right arrow key
    Right,
    /// Up arrow key
    Up,
    /// Down arrow key
    Down,

    /// Home key (go to start of line)
    Home,
    /// End key (go to end of line)
    End,
    /// Page Up key
    PageUp,
    /// Page Down key
    PageDown,

    /// Backspace key (delete character before cursor)
    Backspace,
    /// Delete key (delete character after cursor)
    Delete,
    /// Enter/Return key
    Enter,
    /// Tab key
    Tab,

    /// Shift modifier key
    Shift,
    /// Control modifier key
    Control,
    /// Alt/Option modifier key
    Alt,
    /// Meta/Windows/Command modifier key
    Meta,

    /// Escape key
    Escape,

    /// Function key F1
    F1,
    /// Function key F2
    F2,
    /// Function key F3
    F3,
    /// Function key F4
    F4,
    /// Function key F5
    F5,
    /// Function key F6
    F6,
    /// Function key F7
    F7,
    /// Function key F8
    F8,
    /// Function key F9
    F9,
    /// Function key F10
    F10,
    /// Function key F11
    F11,
    /// Function key F12
    F12,
}

/// Keyboard modifier state.
#[allow(clippy::struct_excessive_bools)] // Mirrors the hardware modifier keys.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Modifiers {
    /// Shift key is held
    pub shift: bool,
    /// Control key is held (Cmd on Mac)
    pub ctrl: bool,
    /// Alt key is held (Option on Mac)
    pub alt: bool,
    /// Meta key is held (Win key, or Cmd on Mac)
    pub meta: bool,
    /// `AltGraph` (`AltGr`) is active.
    ///
    /// On many non-US layouts `AltGr` is reported by the browser as
    /// `ctrl` + `alt` held together while composing a character (e.g. `@`, `€`).
    /// That coincides exactly with the Ctrl+Alt chord used for add-cursor
    /// above/below, so the two are indistinguishable from the `ctrl`/`alt` bits
    /// alone. Hosts that can observe `getModifierState("AltGraph")` set this bit
    /// so modifier chords (see [`super::KeyboardHandler`]) can exclude `AltGr`
    /// and let the character compose untouched. Defaults to `false`.
    #[serde(default)]
    pub alt_graph: bool,
}

impl Modifiers {
    /// No modifiers pressed.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }

    /// Shift only.
    #[must_use]
    pub const fn shift() -> Self {
        Self {
            shift: true,
            ctrl: false,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }

    /// Ctrl only.
    #[must_use]
    pub const fn ctrl() -> Self {
        Self {
            shift: false,
            ctrl: true,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }

    /// Ctrl+Shift.
    #[must_use]
    pub const fn ctrl_shift() -> Self {
        Self {
            shift: true,
            ctrl: true,
            alt: false,
            meta: false,
            alt_graph: false,
        }
    }
}

/// A keyboard event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEvent {
    /// The key that was pressed
    pub key: KeyCode,
    /// Active modifiers
    pub modifiers: Modifiers,
    /// True if this is a key repeat event
    pub is_repeat: bool,
}

impl KeyEvent {
    /// Creates a new key event.
    #[must_use]
    pub const fn new(key: KeyCode, modifiers: Modifiers) -> Self {
        Self {
            key,
            modifiers,
            is_repeat: false,
        }
    }

    /// Creates a new key event with no modifiers.
    #[must_use]
    pub const fn simple(key: KeyCode) -> Self {
        Self::new(key, Modifiers::none())
    }
}

/// Search-related actions triggered by keyboard input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchAction {
    /// Open the search panel (Ctrl+F)
    OpenSearch,
    /// Go to next match (F3 or Enter in search)
    NextMatch,
    /// Go to previous match (Shift+F3)
    PreviousMatch,
    /// Close search panel (Escape when search is open)
    CloseSearch,
}

/// A traversal of the undo tree, requested by a command.
///
/// The keyboard handler cannot perform these itself: it is handed the history
/// by shared reference and never mutates editor state, returning a reversible
/// [`Command`] instead. But an undo is not expressible as a `Command` — it *is*
/// a command, already in the tree, and replaying it means moving the tree's
/// current position, restoring the cursor state recorded there, and refreshing
/// the search and multi-cursor state that hangs off it. So the verb names what
/// it wants and the editor performs it.
///
/// Before this existed, `history.undo` and `history.redo` resolved to
/// [`KeyResult::Handled`] — an acknowledgement — and each face undid by its own
/// route: the web face intercepted `Ctrl+Z` before the keymap entirely. Two
/// consequences followed, and both were live. Running *Undo* from the command
/// palette did nothing, because the palette runs commands and the command did
/// nothing. And the keymap could not rebind undo, because the key never reached
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryRequest {
    /// Step back one edit along the tree's active path.
    Undo,
    /// Step forward one edit along the tree's active path.
    Redo,
    /// Step forward into the branch at `index`, making it the active path.
    RedoBranch(usize),
    /// Make the next sibling of the current node's active child the active one,
    /// without moving. Wraps at the end.
    NextBranch,
    /// Make the previous sibling the active child, without moving. Wraps at the
    /// start.
    PreviousBranch,
}

/// A structural selection change, named rather than performed.
///
/// The keyboard layer cannot carry these out itself for the same reason it
/// cannot carry out an undo: the answer depends on the document's **parse
/// tree**, which lives on the editor, and on the stack of ranges a previous
/// expansion walked through. So the verb names what it wants and the editor
/// performs it — in exactly one place, reached identically by a keystroke and
/// by a command invoked from the palette.
///
/// Every one of these ends as a [`Command::SetSelection`]. That does **not**
/// make them undo steps: `apply_command_internal` records a command only when it
/// changes content, so a selection-only command is applied and never pushed. It
/// is the right behaviour — [`Self::ShrinkSelection`] is the inverse of an
/// expansion, not `Ctrl+Z` — but it is the opposite of what the plan that
/// commissioned this work predicted, so it is written down here rather than left
/// to be rediscovered.
///
/// When the document has no language set, or the tree has not parsed, each of
/// these does nothing at all. That is not an error: it is what "this file has
/// no structure to navigate" looks like, and a key that quietly does nothing is
/// better than one that reports a failure the person cannot act on.
///
/// # Three kinds of request
///
/// The distinction matters because it decides what happens to the expansion
/// stack, and getting it wrong makes shrinking jump somewhere nobody asked for.
///
/// - **Widening** ([`Self::SelectNode`], [`Self::ExpandSelection`]) pushes the
///   state it left onto the stack, so a later shrink can restore it exactly.
/// - **Retracing** ([`Self::ShrinkSelection`]) pops one.
/// - **Moving** — everything else — *clears* the stack. Once the selection has
///   walked sideways to a sibling or down into a child, the state expansion
///   started from is no longer where the person wants "back" to go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AstRequest {
    /// Snap every selection to the smallest node that covers it.
    SelectNode,
    /// Widen every selection to the smallest node that strictly contains it.
    ExpandSelection,
    /// Undo one expansion, restoring the selections exactly as they were.
    ShrinkSelection,

    /// Select the next node beside the current one, climbing when it is last.
    SelectNextSibling,
    /// Select the previous node beside the current one, climbing when it is first.
    SelectPreviousSibling,
    /// Select the first child of the node under each selection.
    SelectFirstChild,
    /// Select the last child of the node under each selection.
    SelectLastChild,

    /// Grow each selection to also cover the node after it.
    ///
    /// Distinct from [`Self::SelectNextSibling`], which moves. This is the verb
    /// for picking up three array elements one press at a time.
    ExtendNextSibling,
    /// Grow each selection to also cover the node before it.
    ExtendPreviousSibling,

    /// Collapse each selection to the start of the node it sits in.
    ///
    /// Repeated presses walk outward — token, expression, statement — because
    /// the node chosen is the smallest one starting *strictly* before the caret.
    CursorNodeStart,
    /// Collapse each selection to the end of the node it sits in, walking
    /// outward on repeated presses in the same way.
    CursorNodeEnd,

    /// Put a cursor on every node beside the current one, including it.
    ///
    /// The verb for editing every element of an array at once. Siblings are
    /// disjoint, so the auto-merge in
    /// [`CursorState::add_cursor`](crate::document::CursorState::add_cursor)
    /// has nothing to do and the cursor count is exactly the sibling count.
    CursorOnEverySibling,
    /// Put a cursor on every child of the node under each selection.
    CursorOnEveryChild,
}

/// Result of handling a keyboard event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyResult {
    /// The event was handled, no further action needed
    Handled,

    /// The event was handled and a command was produced
    Command(Command),

    /// The event triggered a clipboard operation
    Clipboard(ClipboardOperation),

    /// The event triggered a search action (T121, T122)
    Search(SearchAction),

    /// The event asked for a traversal of the undo tree.
    ///
    /// See [`HistoryRequest`] for why this is a request rather than a
    /// [`Self::Command`].
    History(HistoryRequest),

    /// The event asked for a selection change driven by the parse tree.
    ///
    /// See [`AstRequest`] for why this is a request rather than a
    /// [`Self::Command`].
    Ast(AstRequest),

    /// A binding resolved to a command this kernel does not implement.
    ///
    /// The keypress was **consumed** and the command named: it is the caller's to
    /// run. This is the whole extensibility path for commands contributed from
    /// outside the kernel — a host registers a [`CommandMeta`](crate::CommandMeta)
    /// so the palette lists it, binds it in a pushed
    /// [`Keymap`](crate::Keymap), and receives this when the binding fires.
    ///
    /// Reporting it is not optional politeness. Before this variant existed the id
    /// was dropped and the key reported as [`Self::Ignored`], which is
    /// indistinguishable from a meaningless keypress; for a multi-stroke sequence
    /// the earlier strokes had already returned [`Self::Handled`], so the host
    /// could not even reconstruct which sequence had completed.
    HostCommand {
        /// The resolved command id.
        command: CommandId,
        /// The count and captured characters the key sequence carried.
        args: CommandArgs,
    },

    /// The event was not handled (pass to next handler)
    Ignored,
}

/// Why a command named by id could not be run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandRunError {
    /// The kernel implements no command with this id.
    ///
    /// Not necessarily a mistake: the id may be a host command, registered in the
    /// [`CommandRegistry`](crate::CommandRegistry) and implemented outside the
    /// kernel. The caller is the one that knows, which is why this is an error
    /// value rather than a silent no-op.
    #[error("the editing kernel implements no command with id `{id}`")]
    Unimplemented {
        /// The id that has no kernel implementation.
        id: String,
    },
}

/// Clipboard operation requested by keyboard handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardOperation {
    /// Copy selected text
    Copy(String),
    /// Cut selected text (returns text to copy and command to delete)
    Cut {
        /// The text to copy to clipboard
        text: String,
        /// The command to apply to delete the selection
        command: Command,
    },
    /// Request paste from clipboard
    Paste,
}
