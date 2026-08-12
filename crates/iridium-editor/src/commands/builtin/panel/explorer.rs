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
//! [`CommandMeta::scoped`] states it once and two things follow: a `[keys]` line
//! binding one is scoped to that mode, and a command palette does not list it.
//! Both are the same fact — the command belongs to a context — and neither is
//! something a user should have to write down a second time.
//!
//! # ⚠️ Nothing here is bound by the default keymap, and that is not an omission
//!
//! The kernel's default keymap binds editor commands. A key that only means
//! something inside the file explorer is bound by **the file explorer**, whose
//! default keymap lives with the panel in `iridium-panel` — the crate that knows
//! what its own screens are. `every_registered_command_is_bound_except_the_typing_fall_through`
//! therefore skips mode-scoped commands, and the panel carries the matching
//! assertion for its own vocabulary.
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

use crate::commands::{
    CommandCategory, CommandId, CommandMeta, CommandRegistry, ModeName, RegistryError,
};

/// The mode the file explorer is in while it is browsing a tree.
///
/// The screen with the query field at the top and the rows below it — the panel
/// as it opens, and the panel it returns to whenever an editing session ends.
pub const EXPLORER_MODE: ModeName = ModeName::from_static("explorer");

/// The mode the file explorer is in while its rows are being edited as text.
///
/// ⚠️ **A separate mode rather than a flag on the browse one**, because the two
/// screens disagree about what a printable character means: browsing, it
/// narrows the list; editing, it is somebody typing a filename. A single mode
/// would need every binding to re-ask which screen it was on.
pub const EXPLORER_EDIT_MODE: ModeName = ModeName::from_static("explorer.edit");

/// The mode the file explorer is in while a plan is waiting to be confirmed.
///
/// The rows are not on screen — the operations are — so the keys that edit a row
/// are deliberately absent here rather than bound to something harmless.
pub const EXPLORER_CONFIRM_MODE: ModeName = ModeName::from_static("explorer.confirm");

/// The mode the file explorer is in while it is showing why a buffer was
/// refused.
///
/// One key: go back to the rows. The edits are what needs fixing, so anything
/// that threw them away would be the opposite of the help being offered.
pub const EXPLORER_REFUSED_MODE: ModeName = ModeName::from_static("explorer.refused");

/// Clear the explorer's query, or close the panel when there is none.
///
/// Two meanings on one key, and the order is the point: closing a panel somebody
/// has just typed into would throw away the narrowing and the panel together,
/// and the second press costs nothing.
pub const EXPLORER_DISMISS: CommandId = CommandId::from_static("explorer.dismiss");

/// Open the selected file, or toggle the selected folder.
pub const EXPLORER_ACTIVATE: CommandId = CommandId::from_static("explorer.activate");

/// Begin editing the explorer's rows as text — the oil buffer.
pub const EXPLORER_BEGIN_EDIT: CommandId = CommandId::from_static("explorer.beginEdit");

/// Move the explorer's selection one row up.
pub const EXPLORER_MOVE_UP: CommandId = CommandId::from_static("explorer.moveUp");

/// Move the explorer's selection one row down.
pub const EXPLORER_MOVE_DOWN: CommandId = CommandId::from_static("explorer.moveDown");

/// Move the explorer's selection to the first row that can be selected.
pub const EXPLORER_MOVE_TO_FIRST: CommandId = CommandId::from_static("explorer.moveToFirst");

/// Move the explorer's selection to the last row that can be selected.
pub const EXPLORER_MOVE_TO_LAST: CommandId = CommandId::from_static("explorer.moveToLast");

/// Open the selected folder, or step into an already-open one.
pub const EXPLORER_EXPAND: CommandId = CommandId::from_static("explorer.expand");

/// Close the selected folder, or step out to its parent.
pub const EXPLORER_COLLAPSE: CommandId = CommandId::from_static("explorer.collapse");

/// Make the selected folder the explorer's root — "go in here".
pub const EXPLORER_ROOT_AT_SELECTION: CommandId =
    CommandId::from_static("explorer.rootAtSelection");

/// Make the folder above the explorer's root the root — "go up".
pub const EXPLORER_ROOT_ABOVE: CommandId = CommandId::from_static("explorer.rootAbove");

/// Show dot-prefixed entries in the explorer, or stop showing them.
pub const EXPLORER_TOGGLE_HIDDEN: CommandId = CommandId::from_static("explorer.toggleHidden");

/// Delete the character before the caret in the explorer's query.
pub const EXPLORER_QUERY_BACKSPACE: CommandId = CommandId::from_static("explorer.queryBackspace");

/// Stop editing the explorer's rows, asking about unapplied work first.
pub const EXPLORER_EDIT_LEAVE: CommandId = CommandId::from_static("explorer.edit.leave");

