//! The right-click context menu's vocabulary — one screen, six verbs.
//!
//! A fixed list of rows walked with the arrows and entered with `Enter`. One
//! mode, because there is one screen: the menu shows its rows and nothing else,
//! and there is no field to type into.
//!
//! # ⚠️ These are the verbs that *drive* the menu, not the verbs it *offers*
//!
//! The menu's rows run Cut, Copy, Paste, Select All and `palette.open` — kernel
//! commands it merely lists, resolved against the registry when it opens. None
//! of those belong here. This table is the six things a user does *to the menu*,
//! and they are the only things a `[keys]` line could sensibly rebind inside it.
//!
//! # ⚠️ `contextMenu.*`, deliberately not `menu.*`
//!
//! The menubar is a second panel with its own navigation, and it will need its
//! own mode and its own table. Taking the shorter namespace for whichever panel
//! happened to be converted first would have forced the other into a worse name
//! — or, worse, into sharing a mode with a screen it is not. `menuBar.*` is left
//! free on purpose.
//!
//! # ⚠️ There is no `contextMenu.open`
//!
//! The menu opens on a right-click, not a chord, so there is no binding to name
//! and nothing for a user to rebind. [`CONTEXT_MENU_DISMISS`] is the panel's own
//! way out and exists because `Escape` *is* a key.

use crate::commands::{CommandCategory, CommandId, CommandMeta, ModeName};

/// The mode the context menu is in whenever it is up.
pub const CONTEXT_MENU_MODE: ModeName = ModeName::from_static("contextMenu");

/// Close the context menu without running anything.
pub const CONTEXT_MENU_DISMISS: CommandId = CommandId::from_static("contextMenu.dismiss");

/// Run the highlighted row and close the menu.
pub const CONTEXT_MENU_ACCEPT: CommandId = CommandId::from_static("contextMenu.accept");

/// Move the highlight one runnable row up.
pub const CONTEXT_MENU_SELECT_PREVIOUS: CommandId =
    CommandId::from_static("contextMenu.selectPrevious");

/// Move the highlight one runnable row down.
pub const CONTEXT_MENU_SELECT_NEXT: CommandId = CommandId::from_static("contextMenu.selectNext");

/// Move the highlight to the first row that can run.
pub const CONTEXT_MENU_SELECT_FIRST: CommandId = CommandId::from_static("contextMenu.selectFirst");

/// Move the highlight to the last row that can run.
pub const CONTEXT_MENU_SELECT_LAST: CommandId = CommandId::from_static("contextMenu.selectLast");

/// Every context-menu command, in declaration order.
///
/// Ordered as the panel's own documentation table lists them — the two ways out
/// first, then the selection — so a reader comparing the two is comparing like
/// with like.
pub(super) static CONTEXT_MENU: &[CommandMeta] = &[
    CommandMeta::scoped(
        CONTEXT_MENU_DISMISS,
        "Close Context Menu",
        "Closes the context menu without running anything.",
        CommandCategory::GENERAL,
        CONTEXT_MENU_MODE,
    ),
    CommandMeta::scoped(
        CONTEXT_MENU_ACCEPT,
        "Run Highlighted Menu Row",
        "Runs the highlighted context-menu row and closes the menu.",
        CommandCategory::GENERAL,
        CONTEXT_MENU_MODE,
    ),
    CommandMeta::scoped(
        CONTEXT_MENU_SELECT_PREVIOUS,
        "Select Previous Menu Row",
        "Moves the highlight one runnable row up, stopping at the first.",
        CommandCategory::GENERAL,
        CONTEXT_MENU_MODE,
    ),
    CommandMeta::scoped(
        CONTEXT_MENU_SELECT_NEXT,
        "Select Next Menu Row",
        "Moves the highlight one runnable row down, stopping at the last.",
        CommandCategory::GENERAL,
        CONTEXT_MENU_MODE,
    ),
    CommandMeta::scoped(
        CONTEXT_MENU_SELECT_FIRST,
        "Select First Menu Row",
        "Moves the highlight to the first context-menu row that can run.",
        CommandCategory::GENERAL,
        CONTEXT_MENU_MODE,
    ),
    CommandMeta::scoped(
        CONTEXT_MENU_SELECT_LAST,
        "Select Last Menu Row",
        "Moves the highlight to the last context-menu row that can run.",
        CommandCategory::GENERAL,
        CONTEXT_MENU_MODE,
    ),
];
