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

/// Show or hide the file explorer.
///
/// Bound to `Ctrl+Alt+E` by the default keymap, joining the undo tree on the
/// `Ctrl+Alt` shape the panel toggles share.
///
/// A *toggle*, for the same reason the undo tree is one: there is nothing to
/// abandon, so the chord that opened it is how it is put away, and a face that
/// could only open it would need a second id to close it.
///
/// The kernel names it and reports it; the face owns the drawing and the
/// filesystem, which is why nothing here reads a directory — the kernel has no
/// business knowing whether the host has one.
pub const EXPLORER_TOGGLE_PANEL: CommandId = CommandId::from_static("explorer.togglePanel");

/// Move the file explorer between a floating panel and a sidebar.
///
/// Bound to `Ctrl+Alt+B` by the default keymap and to `⌘B` by the desktop
/// face's ⌘ layer — the spelling VS Code and Zed both use for the sidebar, so a
/// hand that has used either arrives knowing it.
///
/// ⭐ **A placement, not a second panel.** The same explorer is drawn either
/// way, with the same rows, keys, filter and edit mode; what changes is where
/// it sits, how much of the window it may fill, and whether it holds key focus.
/// Two ids — "open the sidebar", "open the panel" — would be two things to bind
/// and two states to reconcile when both were somehow on.
///
/// ⚠️ **The kernel names it and knows nothing else about it.** Whether a face
/// *has* two placements is the face's business: the browser face has one
/// document and no window furniture to speak of, and a face with nowhere to put
/// a column is free to report this unimplemented rather than pretending.
pub const EXPLORER_TOGGLE_PLACEMENT: CommandId = CommandId::from_static("explorer.togglePlacement");

/// Swap the editor between its light and dark themes.
///
/// Bound to `Ctrl+Alt+T` by the default keymap, and to `⌘⌥T` by the desktop
/// face's ⌘ layer.
///
/// ⭐ **One toggle, not a `view.lightTheme`/`view.darkTheme` pair** (D-3). A
/// pair reads as more precise and is worse in the hand: the thing anyone
/// actually wants is "the other one", and a pair makes that two ids to bind,
/// two palette rows saying almost the same thing, and a question — which of
/// the two is currently a no-op? — that the face has to answer before it can
/// grey a row out.
///
/// ⚠️ **A host command even though the kernel owns `Theme`.** The kernel can
/// hold a theme and repaint from it, but *which* theme the toggle lands on is
/// not the kernel's to decide: the desktop face follows the system appearance
/// and can be pinned away from it (D-2), the terminal face rebuilds its
/// palette from whatever the editor holds, and a face may carry themes the
/// kernel never named. Naming the action here and letting each face do the
/// swap keeps one id and one key across all three without the kernel
/// pretending to know what "light" means for a terminal.
pub const VIEW_TOGGLE_THEME: CommandId = CommandId::from_static("view.toggleTheme");

/// Re-read the user's configuration file and apply it to this session.
///
/// Bound to `Ctrl+Alt+R` by the default keymap, joining the panel toggles and
/// the theme swap on the `Ctrl+Alt` shape, and to `⌘⌥R` by the desktop face's
/// ⌘ layer.
///
/// ⚠️ **A host command because the kernel has no configuration file.** It has
/// an [`EditorConfig`](crate::editor::EditorConfig) and a keymap stack, and it
/// can be handed new ones — [`Workspace::set_config`](crate::workspace::Workspace::set_config)
/// and [`Workspace::replace_keymap`](crate::workspace::Workspace::replace_keymap)
/// are exactly that — but *where the file lives, what it is called and what
/// dialect it is written in* are the face's, and the browser face has no file
/// at all. Naming the action here is what keeps one id and one key across the
/// faces that do.
///
/// **Replace, never push.** A face implementing this by pushing the re-read
/// bindings would leave the previous version of that layer underneath, so
/// every binding the user *deleted* from the file would go on firing — see
/// [`KeymapStack::replace_named`](crate::commands::KeymapStack::replace_named).
pub const CONFIG_RELOAD: CommandId = CommandId::from_static("config.reload");

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
    CommandMeta::described(
        EXPLORER_TOGGLE_PANEL,
        "Toggle File Explorer",
        "Shows or hides the file explorer, where any file in the project can be reached.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["files", "file tree", "explorer", "open file"]),
    CommandMeta::described(
        EXPLORER_TOGGLE_PLACEMENT,
        "Toggle Explorer Sidebar",
        "Moves the file explorer between a floating panel and a column beside the document.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["sidebar", "side bar", "dock explorer", "file tree sidebar"]),
    CommandMeta::described(
        VIEW_TOGGLE_THEME,
        "Toggle Light/Dark Theme",
        "Swaps between the light and dark themes, repainting every surface.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["theme", "dark mode", "light mode", "appearance"]),
    CommandMeta::described(
        CONFIG_RELOAD,
        "Reload Configuration",
        "Re-reads your configuration file and applies it, without restarting.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["config", "settings", "keybindings", "reload", "keymap"]),
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
