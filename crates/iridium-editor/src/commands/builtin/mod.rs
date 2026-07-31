//! The built-in command set and its registration.
//!
//! [`BUILTIN`] declares every command the kernel implements, in one place, as
//! data. It is a `static` slice rather than a function building a `Vec`, so
//! [`BUILTIN_COMMAND_COUNT`] is *derived* from the table instead of maintained
//! next to it — one fewer place to forget when a command is added.
//!
//! The declaration order is also the [`CommandRegistry::commands`] enumeration
//! order, so it is grouped by category to keep that order meaningful.
//!
//! # Adding a command
//!
//! A command compiled into the kernel touches four places, each of which fails
//! loudly if skipped: an id in [`ids`], an entry here, a
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

pub use host::{
    HOST, HOST_COMMAND_COUNT, PALETTE_OPEN, host_command_metas, host_commands,
    register_host_commands,
};
pub use ids::*;

use crate::commands::{CommandCategory, CommandMeta, CommandRegistry, RegistryError};

const NAV: CommandCategory = CommandCategory::NAVIGATION;
const SEL: CommandCategory = CommandCategory::SELECTION;
const EDIT: CommandCategory = CommandCategory::EDITING;
const LINES: CommandCategory = CommandCategory::LINES;
const COMMENTS: CommandCategory = CommandCategory::COMMENTS;
const CLIP: CommandCategory = CommandCategory::CLIPBOARD;
const HISTORY: CommandCategory = CommandCategory::HISTORY;
const MULTI: CommandCategory = CommandCategory::MULTI_CURSOR;
const SEARCH: CommandCategory = CommandCategory::SEARCH;
const GENERAL: CommandCategory = CommandCategory::GENERAL;

