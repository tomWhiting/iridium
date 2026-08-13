//! The built-in command set and its registration.
//!
//! [`BUILTIN`] declares every command the kernel implements, in one place, as
//! data. It is a `static` slice rather than a function building a `Vec`, so
//! [`BUILTIN_COMMAND_COUNT`] is *derived* from the table instead of maintained
//! next to it — one fewer place to forget when a command is added.
//!
//! The declaration order is also the [`CommandRegistry::commands`] enumeration
//! order, so it is grouped by category to keep that order meaningful. The table
//! itself lives in `table`, which is data and nothing else; this module is the
//! registration built on top of it.
//!
//! # Adding a command
//!
//! A command compiled into the kernel touches four places, each of which fails
//! loudly if skipped: an id in [`ids`], an entry in [`BUILTIN`], a
//! `KeyboardAction` variant, and the row in the action table that pairs the two
//! (the exhaustive `match` makes a missing implementation a compile error, and the
//! dispatch tests make a missing pairing a test failure). A command contributed
//! from **outside** the kernel needs none of them; see the module docs of
//! [`crate::commands`].
//!
//! Entries carry [`CommandMeta::with_aliases`] wherever the title, id, category
//! and description between them miss a word a user would plausibly type — `dupe`,
//! `eol`, `yank`, `uncomment`. They are search terms only: nothing resolves a
//! command *by* an alias, so adding one can never change which command a key or a
//! host invocation runs.

mod host;
mod ids;
mod panel;
mod table;
mod workspace;

pub use host::{
    CONFIG_RELOAD, EXPLORER_TOGGLE_PANEL, EXPLORER_TOGGLE_SIDEBAR, HISTORY_TOGGLE_PANEL, HOST,
    HOST_COMMAND_COUNT, PALETTE_OPEN, VIEW_TOGGLE_THEME, host_command_metas, host_commands,
    register_host_commands,
};
pub use ids::*;
pub use panel::{
    EXPLORER_ACTIVATE, EXPLORER_BEGIN_EDIT, EXPLORER_COLLAPSE, EXPLORER_CONFIRM_APPLY,
    EXPLORER_CONFIRM_CANCEL, EXPLORER_CONFIRM_MODE, EXPLORER_DISMISS, EXPLORER_EDIT_ASK_APPLY,
    EXPLORER_EDIT_BACKSPACE, EXPLORER_EDIT_CARET_END, EXPLORER_EDIT_CARET_HOME,
    EXPLORER_EDIT_CARET_LEFT, EXPLORER_EDIT_CARET_RIGHT, EXPLORER_EDIT_CURSOR_DOWN,
    EXPLORER_EDIT_CURSOR_UP, EXPLORER_EDIT_DELETE, EXPLORER_EDIT_LEAVE, EXPLORER_EDIT_MODE,
    EXPLORER_EDIT_NEW_ROW, EXPLORER_EDIT_STRIKE_ROW, EXPLORER_EXPAND, EXPLORER_MODE,
    EXPLORER_MOVE_DOWN, EXPLORER_MOVE_TO_FIRST, EXPLORER_MOVE_TO_LAST, EXPLORER_MOVE_UP,
    EXPLORER_QUERY_BACKSPACE, EXPLORER_REFUSED_DISMISS, EXPLORER_REFUSED_MODE, EXPLORER_ROOT_ABOVE,
    EXPLORER_ROOT_AT_SELECTION, EXPLORER_TOGGLE_HIDDEN, HISTORY_DISMISS, HISTORY_JUMP,
    HISTORY_MODE, HISTORY_SELECT_FIRST, HISTORY_SELECT_LAST, HISTORY_SELECT_NEXT,
    HISTORY_SELECT_PAGE_DOWN, HISTORY_SELECT_PAGE_UP, HISTORY_SELECT_PREVIOUS, PALETTE_ACCEPT,
    PALETTE_CARET_END, PALETTE_CARET_HOME, PALETTE_CARET_LEFT, PALETTE_CARET_RIGHT,
    PALETTE_DISMISS, PALETTE_MODE, PALETTE_QUERY_BACKSPACE, PALETTE_QUERY_DELETE,
    PALETTE_SELECT_NEXT, PALETTE_SELECT_PAGE_DOWN, PALETTE_SELECT_PAGE_UP, PALETTE_SELECT_PREVIOUS,
    PANEL_COMMAND_COUNT, SEARCH_CARET_END, SEARCH_CARET_HOME, SEARCH_CARET_LEFT,
    SEARCH_CARET_RIGHT, SEARCH_DISMISS, SEARCH_FIELD_BACKSPACE, SEARCH_FIELD_DELETE, SEARCH_MODE,
    SEARCH_REPLACE_ALL, SEARCH_REPLACE_CURRENT, SEARCH_TOGGLE_CASE_SENSITIVE, SEARCH_TOGGLE_FIELD,
    SEARCH_TOGGLE_REGEX, SEARCH_TOGGLE_WHOLE_WORD, panel_command_metas, panel_commands,
    register_panel_commands,
};
pub use table::{BUILTIN, BUILTIN_COMMAND_COUNT};
pub use workspace::{
    WORKSPACE, WORKSPACE_CLOSE_TAB, WORKSPACE_COMMAND_COUNT, WORKSPACE_FIRST_TAB,
    WORKSPACE_LAST_TAB, WORKSPACE_NEXT_TAB, WORKSPACE_PREVIOUS_TAB, register_workspace_commands,
    workspace_command_metas, workspace_commands,
};

