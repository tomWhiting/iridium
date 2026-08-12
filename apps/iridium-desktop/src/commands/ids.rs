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
/// Ask where to write the document, starting from the name it already has.
pub const FILE_SAVE_AS: CommandId = CommandId::from_static("file.saveAs");
/// Open a fresh untitled buffer in a new tab.
pub const FILE_NEW: CommandId = CommandId::from_static("file.new");
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
    // ----- The two Tom asked for on 12 Aug 2026 -----
    //
    // ⚠️ `save_as` was **already built, complete and tested** — the prompt, the
    // atomic write, the language that follows the new name, all of it — and
    // nothing led to it for a document that had a name. `⌘S` reaches it only
    // for an *unnamed* buffer, because for a named one it must write rather
    // than ask. So the whole verb was reachable from exactly one state and
    // invisible from every other, which is why the honest description of this
    // row is a door, not a feature.
    //
    // `file.new` is the other half of the same complaint and was genuinely
    // absent: an untitled buffer appeared only when the last tab closed.
    CommandMeta::described(
        FILE_SAVE_AS,
        "Save As…",
        "Asks where to write the document, starting from the name it already has.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["save copy", "rename", "write as", "saveas", "w"]),
    CommandMeta::described(
        FILE_NEW,
        "New File",
        "Opens a fresh untitled buffer in a new tab beside what is already open.",
        CommandCategory::from_static("File"),
    )
    .with_aliases(&["new", "new buffer", "new tab", "blank", "enew"]),
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
