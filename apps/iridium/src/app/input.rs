//! Input routing and transitions into selection mode.
use super::{App, Flow, Message};
use iridium_editor::{ClipboardOperation, KeyEvent};
use iridium_tui::frame::SearchOutcome;

impl App {
    /// Handles one key press.
    pub(super) fn handle_key(&mut self, event: &KeyEvent) -> Flow {
        self.stop_mouse_drag();
        if self.drive_completion(event) { return Flow::Running; }
        if self.similar.is_some() {
            return self.drive_similar(event);
        }
        if self.project_search_open() {
            return self.drive_project_search(event);
        }
        if self.query.is_some() {
            return self.answer_query(event);
        }
        // A message describes the key before this one.
        self.message = None;

        if self.prompt.is_some() {
            return self.answer_prompt(event);
        }

        if self.palette_open {
            return self.drive_palette(event);
        }

        if self.history_open {
            return self.drive_history(event);
        }

        // Only the focused explorer receives keys: its
        // printable characters go to its filter, so a key
        // that fell through here would be a key typed into the document while
        // the person was looking at a file list.
        if self.focus == crate::app::sidebar::Focus::Explorer {
            if let Some(flow) = self.explorer_host_key(event) {
                return flow;
            }
            if event.key == iridium_editor::KeyCode::Escape
                && self
                    .explorer
                    .as_ref()
                    .is_some_and(|panel| panel.is_sidebar() && !panel.has_unapplied_edits())
            {
                self.focus = crate::app::sidebar::Focus::Document;
                return Flow::Running;
            }
            return self.drive_explorer(event);
        }

        if self.search_open {
            let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() else {
                return Flow::Running;
            };
            match session.search.handle_key(event, editor) {
                // The panel leaves what it does not bind to the host, so a save
                // is not dead while a search is open.
                SearchOutcome::Ignored => {},
                SearchOutcome::CutRequested(cut) => {
                    self.cut_search(cut);
                    return Flow::Running;
                },
                SearchOutcome::Copied(text) => {
                    self.clipboard(ClipboardOperation::Copy(text));
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
                SearchOutcome::PasteRequested => {
                    self.clipboard(ClipboardOperation::Paste);
                    return Flow::Running;
                },
                SearchOutcome::Handled | SearchOutcome::Replaced(_) => {
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
                SearchOutcome::Selected(count) => {
                    self.select_mode();
                    self.search_open = false;
                    self.sync_viewport();
                    self.ensure_caret_visible();
                    self.message = Some(Message::notice(format!(
                        "{count} selections — c changes, y copies"
                    )));
                    return Flow::Running;
                },
                SearchOutcome::Closed => {
                    self.search_open = false;
                    self.sync_viewport();
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
            }
        }

        let Some(editor) = self.workspace.active_editor_mut() else {
            return Flow::Running;
        };
        let before = editor.state().cursor.clone();
        let revision = editor.state().document.revision();
        let result = editor.handle_key(event);
        let selected = editor.state().document.revision() == revision
            && editor.state().cursor != before
            && editor
                .state()
                .cursor
                .all_selections()
                .any(|selection| !selection.is_collapsed());
        let flow = self.consume(result);
        if selected {
            self.select_mode();
        }
        self.ensure_caret_visible();
        flow
    }
}
