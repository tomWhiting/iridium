//! Terminal workspace controls, shared by keyboard and command palette.
use super::{App, Flow, Message};
use iridium_editor::commands::builtin::{
    EXPLORER_TOGGLE_PANEL, EXPLORER_TOGGLE_SIDEBAR, PALETTE_OPEN, WORKSPACE_NEXT_TAB,
    WORKSPACE_PREVIOUS_TAB,
};
use iridium_editor::{
    CommandCategory, CommandId, CommandMeta, KeyBinding, KeyEvent, KeyPress, Keymap,
};

pub(super) const SIDEBAR: CommandId = CommandId::from_static("terminal.toggleSidebar");
pub(super) const WIDER: CommandId = CommandId::from_static("terminal.widenSidebar");
pub(super) const NARROWER: CommandId = CommandId::from_static("terminal.narrowSidebar");
pub(super) const FOCUS: CommandId = CommandId::from_static("terminal.focusSidebar");
pub(super) const CONFIG: CommandId = CommandId::from_static("terminal.openConfig");
pub(super) const KEYS: CommandId = CommandId::from_static("terminal.openKeybindings");
pub(super) const RELOAD: CommandId = CommandId::from_static("terminal.reloadConfig");
pub(super) const COMMANDS: &[CommandMeta] = &[
    CommandMeta::described(
        SIDEBAR,
        "Toggle File Sidebar",
        "Shows or hides the persistent Oil file browser.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        FOCUS,
        "Focus File Sidebar or Editor",
        "Moves focus between the file browser and document.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        WIDER,
        "Widen Sidebar",
        "Adds four columns to the sidebar, keeping room for the document.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        NARROWER,
        "Narrow Sidebar",
        "Removes four columns from the sidebar.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        CONFIG,
        "Open Configuration",
        "Opens config.toml in a tab; save and reload to apply changes.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        KEYS,
        "Open Default Keybindings",
        "Opens the generated terminal binding reference beside config.toml.",
        CommandCategory::GENERAL,
    ),
    CommandMeta::described(
        RELOAD,
        "Reload Configuration",
        "Reloads saved settings and bindings across all tabs and panels.",
        CommandCategory::GENERAL,
    ),
];

pub(super) fn keymap() -> Result<Keymap, iridium_editor::KeymapError> {
    let mut map = Keymap::new("terminal-workbench");
    for (key, command) in [
        ("ctrl+b", SIDEBAR),
        ("ctrl+e", FOCUS),
        ("ctrl+alt+Right", WIDER),
        ("ctrl+alt+Left", NARROWER),
        ("alt+e", FOCUS),
        ("F4", CONFIG),
        ("ctrl+,", CONFIG),
        ("meta+,", CONFIG),
        ("alt+/", KEYS),
        ("ctrl+alt+r", RELOAD),
        ("ctrl+PageDown", WORKSPACE_NEXT_TAB),
        ("ctrl+PageUp", WORKSPACE_PREVIOUS_TAB),
        ("meta+shift+]", WORKSPACE_NEXT_TAB),
        ("meta+shift+[", WORKSPACE_PREVIOUS_TAB),
        ("ctrl+Tab", WORKSPACE_NEXT_TAB),
        ("ctrl+shift+Tab", WORKSPACE_PREVIOUS_TAB),
    ] {
        map.push(KeyBinding::parse(key, command)?);
    }
    Ok(map)
}

pub(super) fn parse(text: &str) -> Option<CommandId> {
    Some(match text {
        "sidebar" => SIDEBAR,
        "focus" => FOCUS,
        "sidebar-wider" => WIDER,
        "sidebar-narrower" => NARROWER,
        "config" => CONFIG,
        "keys" => KEYS,
        "reload-config" => RELOAD,
        "bn" | "bnext" | "tabnext" => WORKSPACE_NEXT_TAB,
        "bp" | "bprevious" | "tabprevious" => WORKSPACE_PREVIOUS_TAB,
        _ => return None,
    })
}

impl App {
    pub(super) fn workbench_command(&mut self, command: &CommandId) -> Option<Flow> {
        Some(
            if command == &SIDEBAR || command == &EXPLORER_TOGGLE_SIDEBAR {
                self.toggle_sidebar()
            } else if command == &WIDER || command == &NARROWER {
                self.resize_sidebar_command(command == &WIDER)
            } else if command == &FOCUS {
                if self.explorer.is_none() {
                    self.toggle_sidebar();
                } else {
                    if let Some(panel) = self.explorer.as_mut() {
                        panel.set_sidebar(true);
                    }
                    self.focus = if self.focus == crate::app::sidebar::Focus::Explorer {
                        crate::app::sidebar::Focus::Document
                    } else {
                        crate::app::sidebar::Focus::Explorer
                    };
                    self.sync_viewport();
                }
                Flow::Running
            } else if command == &CONFIG || command == &KEYS {
                self.open_configuration(command == &KEYS)
            } else if command == &RELOAD {
                match self.reload_configuration() {
                    Ok(()) => {
                        self.message =
                            Some(Message::notice("Configuration reloaded across all tabs"));
                    },
                    Err(error) => self.message = Some(Message::error(error)),
                }
                Flow::Running
            } else {
                return None;
            },
        )
    }

    /// Resolve global workspace chords through the real stack, including user overrides.
    pub(super) fn explorer_host_key(&mut self, event: &KeyEvent) -> Option<Flow> {
        let editor = self.editor()?;
        let command = editor
            .keymap()
            .exact_match(&[KeyPress::new(event.key, event.modifiers)], editor.mode())?
            .command()?
            .clone();
        if command == EXPLORER_TOGGLE_PANEL
            || command == PALETTE_OPEN
            || self.workspace.handles_command(&command)
            || COMMANDS.iter().any(|meta| meta.id() == &command)
        {
            Some(self.run_host_command(&command))
        } else {
            None
        }
    }
}
