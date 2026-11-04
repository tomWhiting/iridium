//! The file explorer's vocabulary — its four screens and the verbs on each.
//!
//! The first panel converted onto this mechanism (#91), and therefore the one
//! the others are written against. [`super`] carries the argument for why panel
//! commands are registered at all and why they carry a mode; this file is the
//! explorer's answer to it.
//!
//! # Four modes, because there are four screens
//!
//! Browsing, editing the rows as text, confirming a plan, and reading a
//! refusal. They are separate modes rather than flags on one because the
//! screens disagree about what a printable character *means* — browsing it
//! narrows the list, editing it is somebody typing a filename — and a single
//! mode would make every binding re-ask which screen it was on.

use crate::commands::{CommandCategory, CommandId, CommandMeta, ModeName};

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

/// Work out what the edited rows say, and show it for confirmation.
///
/// ⛔ **This does not touch the disk.** It reaches the confirmation and stops;
/// [`EXPLORER_CONFIRM_APPLY`] is what applies.
pub const EXPLORER_EDIT_ASK_APPLY: CommandId = CommandId::from_static("explorer.edit.askApply");

/// Carry out the confirmed plan, touching the disk.
pub const EXPLORER_CONFIRM_APPLY: CommandId = CommandId::from_static("explorer.confirm.apply");

/// Go back to the rows without applying the plan.
pub const EXPLORER_CONFIRM_CANCEL: CommandId = CommandId::from_static("explorer.confirm.cancel");

/// Dismiss the refusals and go back to the rows.
pub const EXPLORER_REFUSED_DISMISS: CommandId = CommandId::from_static("explorer.refused.dismiss");

/// Every file explorer command, in declaration order.
///
/// Grouped by mode, and within a mode in the order the panel's own
/// documentation tables list them, so that a reader comparing the two is
/// comparing like with like.
pub(super) static EXPLORER: &[CommandMeta] = &[
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
        EXPLORER_EDIT_ASK_APPLY,
        "Apply Edited Rows",
        "Works out what the edited rows say, and shows it for confirmation.",
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
