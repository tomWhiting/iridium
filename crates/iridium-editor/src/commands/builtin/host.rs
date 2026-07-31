//! Commands the kernel *names* but does not implement.
//!
//! A host command is a real, registered, bindable command whose behaviour lives
//! outside the kernel. Opening a command palette is the archetype: the kernel
//! cannot draw a UI, but it can — and must — own the *identity* of the action, so
//! that one id and one key sequence mean the same thing in every face.
//!
//! The alternative was to let each face register its own id. That is how three
//! faces end up with `palette.open`, `commandPalette.show` and `ui.palette`, and
//! how a keymap stops being portable between them. Naming it here costs one table
//! and makes the terminal face inherit the binding for free — which is the whole
//! argument for building features before faces.
//!
//! # How one runs
//!
//! Nothing here appears in [`BUILTIN`](super::BUILTIN), and nothing here has a
//! `KeyboardAction`: the action table is exhaustively matched against the
//! built-ins, and adding a host command to it would be a compile error asking for
//! an implementation the kernel cannot write. Instead:
//!
//! - a matching keypress resolves to
//!   [`KeyResult::HostCommand`](crate::input::KeyResult::HostCommand), naming the
//!   id and its arguments, and the face runs its own behaviour;
//! - `Editor::run_command` returns
//!   [`CommandRunError::Unimplemented`](crate::input::CommandRunError::Unimplemented),
//!   which is the same statement in the other direction — the kernel is telling
//!   the caller that this one is theirs.
//!
//! Both are *reports*, not failures. A face that ignores them silently drops the
//! command, which is the bug this module's registration is meant to make visible.

use crate::commands::{CommandCategory, CommandId, CommandMeta, CommandRegistry, RegistryError};

/// Open the command palette.
///
/// Bound to `Ctrl+K`, `Ctrl+P` and `Ctrl+Shift+P` by the default keymap. The
/// kernel resolves the key and reports the command; the face opens the UI and
/// drives [`palette::search`](crate::commands::palette::search) as the user
/// types.
pub const PALETTE_OPEN: CommandId = CommandId::from_static("palette.open");

/// Show or hide the undo-tree panel.
///
/// Bound to `Ctrl+Alt+H` by the default keymap, joining
/// [`HISTORY_PREVIOUS_BRANCH`](super::HISTORY_PREVIOUS_BRANCH) and
/// [`HISTORY_NEXT_BRANCH`](super::HISTORY_NEXT_BRANCH) on `Ctrl+Alt`.
///
/// A *toggle* rather than an open, because unlike the palette this panel has no
/// query to abandon: pressing the key again is how you put it away, and a face
/// that only ever opened it would need a second id to close it.
///
/// The kernel names it and reports it; the face owns the drawing, and reads the
/// tree through [`Editor::history_snapshot`](crate::Editor::history_snapshot).
pub const HISTORY_TOGGLE_PANEL: CommandId = CommandId::from_static("history.togglePanel");

/// Every host command the kernel names, in declaration order.
pub static HOST: &[CommandMeta] = &[
    CommandMeta::described(
        PALETTE_OPEN,
        "Show All Commands",
        "Opens the command palette, which runs any command by name.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["command palette", "palette"]),
    CommandMeta::described(
        HISTORY_TOGGLE_PANEL,
        "Toggle Undo Tree",
        "Shows or hides the undo tree, where every branch of the history can be reached.",
        CommandCategory::HISTORY,
    )
    .with_aliases(&["undo tree", "history panel", "branches"]),
];

/// The number of host commands the kernel names.
///
/// Derived from [`HOST`], so it cannot disagree with the table.
pub const HOST_COMMAND_COUNT: usize = HOST.len();

/// Returns every host command's metadata, in declaration order.
///
/// Prefer [`host_command_metas`] when a borrow will do; this clones.
#[must_use]
pub fn host_commands() -> Vec<CommandMeta> {
    HOST.to_vec()
}

/// Borrows every host command's metadata, in declaration order.
#[must_use]
pub const fn host_command_metas() -> &'static [CommandMeta] {
    HOST
}

/// Registers every host command into `registry`.
///
/// # Errors
///
/// [`RegistryError::DuplicateId`] if `registry` already holds one of the host
/// ids, which means a face claimed an id the kernel had already named.
pub fn register_host_commands(registry: &mut CommandRegistry) -> Result<(), RegistryError> {
    registry.register_all(host_commands())
}