/// Strike the row under the cursor through, or unstrike it.
pub const EXPLORER_EDIT_STRIKE_ROW: CommandId = CommandId::from_static("explorer.edit.strikeRow");

/// Type a new, empty row below the cursor.
pub const EXPLORER_EDIT_NEW_ROW: CommandId = CommandId::from_static("explorer.edit.newRow");

/// Work out what the edited rows say, and show it for confirmation.
///
/// ⛔ **This does not touch the disk.** It reaches the confirmation and stops;
/// [`EXPLORER_CONFIRM_APPLY`] is what applies.
pub const EXPLORER_EDIT_ASK_APPLY: CommandId = CommandId::from_static("explorer.edit.askApply");

/// Move the editing cursor one row up.
pub const EXPLORER_EDIT_CURSOR_UP: CommandId = CommandId::from_static("explorer.edit.cursorUp");

/// Move the editing cursor one row down.
pub const EXPLORER_EDIT_CURSOR_DOWN: CommandId = CommandId::from_static("explorer.edit.cursorDown");

/// Move the caret one character left within the row being edited.
pub const EXPLORER_EDIT_CARET_LEFT: CommandId = CommandId::from_static("explorer.edit.caretLeft");

/// Move the caret one character right within the row being edited.
pub const EXPLORER_EDIT_CARET_RIGHT: CommandId = CommandId::from_static("explorer.edit.caretRight");

/// Move the caret to the start of the row being edited.
pub const EXPLORER_EDIT_CARET_HOME: CommandId = CommandId::from_static("explorer.edit.caretHome");

/// Move the caret to the end of the row being edited.
pub const EXPLORER_EDIT_CARET_END: CommandId = CommandId::from_static("explorer.edit.caretEnd");

/// Delete the character before the caret, or take back an emptied typed row.
pub const EXPLORER_EDIT_BACKSPACE: CommandId = CommandId::from_static("explorer.edit.backspace");

/// Delete the character after the caret in the row being edited.
pub const EXPLORER_EDIT_DELETE: CommandId = CommandId::from_static("explorer.edit.delete");

/// Carry out the confirmed plan, touching the disk.
pub const EXPLORER_CONFIRM_APPLY: CommandId = CommandId::from_static("explorer.confirm.apply");

/// Go back to the rows without applying the plan.
pub const EXPLORER_CONFIRM_CANCEL: CommandId = CommandId::from_static("explorer.confirm.cancel");

/// Dismiss the refusals and go back to the rows.
pub const EXPLORER_REFUSED_DISMISS: CommandId = CommandId::from_static("explorer.refused.dismiss");

