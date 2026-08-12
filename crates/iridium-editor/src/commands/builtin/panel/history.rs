//! The undo-tree panel's vocabulary — one screen, eight verbs.
//!
//! A list of every state the document has been in, walked with the arrows and
//! entered with `Enter`. One mode, because there is one screen: the panel shows
//! the tree and nothing else, and there is no field to type into.
//!
//! # ⚠️ `history.togglePanel` is deliberately not here
//!
//! Opening the undo tree is something the editor can be asked from anywhere, so
//! it stays the mode-free host command in [`super::super::host`]. The panel's
//! own way out is [`HISTORY_DISMISS`], which is a different command sharing a
//! chord — exactly as the palette's `dismiss` and `palette.open` do, and for the
//! same reason: rebinding either must leave the other alone.
//!
//! # Why `jump` is one verb and not two
//!
//! `Enter` moves the document to the selected state **and leaves the panel
//! open**, because hopping between two states and watching the document change
//! underneath is what a tree is for. That is one thing the panel does, not a
//! jump followed by a decision not to close, so it is one command a user can
//! rebind rather than two they would have to bind together.

use crate::commands::{CommandCategory, CommandId, CommandMeta, ModeName};

/// The mode the undo-tree panel is in whenever it is open.
pub const HISTORY_MODE: ModeName = ModeName::from_static("history");

/// Close the undo-tree panel.
pub const HISTORY_DISMISS: CommandId = CommandId::from_static("history.dismiss");

/// Move the document to the selected state, leaving the panel open.
pub const HISTORY_JUMP: CommandId = CommandId::from_static("history.jump");

/// Move the undo tree's selection one row up.
pub const HISTORY_SELECT_PREVIOUS: CommandId = CommandId::from_static("history.selectPrevious");

/// Move the undo tree's selection one row down.
pub const HISTORY_SELECT_NEXT: CommandId = CommandId::from_static("history.selectNext");

/// Move the undo tree's selection up by one windowful.
pub const HISTORY_SELECT_PAGE_UP: CommandId = CommandId::from_static("history.selectPageUp");

/// Move the undo tree's selection down by one windowful.
pub const HISTORY_SELECT_PAGE_DOWN: CommandId = CommandId::from_static("history.selectPageDown");

/// Move the undo tree's selection to the oldest state.
pub const HISTORY_SELECT_FIRST: CommandId = CommandId::from_static("history.selectFirst");

/// Move the undo tree's selection to the newest state.
pub const HISTORY_SELECT_LAST: CommandId = CommandId::from_static("history.selectLast");

/// Every undo-tree panel command, in declaration order.
///
/// Ordered as the panel's own documentation table lists them — the two ways out
/// first, then the selection — so a reader comparing the two is comparing like
/// with like.
pub(super) static HISTORY: &[CommandMeta] = &[
    CommandMeta::scoped(
        HISTORY_DISMISS,
        "Close Undo Tree",
        "Closes the undo-tree panel.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
    CommandMeta::scoped(
        HISTORY_JUMP,
        "Jump To Selected State",
        "Moves the document to the selected state, leaving the panel open.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
    CommandMeta::scoped(
        HISTORY_SELECT_PREVIOUS,
        "Select Previous State",
        "Moves the undo tree's selection one row up, stopping at the oldest.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
    CommandMeta::scoped(
        HISTORY_SELECT_NEXT,
        "Select Next State",
        "Moves the undo tree's selection one row down, stopping at the newest.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
    CommandMeta::scoped(
        HISTORY_SELECT_PAGE_UP,
        "Select A Page Of States Up",
        "Moves the undo tree's selection up by one windowful of rows.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
    CommandMeta::scoped(
        HISTORY_SELECT_PAGE_DOWN,
        "Select A Page Of States Down",
        "Moves the undo tree's selection down by one windowful of rows.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
    CommandMeta::scoped(
        HISTORY_SELECT_FIRST,
        "Select Oldest State",
        "Moves the undo tree's selection to the oldest state in the tree.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
    CommandMeta::scoped(
        HISTORY_SELECT_LAST,
        "Select Newest State",
        "Moves the undo tree's selection to the newest state in the tree.",
        CommandCategory::GENERAL,
        HISTORY_MODE,
    ),
];
