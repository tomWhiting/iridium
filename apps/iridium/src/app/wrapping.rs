//! Per-document wrapping controls, exposed through the command palette.
use super::{App, Flow, Message};
use iridium_editor::{CommandCategory, CommandId, CommandMeta};
const ON: CommandId = CommandId::from_static("view.wrap");
const OFF: CommandId = CommandId::from_static("view.nowrap");
const TOGGLE: CommandId = CommandId::from_static("view.toggleWrap");
pub(super) const COMMANDS: &[CommandMeta] = &[
    CommandMeta::described(
        ON,
        "Enable Line Wrapping",
        "Wrap long lines at the viewport edge without changing their contents.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["wrap"]),
    CommandMeta::described(
        OFF,
        "Disable Line Wrapping",
        "Show one row per line; horizontal scrolling follows the caret.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["nowrap"]),
    CommandMeta::described(
        TOGGLE,
        "Toggle Line Wrapping",
        "Toggle soft wrapping for this document.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["wrap!"]),
];
pub(super) fn parse(text: &str) -> Option<CommandId> {
    match text {
        "wrap" => Some(ON),
        "nowrap" => Some(OFF),
        "wrap!" => Some(TOGGLE),
        _ => None,
    }
}
impl App {
    pub(super) fn wrap_command(&mut self, command: &CommandId) -> Option<Flow> {
        if command != &ON && command != &OFF && command != &TOGGLE {
            return None;
        }
        if let Some(editor) = self.workspace.active_editor_mut() {
            let mut config = editor.state().config.clone();
            config.word_wrap = if command == &TOGGLE {
                !config.word_wrap
            } else {
                command == &ON
            };
            let enabled = config.word_wrap;
            editor.set_config(config);
            let mut viewport = editor.state().viewport.clone();
            viewport.scroll_offset_y = 0.0;
            viewport.scroll_offset_x = 0.0;
            editor.set_viewport(viewport);
            self.message = Some(Message::notice(if enabled {
                "Line wrapping on"
            } else {
                "Line wrapping off · Shift+wheel pans horizontally"
            }));
        }
        self.sync_viewport();
        self.ensure_caret_visible();
        Some(Flow::Running)
    }
}