/// Every panel command, in declaration order.
///
/// Grouped by mode, and within a mode in the order the panel's own
/// documentation tables list them, so that a reader comparing the two is
/// comparing like with like.
static PANEL: &[CommandMeta] = &[
    // Browsing.
    CommandMeta::scoped(
        EXPLORER_DISMISS,
        "Clear Query or Close Explorer",
        "Clears the explorer's query, or closes the panel when there is none.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_ACTIVATE,
        "Open Selected Entry",
        "Opens the selected file, or opens and closes the selected folder.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_BEGIN_EDIT,
        "Edit Rows As Text",
        "Edits the explorer's rows as text, renaming and creating by typing.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_MOVE_UP,
        "Select Row Above",
        "Moves the explorer's selection one row up.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_MOVE_DOWN,
        "Select Row Below",
        "Moves the explorer's selection one row down.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_MOVE_TO_FIRST,
        "Select First Row",
        "Moves the explorer's selection to the first row that can be selected.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_MOVE_TO_LAST,
        "Select Last Row",
        "Moves the explorer's selection to the last row that can be selected.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EXPAND,
        "Open Folder",
        "Opens the selected folder, or steps into one that is already open.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_COLLAPSE,
        "Close Folder",
        "Closes the selected folder, or steps out to the folder holding it.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_ROOT_AT_SELECTION,
        "Set Root To Selection",
        "Makes the selected folder the explorer's root.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_ROOT_ABOVE,
        "Set Root To Parent",
        "Makes the folder above the explorer's root the root.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_TOGGLE_HIDDEN,
        "Toggle Hidden Entries",
        "Shows dot-prefixed entries in the explorer, or stops showing them.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_QUERY_BACKSPACE,
        "Delete In Query",
        "Deletes the character before the caret in the explorer's query.",
        CommandCategory::GENERAL,
        EXPLORER_MODE,
    ),
    // Editing the rows.
    CommandMeta::scoped(
        EXPLORER_EDIT_LEAVE,
        "Stop Editing Rows",
        "Stops editing the explorer's rows, asking about unapplied work first.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_STRIKE_ROW,
        "Mark Row Deleted",
        "Strikes the row under the cursor through, or unstrikes it.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_NEW_ROW,
        "New Row",
        "Types a new, empty row below the cursor.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_ASK_APPLY,
        "Apply Edited Rows",
        "Works out what the edited rows say, and shows it for confirmation.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_CURSOR_UP,
        "Move Cursor To Row Above",
        "Moves the editing cursor one row up.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_CURSOR_DOWN,
        "Move Cursor To Row Below",
        "Moves the editing cursor one row down.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_CARET_LEFT,
        "Move Caret Left In Row",
        "Moves the caret one character left within the row being edited.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_CARET_RIGHT,
        "Move Caret Right In Row",
        "Moves the caret one character right within the row being edited.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_CARET_HOME,
        "Move Caret To Row Start",
        "Moves the caret to the start of the row being edited.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_CARET_END,
        "Move Caret To Row End",
        "Moves the caret to the end of the row being edited.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_BACKSPACE,
        "Delete In Row",
        "Deletes the character before the caret, or takes back an emptied row.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    CommandMeta::scoped(
        EXPLORER_EDIT_DELETE,
        "Delete Forward In Row",
        "Deletes the character after the caret in the row being edited.",
        CommandCategory::GENERAL,
        EXPLORER_EDIT_MODE,
    ),
    // Confirming a plan.
    CommandMeta::scoped(
        EXPLORER_CONFIRM_APPLY,
        "Confirm Rename Plan",
        "Carries out the confirmed plan, renaming, creating and deleting on disk.",
        CommandCategory::GENERAL,
        EXPLORER_CONFIRM_MODE,
    )
    .mutating(),
    CommandMeta::scoped(
        EXPLORER_CONFIRM_CANCEL,
        "Cancel Rename Plan",
        "Goes back to the rows without applying the plan.",
        CommandCategory::GENERAL,
        EXPLORER_CONFIRM_MODE,
    ),
    // Reading a refusal.
    CommandMeta::scoped(
        EXPLORER_REFUSED_DISMISS,
        "Dismiss Refusals",
        "Dismisses the refusals and goes back to the rows.",
        CommandCategory::GENERAL,
        EXPLORER_REFUSED_MODE,
    ),
];

/// The number of panel commands the kernel names.
///
/// Derived from [`PANEL`], so it cannot disagree with the table.
pub const PANEL_COMMAND_COUNT: usize = PANEL.len();

/// Returns every panel command's metadata, in declaration order.
///
/// Prefer [`panel_command_metas`] when a borrow will do; this clones.
#[must_use]
pub fn panel_commands() -> Vec<CommandMeta> {
    PANEL.to_vec()
}

/// Borrows every panel command's metadata, in declaration order.
#[must_use]
pub const fn panel_command_metas() -> &'static [CommandMeta] {
    PANEL
}

/// Registers every panel command into `registry`.
///
/// # Errors
///
/// [`RegistryError::DuplicateId`] if `registry` already holds one of these ids,
/// which means a face claimed an id the kernel had already named.
pub fn register_panel_commands(registry: &mut CommandRegistry) -> Result<(), RegistryError> {
    registry.register_all(panel_commands())
}

#[cfg(test)]
mod tests {
    use super::{PANEL, PANEL_COMMAND_COUNT};
    use std::collections::BTreeSet;

    #[test]
    fn every_panel_command_names_a_mode() {
        // The defining property of this table, and the one that keeps these
        // commands out of a palette. A row that forgot would be offered as a
        // global entry that cannot do anything.
        for meta in PANEL {
            assert!(
                meta.mode().is_some(),
                "`{}` is in the panel table and names no mode",
                meta.id()
            );
            assert!(
                !meta.is_palette_entry(),
                "`{}` would be offered by a command palette",
                meta.id()
            );
        }
    }

    #[test]
    fn the_ids_are_distinct() {
        let ids: BTreeSet<&str> = PANEL.iter().map(|meta| meta.id().as_str()).collect();
        assert_eq!(
            ids.len(),
            PANEL_COMMAND_COUNT,
            "two rows of the panel table share an id"
        );
    }

    /// ⚠️ The id is what a user writes in `config.toml`, so it is a published
    /// surface: an id and the mode it is scoped to must agree, or a `[keys]`
    /// line binds a command into a screen it does not belong to.
    #[test]
    fn an_id_sits_under_the_mode_it_is_scoped_to() {
        for meta in PANEL {
            let mode = meta.mode().expect("every panel command names a mode");
            assert!(
                meta.id().as_str().starts_with(mode.as_str()),
                "`{}` is scoped to mode `{mode}`, which its id does not name",
                meta.id()
            );
        }
    }
}
