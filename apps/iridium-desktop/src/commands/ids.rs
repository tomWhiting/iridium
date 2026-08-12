//! The command ids this face contributes, and what the palette calls them.
//!
//! Split from [`keymap`](super::keymap) on the line the module doc draws: a
//! *verb* and the *key that reaches it* are different decisions, changed at
//! different times and by different people — a `[keys]` line in a user's
//! configuration file changes the second and can never change the first.

use iridium_editor::{CommandCategory, CommandId, CommandMeta};

/// Write the document to its file.
pub const FILE_SAVE: CommandId = CommandId::from_static("file.save");
/// Write the document to its file even though the file changed on disk.
pub const FILE_SAVE_FORCE: CommandId = CommandId::from_static("file.saveForce");
/// Pick a file with the system chooser and open it in a tab.
pub const FILE_OPEN: CommandId = CommandId::from_static("file.open");
/// Pick a folder with the system chooser and make it this session's project.
pub const PROJECT_OPEN: CommandId = CommandId::from_static("project.open");
/// Make the folder the explorer is showing this session's project.
pub const PROJECT_SET: CommandId = CommandId::from_static("project.set");
/// Open a tab listing every command by the id a configuration file names it by.
pub const COMMANDS_LIST: CommandId = CommandId::from_static("commands.list");
/// Open the user's `config.toml` as an ordinary, savable tab.
pub const CONFIG_EDIT: CommandId = CommandId::from_static("config.edit");

/// Every command this face contributes, in declaration order.
pub static COMMANDS: &[CommandMeta] = &[
    CommandMeta::described(
        FILE_SAVE,
        "Save",
        "Writes the document to its file, refusing if the file changed on disk.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["write", "w"]),
    CommandMeta::described(
        FILE_SAVE_FORCE,
        "Save Anyway",
        "Writes the document to its file even though the file changed on disk.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["overwrite", "force write", "w!"]),
    // ----- The three ways in, added 9 Aug 2026 -----
    //
    // ⚠️ Before these, the only way to open anything was to name it on the
    // command line or drop it on the window, and the only way to change what
    // the explorer was rooted on was `⌘↓` — which re-roots the *panel* and is
    // forgotten the moment it closes. Tom's words: "there's no open button,
    // there's no open directory, there's no set project."
    //
    // `…` on the two that open a chooser, per the platform convention that an
    // ellipsis means "this asks you something first". `project.set` asks
    // nothing, so it has none.
    CommandMeta::described(
        FILE_OPEN,
        "Open File…",
        "Picks a file with the system chooser and opens it in a new tab.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["open", "e", "edit"]),
    CommandMeta::described(
        PROJECT_OPEN,
        "Open Folder…",
        "Picks a folder with the system chooser and opens it as this session's project.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["open folder", "open directory", "open project", "cd"]),
    // Deliberately unbound, and palette-only. It is the *answer* to a question
    // — "I have walked somewhere in the explorer, how do I stay here" — asked
    // once in a session at most, and every chord spent is one a user cannot
    // have. `project.open` is the one with a key, because it is the one that
    // needs no panel open first.
    CommandMeta::described(
        PROJECT_SET,
        "Set Project to the Explorer's Folder",
        "Pins this session's project to the folder the file explorer is showing.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["set project", "set root", "pin folder", "use this folder"]),
    // Deliberately unbound. It is read once while writing a configuration
    // file and then not again for months, so it is worth a palette entry and
    // not worth a chord — and every chord spent here is one a user cannot
    // have.
    CommandMeta::described(
        COMMANDS_LIST,
        "List Every Command",
        "Opens a tab listing every command by the id to write in config.toml, with its key.",
        CommandCategory::from_static("Help"),
    )
    .with_aliases(&["keybindings", "shortcuts", "command ids", "config"]),
    // Deliberately unbound, and on the palette-only list with `commands.list`
    // for the same reason: it is the door into a file edited once and then not
    // for months. The point of it is that "where do I change my keys" has an
    // answer you can *search for* — Tom asked that question on 9 Aug 2026 and
    // the honest answer at the time was a path he had to be told.
    CommandMeta::described(
        CONFIG_EDIT,
        "Edit Configuration",
        "Opens ~/.config/iridium/config.toml in a tab, writing the default file if none exists.",
        CommandCategory::from_static("Help"),
    )
    .with_aliases(&[
        "settings",
        "preferences",
        "keybindings",
        "keymap",
        "config.toml",
    ]),
];

/// The number of commands this face contributes.
///
/// Derived from [`COMMANDS`] so it cannot drift from the table.
pub const COMMAND_COUNT: usize = COMMANDS.len();

/// Every command this face contributes, cloned for registration.
#[must_use]
pub fn command_metas() -> Vec<CommandMeta> {
    COMMANDS.to_vec()
}