use crate::commands::{CommandMeta, CommandRegistry, RegistryError};

/// Returns every built-in command's metadata, in declaration order.
///
/// Prefer [`builtin_command_metas`] when a borrow will do; this clones.
#[must_use]
pub fn builtin_commands() -> Vec<CommandMeta> {
    BUILTIN.to_vec()
}

/// Borrows every built-in command's metadata, in declaration order.
#[must_use]
pub const fn builtin_command_metas() -> &'static [CommandMeta] {
    BUILTIN
}

/// Registers every built-in command into `registry`.
///
/// # Errors
///
/// [`RegistryError::DuplicateId`] if `registry` already holds one of the built-in
/// ids, which means a host claimed a kernel id.
pub fn register_builtin_commands(registry: &mut CommandRegistry) -> Result<(), RegistryError> {
    registry.register_all(builtin_commands())
}

/// Builds a registry containing exactly the built-in commands.
///
/// # Errors
///
/// [`RegistryError`] only if the built-in table itself is inconsistent (a
/// duplicated id), which the module tests rule out.
pub fn builtin_registry() -> Result<CommandRegistry, RegistryError> {
    let mut registry = CommandRegistry::with_capacity(BUILTIN_COMMAND_COUNT);
    register_builtin_commands(&mut registry)?;
    Ok(registry)
}

/// Builds the registry an editor starts with: the built-ins **and** the host
/// commands the kernel names.
///
/// This, not [`builtin_registry`], is what the default keymap must be validated
/// against — the default binds `palette.open`, which no kernel action implements,
/// and a registry missing it would report the binding as a typo.
///
/// # Errors
///
/// [`RegistryError`] only if one of the two tables is inconsistent, or if they
/// collide with each other; the module tests rule both out.
pub fn default_registry() -> Result<CommandRegistry, RegistryError> {
    let mut registry = CommandRegistry::with_capacity(
        BUILTIN_COMMAND_COUNT + HOST_COMMAND_COUNT + WORKSPACE_COMMAND_COUNT + PANEL_COMMAND_COUNT,
    );
    register_builtin_commands(&mut registry)?;
    register_host_commands(&mut registry)?;
    register_workspace_commands(&mut registry)?;
    register_panel_commands(&mut registry)?;
    Ok(registry)
}