/// Every built-in command's metadata, in declaration order.
///
/// `static` so it costs nothing to enumerate and so the count below cannot drift.
pub static BUILTIN: &[CommandMeta] = &[
    // ----- Navigation -----
    CommandMeta::from_static(CURSOR_CHAR_LEFT, "Cursor Left", NAV),
    CommandMeta::from_static(CURSOR_CHAR_RIGHT, "Cursor Right", NAV),
    CommandMeta::from_static(CURSOR_WORD_LEFT, "Cursor Word Left", NAV),
    CommandMeta::from_static(CURSOR_WORD_RIGHT, "Cursor Word Right", NAV),
    CommandMeta::described(
        CURSOR_LINE_UP,
        "Cursor Up",
        "Keeps each cursor's sticky preferred column.",
        NAV,
    ),
    CommandMeta::described(
        CURSOR_LINE_DOWN,
        "Cursor Down",
        "Keeps each cursor's sticky preferred column.",
        NAV,
    ),
    CommandMeta::described(
        CURSOR_LINE_START,
        "Cursor to Line Start",
        "Smart home: the first non-whitespace character, or column zero when already there.",
        NAV,
    )
    .with_aliases(&["bol", "home"]),
    CommandMeta::from_static(CURSOR_LINE_END, "Cursor to Line End", NAV).with_aliases(&["eol"]),
    CommandMeta::described(
        CURSOR_DOCUMENT_START,
        "Cursor to Document Start",
        "Merges every cursor into one.",
        NAV,
    )
    .with_aliases(&["bof", "top"]),
    CommandMeta::described(
        CURSOR_DOCUMENT_END,
        "Cursor to Document End",
        "Merges every cursor into one.",
        NAV,
    )
    .with_aliases(&["eof", "bottom"]),
    // ----- Selection -----
    CommandMeta::from_static(CURSOR_CHAR_LEFT_SELECT, "Extend Selection Left", SEL),
    CommandMeta::from_static(CURSOR_CHAR_RIGHT_SELECT, "Extend Selection Right", SEL),
    CommandMeta::from_static(CURSOR_WORD_LEFT_SELECT, "Extend Selection Word Left", SEL),
    CommandMeta::from_static(CURSOR_WORD_RIGHT_SELECT, "Extend Selection Word Right", SEL),
    CommandMeta::from_static(CURSOR_LINE_UP_SELECT, "Extend Selection Up", SEL),
    CommandMeta::from_static(CURSOR_LINE_DOWN_SELECT, "Extend Selection Down", SEL),
    CommandMeta::from_static(
        CURSOR_LINE_START_SELECT,
        "Extend Selection to Line Start",
        SEL,
    ),
    CommandMeta::from_static(CURSOR_LINE_END_SELECT, "Extend Selection to Line End", SEL),
    CommandMeta::from_static(
        CURSOR_DOCUMENT_START_SELECT,
        "Extend Selection to Document Start",
        SEL,
    ),
    CommandMeta::from_static(
        CURSOR_DOCUMENT_END_SELECT,
        "Extend Selection to Document End",
        SEL,
    ),
    CommandMeta::from_static(SELECTION_SELECT_ALL, "Select All", SEL),
    CommandMeta::described(
        SELECTION_COLLAPSE_TO_PRIMARY,
        "Collapse to Single Cursor",
        "Drops every secondary cursor and collapses the primary selection.",
        SEL,
    )
    .with_aliases(&["one cursor", "exit multi cursor"]),
    // ----- Editing -----
    CommandMeta::described(
        EDIT_INSERT_CHARACTER,
        "Insert Character",
        "The typing fall-through. Not bound to any key sequence: it runs when a keypress \
             matches no binding and carries text.",
        EDIT,
    )
    .mutating(),
    CommandMeta::described(
        EDIT_INSERT_NEWLINE,
        "Insert Line Break",
        "Applies auto-indent, bracket-block expansion and code-fence expansion when enabled.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["newline", "enter", "return"]),
    CommandMeta::described(
        EDIT_TAB,
        "Tab",
        "Indents every touched line when anything is selected; otherwise inserts a tab or \
             pads to the next tab stop.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["indent"]),
    CommandMeta::from_static(EDIT_OUTDENT, "Outdent", EDIT)
        .mutating()
        .with_aliases(&["unindent", "dedent"]),
    CommandMeta::described(
        EDIT_DELETE_BACKWARD,
        "Delete Backward",
        "Deletes the selection, or the character before each caret.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["backspace", "erase"]),
    CommandMeta::from_static(EDIT_DELETE_WORD_BACKWARD, "Delete Word Backward", EDIT).mutating(),
    CommandMeta::described(
        EDIT_DELETE_FORWARD,
        "Delete Forward",
        "Deletes the selection, or the character after each caret.",
        EDIT,
    )
    .mutating()
    .with_aliases(&["del", "erase"]),
    CommandMeta::from_static(EDIT_DELETE_WORD_FORWARD, "Delete Word Forward", EDIT).mutating(),
    // ----- Lines -----
    CommandMeta::from_static(LINES_MOVE_UP, "Move Line Up", LINES)
        .mutating()
        .with_aliases(&["swap line up"]),
    CommandMeta::from_static(LINES_MOVE_DOWN, "Move Line Down", LINES)
        .mutating()
        .with_aliases(&["swap line down"]),
    CommandMeta::from_static(LINES_DUPLICATE_UP, "Duplicate Line Up", LINES)
        .mutating()
        .with_aliases(&["clone", "dupe"]),
    CommandMeta::from_static(LINES_DUPLICATE_DOWN, "Duplicate Line Down", LINES)
        .mutating()
        .with_aliases(&["clone", "dupe"]),
    CommandMeta::from_static(LINES_DELETE, "Delete Line", LINES)
        .mutating()
        .with_aliases(&["kill line", "remove line"]),
    CommandMeta::from_static(LINES_JOIN, "Join Lines", LINES)
        .mutating()
        .with_aliases(&["merge lines"]),
    // ----- Comments -----
    CommandMeta::described(
        COMMENT_TOGGLE_LINE,
        "Toggle Line Comment",
        "Uses the document language's comment syntax, falling back to the configured token; \
             acknowledged without editing when neither exists.",
        COMMENTS,
    )
    .mutating()
    .with_aliases(&["uncomment", "//"]),
    CommandMeta::described(
        COMMENT_TOGGLE_BLOCK,
        "Toggle Block Comment",
        "Falls back to the line comment toggle for languages with no block pair.",
        COMMENTS,
    )
    .mutating()
    .with_aliases(&["uncomment", "/*"]),
    // ----- Clipboard -----
    CommandMeta::described(
        CLIPBOARD_COPY,
        "Copy",
        "Copies the selections, or each cursor's whole line when nothing is selected.",
        CLIP,
    )
    .with_aliases(&["yank"]),
    CommandMeta::described(
        CLIPBOARD_CUT,
        "Cut",
        "Always updates the clipboard, even when nothing can be removed.",
        CLIP,
    )
    .mutating()
    .with_aliases(&["kill"]),
    CommandMeta::described(
        CLIPBOARD_PASTE,
        "Paste",
        "Requests clipboard text from the host, then inserts it at every cursor.",
        CLIP,
    )
    .mutating()
    .with_aliases(&["put"]),
    // ----- History -----
    CommandMeta::described(
        HISTORY_UNDO,
        "Undo",
        "Steps back along the active branch of the undo tree.",
        HISTORY,
    )
    .mutating()
    .with_aliases(&["revert"]),
    CommandMeta::described(
        HISTORY_REDO,
        "Redo",
        "Steps forward along the active branch of the undo tree.",
        HISTORY,
    )
    .mutating()
    .with_aliases(&["reapply"]),
    // ----- Multi-cursor -----
    CommandMeta::described(
        MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH,
        "Add Selection to Next Find Match",
        "Selects the word at the caret first when nothing is selected.",
        MULTI,
    )
    .with_aliases(&["next occurrence"]),
    CommandMeta::from_static(
        MULTI_CURSOR_SELECT_ALL_OCCURRENCES,
        "Select All Occurrences",
        MULTI,
    ),
    CommandMeta::described(
        MULTI_CURSOR_REMOVE_LAST_CURSOR,
        "Remove Last Cursor",
        "Pops the most recently added cursor; a no-op once an edit or motion has invalidated \
             the addition order.",
        MULTI,
    ),
    CommandMeta::from_static(MULTI_CURSOR_ADD_CURSOR_ABOVE, "Add Cursor Above", MULTI),
    CommandMeta::from_static(MULTI_CURSOR_ADD_CURSOR_BELOW, "Add Cursor Below", MULTI),
    CommandMeta::described(
        MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
        "Move Last Selection to Next Find Match",
        "Drops the most recently added occurrence cursor and takes the next one.",
        MULTI,
    )
    .with_aliases(&["skip occurrence"]),
    // ----- Search -----
    CommandMeta::from_static(SEARCH_OPEN, "Find", SEARCH).with_aliases(&["search"]),
    CommandMeta::from_static(SEARCH_NEXT_MATCH, "Find Next", SEARCH).with_aliases(&["search next"]),
    CommandMeta::from_static(SEARCH_PREVIOUS_MATCH, "Find Previous", SEARCH)
        .with_aliases(&["search previous"]),
    // ----- General -----
    CommandMeta::described(
        COMMAND_NO_OP,
        "Do Nothing",
        "Swallows the keypress. Bound by a modal keymap so an unhandled key in a \
         non-typing mode is consumed instead of inserted.",
        GENERAL,
    ),
];

/// The number of commands the kernel declares.
///
/// Derived from [`BUILTIN`], so it cannot disagree with the table.
pub const BUILTIN_COMMAND_COUNT: usize = BUILTIN.len();

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
    let mut registry = CommandRegistry::with_capacity(BUILTIN_COMMAND_COUNT + HOST_COMMAND_COUNT);
    register_builtin_commands(&mut registry)?;
    register_host_commands(&mut registry)?;
    Ok(registry)
}
