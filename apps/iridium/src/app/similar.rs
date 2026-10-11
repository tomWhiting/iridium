//! Modal structural preview, retaining the source identity and original cursors.
use super::{App, Flow, Message};
use iridium_editor::{
    CursorState, KeyCode, KeyEvent, SyntaxSimilarity, render::Viewport, workspace::NodeId,
};
pub(super) struct Preview {
    tab: NodeId,
    revision: u64,
    original: CursorState,
    viewport: Viewport,
    mode: SyntaxSimilarity,
    valid: bool,
    index: usize,
    matches: CursorState,
}
impl App {
    pub(super) fn open_similar(&mut self) -> Flow {
        self.palette_open = false;
        let Some(tab) = self.workspace.active() else {
            return Flow::Running;
        };
        let Some(editor) = self.editor() else {
            return Flow::Running;
        };
        let original = editor.state().cursor.clone();
        let mut preview = Preview {
            tab,
            revision: editor.state().document.revision(),
            original: original.clone(),
            viewport: editor.state().viewport.clone(),
            mode: SyntaxSimilarity::Kind,
            valid: false,
            index: 0,
            matches: original,
        };
        self.refresh_similar(&mut preview);
        self.similar = Some(preview);
        Flow::Running
    }
    fn refresh_similar(&mut self, preview: &mut Preview) {
        let Some(editor) = self.workspace.active_editor_mut() else {
            return;
        };
        editor.set_selections(preview.original.clone());
        match editor.similar_syntax_selections(preview.mode) {
            Ok(matches) => {
                preview.valid = true;
                preview.index = 0;
                preview.matches = matches.clone();
                editor.set_selections(matches);
                self.similar_notice(preview);
            },
            Err(error) => {
                preview.valid = false;
                self.message = Some(Message::error(format!(
                    "{error} · F2 kind/shape · Esc restores"
                )));
            },
        }
        self.ensure_caret_visible();
    }
    fn similar_notice(&mut self, preview: &Preview) {
        let mode = match preview.mode {
            SyntaxSimilarity::Kind => "kind",
            SyntaxSimilarity::Shape => "shape",
        };
        self.message = Some(Message::notice(format!(
            "Similar {mode} · {} matches · F2 kind/shape · ↑↓ inspect · Enter keep · Esc restore",
            preview.matches.cursor_count()
        )));
    }
    pub(super) fn drive_similar(&mut self, event: &KeyEvent) -> Flow {
        let Some(mut preview) = self.similar.take() else {
            return Flow::Running;
        };
        let current = self.workspace.active() == Some(preview.tab)
            && self
                .editor()
                .is_some_and(|editor| editor.state().document.revision() == preview.revision);
        if !current {
            self.message = Some(Message::error(
                "Similar-node preview cancelled: document changed",
            ));
            return Flow::Running;
        }
        match event.key {
            KeyCode::Escape => {
                if let Some(editor) = self.workspace.active_editor_mut() {
                    editor.set_selections(preview.original);
                    editor.set_viewport(preview.viewport);
                }
                self.message = Some(Message::notice("Original selections restored"));
                return Flow::Running;
            },
            KeyCode::Enter if preview.valid => {
                self.select_mode();
                self.message = Some(Message::notice(format!(
                    "{} similar nodes selected · c change · y copy · u undo edits",
                    preview.matches.cursor_count()
                )));
                return Flow::Running;
            },
            KeyCode::F2 => {
                preview.mode = if preview.mode == SyntaxSimilarity::Kind {
                    SyntaxSimilarity::Shape
                } else {
                    SyntaxSimilarity::Kind
                };
                self.refresh_similar(&mut preview);
            },
            KeyCode::Up | KeyCode::Down if preview.valid => {
                let count = preview.matches.cursor_count();
                preview.index = if event.key == KeyCode::Down {
                    (preview.index + 1) % count
                } else {
                    (preview.index + count - 1) % count
                };
                let mut selections: Vec<_> = preview.matches.all_selections().copied().collect();
                let primary = selections.remove(preview.index);
                if let Some(editor) = self.workspace.active_editor_mut() {
                    editor.set_selections(CursorState::from_selections(primary, selections));
                }
                self.ensure_caret_visible();
                self.similar_notice(&preview);
            },
            _ => {},
        }
        self.similar = Some(preview);
        Flow::Running
    }
}

#[cfg(test)]
mod tests;
