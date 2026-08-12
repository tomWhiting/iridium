//! The command palette's vocabulary — one screen, twelve verbs.
//!
//! The palette is the panel with the fewest screens and the most reach: it is a
//! query field, a ranked list, and a way to run whichever entry is selected. One
//! mode covers all of it, because unlike the explorer there is no second screen
//! that disagrees about what a printable character means — every character the
//! palette is given narrows the list, always.
//!
//! # ⚠️ `palette.open` is deliberately not here
//!
//! Opening the palette is something the editor can be asked from anywhere, so it
//! stays the mode-free host command in [`super::super::host`]. The same chord
//! doing both — `⌘K` opens it and closes it again — is *two* commands, which is
//! exactly what the two scopes make possible: rebinding either leaves the other
//! alone. A user who wants `⌘P` to open the palette and `Escape` alone to close
//! it can have that, and neither line has to mention the other.
//!
//! # Why the caret verbs are the palette's and not the editor's
//!
//! `edit.caretLeft` moves a caret in a document; `palette.caretLeft` moves the
//! caret in a query field that is not a document and has no selection, no undo
//! and no multi-cursor. They look alike and are not the same verb, and binding
//! the document one into this mode would name a command the panel cannot run.

use crate::commands::{CommandCategory, CommandId, CommandMeta, ModeName};

/// The mode the command palette is in whenever it is open.
///
/// One mode, because there is one screen. See the module documentation for why
/// that is a fact about the palette rather than a simplification of it.
pub const PALETTE_MODE: ModeName = ModeName::from_static("palette");

/// Close the command palette without running anything.
pub const PALETTE_DISMISS: CommandId = CommandId::from_static("palette.dismiss");

/// Run the selected command and close the palette.
pub const PALETTE_ACCEPT: CommandId = CommandId::from_static("palette.accept");

/// Move the palette's selection one entry up.
pub const PALETTE_SELECT_PREVIOUS: CommandId = CommandId::from_static("palette.selectPrevious");

/// Move the palette's selection one entry down.
pub const PALETTE_SELECT_NEXT: CommandId = CommandId::from_static("palette.selectNext");

/// Move the palette's selection up by one windowful.
pub const PALETTE_SELECT_PAGE_UP: CommandId = CommandId::from_static("palette.selectPageUp");

/// Move the palette's selection down by one windowful.
pub const PALETTE_SELECT_PAGE_DOWN: CommandId = CommandId::from_static("palette.selectPageDown");

/// Move the caret one character left within the palette's query.
pub const PALETTE_CARET_LEFT: CommandId = CommandId::from_static("palette.caretLeft");

/// Move the caret one character right within the palette's query.
pub const PALETTE_CARET_RIGHT: CommandId = CommandId::from_static("palette.caretRight");

/// Move the caret to the start of the palette's query.
pub const PALETTE_CARET_HOME: CommandId = CommandId::from_static("palette.caretHome");

/// Move the caret to the end of the palette's query.
pub const PALETTE_CARET_END: CommandId = CommandId::from_static("palette.caretEnd");

/// Delete the character before the caret in the palette's query.
pub const PALETTE_QUERY_BACKSPACE: CommandId = CommandId::from_static("palette.queryBackspace");

/// Delete the character after the caret in the palette's query.
pub const PALETTE_QUERY_DELETE: CommandId = CommandId::from_static("palette.queryDelete");

/// Every command palette command, in declaration order.
///
/// Ordered as the panel's own documentation table lists them — the two ways out
/// first, then the selection, then the query — so a reader comparing the two is
/// comparing like with like.
pub(super) static PALETTE: &[CommandMeta] = &[
    CommandMeta::scoped(
        PALETTE_DISMISS,
        "Close Command Palette",
        "Closes the command palette without running anything.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_ACCEPT,
        "Run Selected Command",
        "Runs the command the palette has selected, and closes the palette.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_SELECT_PREVIOUS,
        "Select Previous Command",
        "Moves the palette's selection one entry up, stopping at the first.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_SELECT_NEXT,
        "Select Next Command",
        "Moves the palette's selection one entry down, stopping at the last.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_SELECT_PAGE_UP,
        "Select A Page Up",
        "Moves the palette's selection up by one windowful of entries.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_SELECT_PAGE_DOWN,
        "Select A Page Down",
        "Moves the palette's selection down by one windowful of entries.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_CARET_LEFT,
        "Move Caret Left In Query",
        "Moves the caret one character left within the palette's query.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_CARET_RIGHT,
        "Move Caret Right In Query",
        "Moves the caret one character right within the palette's query.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_CARET_HOME,
        "Move Caret To Query Start",
        "Moves the caret to the start of the palette's query.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_CARET_END,
        "Move Caret To Query End",
        "Moves the caret to the end of the palette's query.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_QUERY_BACKSPACE,
        "Delete In Query",
        "Deletes the character before the caret in the palette's query.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
    CommandMeta::scoped(
        PALETTE_QUERY_DELETE,
        "Delete Forward In Query",
        "Deletes the character after the caret in the palette's query.",
        CommandCategory::GENERAL,
        PALETTE_MODE,
    ),
];
