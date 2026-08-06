//! Commands the kernel names *and* implements — one level above the editor.
//!
//! These are a third category, and the distinction from
//! [`host`](super::host) is worth keeping straight because it decides who
//! writes the behaviour:
//!
//! - a **built-in** acts on one [`Editor`](crate::Editor) and is implemented
//!   by a `KeyboardAction`;
//! - a **host command** cannot be implemented by the kernel at all — the
//!   kernel cannot draw a palette — so each face writes its own;
//! - a **workspace command** acts on the
//!   [`Workspace`](crate::workspace::Workspace), which owns the editors. The
//!   kernel implements it, in
//!   [`Workspace::run_command`](crate::workspace::Workspace::run_command);
//!   the face merely forwards.
//!
//! **Getting that third case wrong is what makes faces diverge.** If
//! "switch to the next tab" were a host command, the terminal face and the
//! web face would each write their own idea of what "next" means across a
//! nested group — and one of them would be wrong in a way nobody noticed
//! until both were shipped. The kernel already knows; it should answer.
//!
//! # How one runs
//!
//! Nothing here appears in [`BUILTIN`](super::BUILTIN) and nothing here has
//! a `KeyboardAction`, for the same mechanical reason as a host command: the
//! action table is exhaustively matched against the built-ins, and a
//! workspace command has no editor-level implementation to name. A matching
//! keypress therefore resolves to
//! [`KeyResult::HostCommand`](crate::input::KeyResult::HostCommand) exactly
//! as a host command does — and the face's one job is to hand the id to
//! `Workspace::run_command` instead of implementing it.

use crate::commands::{CommandCategory, CommandId, CommandMeta, CommandRegistry, RegistryError};

/// Activate the next tab in display order.
pub const WORKSPACE_NEXT_TAB: CommandId = CommandId::from_static("workspace.nextTab");

/// Activate the previous tab in display order.
pub const WORKSPACE_PREVIOUS_TAB: CommandId = CommandId::from_static("workspace.previousTab");

/// Activate the first tab in display order.
pub const WORKSPACE_FIRST_TAB: CommandId = CommandId::from_static("workspace.firstTab");

/// Activate the last tab in display order.
pub const WORKSPACE_LAST_TAB: CommandId = CommandId::from_static("workspace.lastTab");

/// Close the active tab.
///
/// Closes the *tab*, not the document: a file open in two places loses one
/// view and keeps its buffer, cursors and undo history.
pub const WORKSPACE_CLOSE_TAB: CommandId = CommandId::from_static("workspace.closeTab");

/// Every workspace command the kernel names, in declaration order.
pub static WORKSPACE: &[CommandMeta] = &[
    CommandMeta::described(
        WORKSPACE_NEXT_TAB,
        "Next Tab",
        "Moves to the next open tab, continuing past the end of a group.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["next buffer", "forward tab"]),
    CommandMeta::described(
        WORKSPACE_PREVIOUS_TAB,
        "Previous Tab",
        "Moves to the previous open tab, continuing past the start of a group.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["previous buffer", "back tab"]),
    CommandMeta::described(
        WORKSPACE_FIRST_TAB,
        "First Tab",
        "Moves to the first open tab.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        WORKSPACE_LAST_TAB,
        "Last Tab",
        "Moves to the last open tab.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        WORKSPACE_CLOSE_TAB,
        "Close Tab",
        "Closes the active tab, keeping the file open if another tab still shows it.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["close buffer", "close file"]),
];

/// The number of workspace commands the kernel names.
///
/// Derived from [`WORKSPACE`], so it cannot disagree with the table.
pub const WORKSPACE_COMMAND_COUNT: usize = WORKSPACE.len();

/// Returns every workspace command's metadata, in declaration order.
///
/// Prefer [`workspace_command_metas`] when a borrow will do; this clones.
#[must_use]
pub fn workspace_commands() -> Vec<CommandMeta> {
    WORKSPACE.to_vec()
}

/// Borrows every workspace command's metadata, in declaration order.
#[must_use]
pub const fn workspace_command_metas() -> &'static [CommandMeta] {
    WORKSPACE
}

/// Registers every workspace command into `registry`.
///
/// # Errors
///
/// [`RegistryError::DuplicateId`] if `registry` already holds one of these
/// ids, which means a face claimed an id the kernel had already named.
pub fn register_workspace_commands(registry: &mut CommandRegistry) -> Result<(), RegistryError> {
    registry.register_all(workspace_commands())
}
