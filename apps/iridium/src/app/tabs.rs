//! Tab lifecycle. Workspace defines navigation; the host guards unsaved files.
use iridium_editor::CommandId;
use iridium_editor::commands::builtin::WORKSPACE_CLOSE_TAB;
use iridium_editor::workspace::{DocumentId, NodeId};

use super::prompt::{Deed, Message, Prompt};
use super::{App, Flow};

impl App {
    pub(super) fn tab_mouse(&mut self, event: iridium_tui::input::MouseEvent) -> bool {
        use iridium_tui::input::{MouseButton, MouseEventKind};
        let Some(row) = self.mouse_layout().and_then(|layout| layout.tab_row) else {
            return false;
        };
        if usize::from(event.row) != row {
            return false;
        }
        if event.kind == MouseEventKind::Down(MouseButton::Left)
            && let Some(index) = self
                .tab_strip()
                .hit(usize::from(event.column), self.columns)
            && let Some(tab) = self.workspace.tabs().get(index).copied()
        {
            self.stop_mouse_drag();
            self.leave_tab();
            if self.workspace.activate(tab) {
                self.after_tab_change();
            }
        }
        true
    }

    pub(super) fn document_is_dirty(&self, id: DocumentId) -> bool {
        self.workspace
            .editor(id)
            .zip(self.workspace.payload(id))
            .is_some_and(|(editor, session)| session.is_dirty(editor))
    }

    pub(super) fn workspace_command(&mut self, command: &CommandId) -> Flow {
        if command == &WORKSPACE_CLOSE_TAB {
            let Some(tab) = self.workspace.active() else {
                return Flow::Running;
            };
            if self.is_dirty() {
                self.prompt = Some(Prompt::confirm(
                    "Unsaved changes. Close this tab without saving? (y/n)",
                    Deed::CloseTab(tab),
                ));
                return Flow::Running;
            }
            return self.close_tab(tab);
        }
        let before = self.workspace.active_document();
        match self.workspace.run_command(command) {
            Ok(true) => {
                if let Some((editor, session)) =
                    before.and_then(|id| self.workspace.editor_and_payload_mut(id))
                {
                    if self.search_open {
                        session.search.close(editor);
                    }
                    editor.abort_pending_key_sequence();
                }
                self.after_tab_change();
            },
            Ok(false) => {},
            Err(error) => self.message = Some(Message::error(error.to_string())),
        }
        Flow::Running
    }

    pub(super) fn close_tab(&mut self, tab: NodeId) -> Flow {
        if !self.workspace.close(tab) {
            self.message = Some(Message::error("that tab is no longer open"));
            return Flow::Running;
        }
        if self.workspace.is_empty() {
            if let Err(error) = self.add_scratch() {
                self.message = Some(Message::error(error.to_string()));
            }
        }
        self.after_tab_change();
        Flow::Running
    }

    pub(super) fn new_scratch(&mut self) -> Flow {
        if let Err(error) = self.add_scratch() {
            self.message = Some(Message::error(error.to_string()));
        }
        self.sync_viewport();
        Flow::Running
    }

    pub(super) fn leave_tab(&mut self) {
        if let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() {
            if self.search_open {
                session.search.close(editor);
            }
            editor.abort_pending_key_sequence();
        }
    }

    pub(super) fn after_tab_change(&mut self) {
        self.focus = crate::app::sidebar::Focus::Document;
        self.search_open = false;
        self.history_open = false;
        self.palette_open = false;
        self.sync_viewport();
    }
}
