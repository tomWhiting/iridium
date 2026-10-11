//! The terminal owns only the menu; the shared editor owns replacement commands.
use iridium_editor::{CompletionSession, KeyCode, KeyEvent};
use iridium_tui::{cell::CellBuffer, frame::Palette};

use super::{App, Flow, Message};

pub(super) struct Menu {
    session: CompletionSession,
    selected: usize,
}

impl App {
    pub(super) fn open_completion(&mut self) -> Flow {
        self.palette_open = false;
        let Some(editor) = self.editor() else {
            return Flow::Running;
        };
        if editor.state().read_only {
            self.message = Some(Message::error(
                "Completion cannot edit a read-only document",
            ));
            return Flow::Running;
        }
        match CompletionSession::new(&editor.state().document, &editor.state().cursor) {
            Ok(session) => {
                self.completion = Some(Menu {
                    session,
                    selected: 0,
                });
            },
            Err(error) => self.message = Some(Message::notice(error.to_string())),
        }
        Flow::Running
    }

    pub(super) fn drive_completion(&mut self, event: &KeyEvent) -> bool {
        let Some(mut menu) = self.completion.take() else {
            return false;
        };
        let direction = match event.key {
            KeyCode::Down => Some(1),
            KeyCode::Up => Some(-1),
            KeyCode::Char('n') if event.modifiers.ctrl => Some(1),
            KeyCode::Char('p') if event.modifiers.ctrl => Some(-1),
            _ => None,
        };
        if let Some(direction) = direction {
            let count = menu.session.candidates().len();
            menu.selected = if direction > 0 {
                (menu.selected + 1) % count
            } else {
                (menu.selected + count - 1) % count
            };
            self.completion = Some(menu);
            return true;
        }
        if event.key == KeyCode::Escape {
            return true;
        }
        if matches!(event.key, KeyCode::Enter | KeyCode::Tab) {
            let Some(editor) = self.workspace.active_editor_mut() else {
                return true;
            };
            if editor.state().read_only {
                self.message = Some(Message::error(
                    "Completion cannot edit a read-only document",
                ));
                return true;
            }
            match menu.session.accept(
                menu.selected,
                &editor.state().document,
                &editor.state().cursor,
            ) {
                Ok(command) => editor.apply_command(command),
                Err(error) => self.message = Some(Message::error(error.to_string())),
            }
            self.ensure_caret_visible();
            return true;
        }
        false
    }
}

impl Menu {
    pub(super) fn paint(
        &self,
        buffer: &mut CellBuffer,
        caret: Option<iridium_tui::frame::CellPosition>,
        palette: &Palette,
    ) {
        let Some(caret) = caret else {
            return;
        };
        let count = self.session.candidates().len().min(8);
        let width = 34.min(buffer.width());
        let column = caret.column.min(buffer.width().saturating_sub(width));
        let row = if caret.row + count + 1 < buffer.height() {
            caret.row + 1
        } else {
            caret.row.saturating_sub(count)
        };
        let first = self.selected.saturating_sub(count - 1);
        let background = " ".repeat(width);
        for (offset, word) in self
            .session
            .candidates()
            .iter()
            .enumerate()
            .skip(first)
            .take(count)
        {
            let style = palette.overlay_toggle(offset == self.selected);
            buffer.set_str(column, row + offset - first, &background, style);
            let label: String = word.chars().take(30).collect();
            buffer.set_str(column, row + offset - first, &label, style);
        }
    }
}
