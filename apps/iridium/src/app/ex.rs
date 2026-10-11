//! Exact ex-style aliases execute registered commands, independently of fuzzy ranking.
use super::prompt::Message;
use super::{App, Flow};
use iridium_editor::{CommandCategory, CommandId, CommandMeta};

const QUIT: CommandId = CommandId::from_static("terminal.quitTab");
const QUIT_FORCE: CommandId = CommandId::from_static("terminal.quitTabForce");
const QUIT_ALL: CommandId = CommandId::from_static("terminal.quitAll");
const QUIT_ALL_FORCE: CommandId = CommandId::from_static("terminal.quitAllForce");
const WRITE_QUIT: CommandId = CommandId::from_static("terminal.writeQuit");

pub(super) const COMMANDS: &[CommandMeta] = &[
    CommandMeta::described(
        QUIT,
        "Quit Tab",
        "Closes the active tab, refusing unsaved changes; exits on the last tab.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["q", "quit"]),
    CommandMeta::described(
        QUIT_FORCE,
        "Quit Tab Without Saving",
        "Discards unsaved changes in this tab; exits on the last tab.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["q!", "quit!"]),
    CommandMeta::described(
        QUIT_ALL,
        "Quit All",
        "Exits only when every open document is saved.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["qa", "qall"]),
    CommandMeta::described(
        QUIT_ALL_FORCE,
        "Quit All Without Saving",
        "Exits, discarding unsaved changes in all documents.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["qa!", "qall!"]),
    CommandMeta::described(
        WRITE_QUIT,
        "Write and Quit",
        "Writes the active document, then closes its tab only after a successful save.",
        CommandCategory::GENERAL,
    )
    .with_aliases(&["wq", "x"]),
];

pub(super) enum WriteOutcome {
    Saved,
    NeedsName,
    Refused,
}

pub(super) fn parse(query: &str) -> Option<CommandId> {
    let query = query.trim();
    let query = query.strip_prefix(':').unwrap_or(query);
    Some(match query {
        "w" | "write" => super::commands::FILE_SAVE,
        "w!" | "write!" => super::commands::FILE_SAVE_FORCE,
        "q" | "quit" => QUIT,
        "q!" | "quit!" => QUIT_FORCE,
        "qa" | "qall" => QUIT_ALL,
        "qa!" | "qall!" => QUIT_ALL_FORCE,
        "wq" | "x" => WRITE_QUIT,
        _ => return super::workbench::parse(query).or_else(|| super::wrapping::parse(query)),
    })
}

impl App {
    pub(super) fn ex_command(&mut self, command: &CommandId) -> Option<Flow> {
        Some(if command == &QUIT {
            self.quit_tab(false)
        } else if command == &QUIT_FORCE {
            self.quit_tab(true)
        } else if command == &QUIT_ALL_FORCE {
            Flow::Exit
        } else if command == &QUIT_ALL {
            if self.workspace.tabs().iter().any(|tab| {
                self.workspace
                    .node(*tab)
                    .and_then(iridium_editor::workspace::Node::document)
                    .is_some_and(|id| self.document_is_dirty(id))
            }) {
                self.message = Some(Message::error(
                    "Unsaved documents. Write them first, or use :qa! to discard all changes.",
                ));
                Flow::Running
            } else {
                Flow::Exit
            }
        } else if command == &WRITE_QUIT {
            self.pending_write_quit = None;
            match self.write_file(false) {
                WriteOutcome::Saved => self.quit_tab(false),
                WriteOutcome::NeedsName => {
                    self.pending_write_quit = self.workspace.active();
                    Flow::Running
                },
                WriteOutcome::Refused => Flow::Running,
            }
        } else {
            return None;
        })
    }

    pub(super) fn complete_write_as(&mut self, path: &std::path::Path) -> Flow {
        let quit_tab = self.pending_write_quit.take();
        if quit_tab.is_some() && quit_tab != self.workspace.active() {
            self.message = Some(Message::error(
                "The tab changed while waiting for a filename; write and quit cancelled.",
            ));
            return Flow::Running;
        }
        match self.write_as(path) {
            WriteOutcome::Saved if quit_tab.is_some() => self.quit_tab(false),
            WriteOutcome::Saved | WriteOutcome::NeedsName | WriteOutcome::Refused => Flow::Running,
        }
    }

    fn quit_tab(&mut self, force: bool) -> Flow {
        if !force && self.is_dirty() {
            self.message = Some(Message::error(
                "No write since last change. Use :w to save or :q! to discard.",
            ));
            return Flow::Running;
        }
        if self.workspace.tabs().len() <= 1 {
            return Flow::Exit;
        }
        self.workspace
            .active()
            .map_or(Flow::Exit, |tab| self.close_tab(tab))
    }
}
