//! Commands that belong to a panel rather than to the editor.
//!
//! A panel command is a real, registered, bindable command that is only
//! meaningful while the panel owning it is open and on a particular screen.
//! *Move Down* in the file explorer is the archetype: it is a verb a user should
//! be able to rebind, and it is not a thing the editor can be asked to do.
//!
//! # Why they are registered at all
//!
//! Because `[keys]` validates. `Keymap::validate` reports
//! [`KeymapError::UnknownCommand`](crate::commands::KeymapError) for a binding
//! naming a command no registry holds, and `iridium-config` turns that into a
//! diagnostic quoting the offending line. Before this table existed, a user
//! writing
//!
//! ```toml
//! [keys]
//! "ctrl+j" = "explorer.moveDown"
//! ```
//!
//! was told their command did not exist — which was true, and was the whole
//! defect. Registering the id is what makes that line *work*, while leaving a
//! genuine typo reported exactly as before.
//!
//! # Why they carry a mode
//!
//! [`CommandMeta::scoped`] states it once and three things follow: a `[keys]`
//! line binding one is scoped to that mode, a command palette does not list it,
//! and `Keymap::canonicalize` refuses to let it apply outside its own screen.
//! All three are the same fact — the command belongs to a context — and none of
//! them is something a user should have to write down a second time.
//!
//! # ⚠️ Nothing here is bound by the default keymap, and that is not an omission
//!
//! The kernel's default keymap binds editor commands. A key that only means
//! something inside the file explorer is bound by **the file explorer**, whose
//! default keymap lives with the panel — the crate, or the face, that knows what
//! its own screens are.
//! `every_registered_command_is_bound_except_the_typing_fall_through` therefore
//! skips mode-scoped commands, and each panel carries the matching assertion for
//! its own vocabulary.
//!
//! Reading it the other way round is the useful test of the split: the kernel
//! owns the **names**, so that one verb has one id in every face, and the panel
//! owns the **keys**, so that a face with different screens is not forced to
//! pretend it has the explorer's.
//!
//! # How one runs
//!
//! Exactly like a host command, and for the same reason: nothing here appears in
//! [`BUILTIN`](super::BUILTIN) and nothing here has a `KeyboardAction`. The
//! panel resolves the key against its own stack and acts; `Editor::run_command`
//! reports
//! [`CommandRunError::Unimplemented`](crate::input::CommandRunError::Unimplemented),
//! which is the kernel saying this one is not its to run.
//!
//! # One file per panel
//!
//! ⚠️ A single table for every panel was the shape until the second one arrived,
//! and it does not survive seven: the explorer alone is 28 verbs. Each panel
//! declares its own modes, ids and table in its own file; [`TABLES`] is the only
//! place that knows there is more than one, and adding a panel is one line there
//! plus one `mod`.

mod explorer;
mod palette;
#[cfg(test)]
mod tests;

pub use explorer::{
    EXPLORER_ACTIVATE, EXPLORER_BEGIN_EDIT, EXPLORER_COLLAPSE, EXPLORER_CONFIRM_APPLY,
    EXPLORER_CONFIRM_CANCEL, EXPLORER_CONFIRM_MODE, EXPLORER_DISMISS, EXPLORER_EDIT_ASK_APPLY,
    EXPLORER_EDIT_BACKSPACE, EXPLORER_EDIT_CARET_END, EXPLORER_EDIT_CARET_HOME,
    EXPLORER_EDIT_CARET_LEFT, EXPLORER_EDIT_CARET_RIGHT, EXPLORER_EDIT_CURSOR_DOWN,
    EXPLORER_EDIT_CURSOR_UP, EXPLORER_EDIT_DELETE, EXPLORER_EDIT_LEAVE, EXPLORER_EDIT_MODE,
    EXPLORER_EDIT_NEW_ROW, EXPLORER_EDIT_STRIKE_ROW, EXPLORER_EXPAND, EXPLORER_MODE,
    EXPLORER_MOVE_DOWN, EXPLORER_MOVE_TO_FIRST, EXPLORER_MOVE_TO_LAST, EXPLORER_MOVE_UP,
    EXPLORER_QUERY_BACKSPACE, EXPLORER_REFUSED_DISMISS, EXPLORER_REFUSED_MODE, EXPLORER_ROOT_ABOVE,
    EXPLORER_ROOT_AT_SELECTION, EXPLORER_TOGGLE_HIDDEN,
};
pub use palette::{
    PALETTE_ACCEPT, PALETTE_CARET_END, PALETTE_CARET_HOME, PALETTE_CARET_LEFT, PALETTE_CARET_RIGHT,
    PALETTE_DISMISS, PALETTE_MODE, PALETTE_QUERY_BACKSPACE, PALETTE_QUERY_DELETE,
    PALETTE_SELECT_NEXT, PALETTE_SELECT_PAGE_DOWN, PALETTE_SELECT_PAGE_UP, PALETTE_SELECT_PREVIOUS,
};

use crate::commands::{CommandMeta, CommandRegistry, RegistryError};

/// Every panel's table, one entry per panel.
///
/// The single place that knows more than one panel exists. Order is the
/// registration order and therefore part of
/// [`CommandRegistry::commands`](crate::commands::CommandRegistry::commands)'
/// enumeration order, so it is kept in the order the panels were converted
/// rather than sorted — a reader tracing a command back to its file is helped
/// more by "which panel" than by alphabetical.
pub const TABLES: &[&[CommandMeta]] = &[explorer::EXPLORER, palette::PALETTE];

/// The number of panel commands the kernel names.
///
/// Derived from the tables, so it cannot disagree with them. Summed in a `const`
/// loop rather than written down, for the same reason.
pub const PANEL_COMMAND_COUNT: usize = {
    let mut total = 0;
    let mut index = 0;
    while index < TABLES.len() {
        total += TABLES[index].len();
        index += 1;
    }
    total
};

/// Returns every panel command's metadata, in declaration order.
///
/// Prefer [`panel_command_metas`] when a borrow will do; this clones.
#[must_use]
pub fn panel_commands() -> Vec<CommandMeta> {
    let mut all = Vec::with_capacity(PANEL_COMMAND_COUNT);
    for table in TABLES {
        all.extend_from_slice(table);
    }
    all
}

/// Borrows every panel command's metadata, in declaration order.
///
/// An iterator rather than one flat slice: the tables are separate `static`s, so
/// there is no single slice to lend without building one at runtime, and every
/// caller iterates anyway.
pub fn panel_command_metas() -> impl Iterator<Item = &'static CommandMeta> {
    TABLES.iter().copied().flatten()
}

/// Registers every panel command into `registry`.
///
/// # Errors
///
/// [`RegistryError::DuplicateId`] if `registry` already holds one of these ids,
/// which means a face claimed an id the kernel had already named — or that two
/// panels in [`TABLES`] chose the same one, which the module tests rule out.
pub fn register_panel_commands(registry: &mut CommandRegistry) -> Result<(), RegistryError> {
    registry.register_all(panel_commands())
}
