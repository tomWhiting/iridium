//! Persistent explorer focus, independent of document focus.
use super::{App, Flow, Message};
use iridium_tui::frame::ExplorerAction;
use iridium_tui::input::{MouseButton, MouseEvent, MouseEventKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Document,
    Explorer,
}

impl App {
    pub(super) fn toggle_sidebar(&mut self) -> Flow {
        let before = self.sidebar_columns();
        if self
            .explorer
            .as_ref()
            .is_some_and(iridium_tui::frame::FileExplorerPanel::is_sidebar)
        {
            if let Some(panel) = self
                .explorer
                .as_ref()
                .filter(|panel| panel.has_unapplied_edits())
            {
                self.message = Some(Message::error(panel.unapplied_edits_message()));
                return Flow::Running;
            }
            self.close_explorer();
        } else {
            if self.explorer.is_none() {
                self.toggle_explorer();
            }
            if let Some(panel) = self.explorer.as_mut() {
                panel.set_sidebar(true);
            }
            self.focus = if self.explorer.is_some() {
                crate::app::sidebar::Focus::Explorer
            } else {
                crate::app::sidebar::Focus::Document
            };
        }
        self.resync_after_band_change(before);
        Flow::Running
    }

    pub(super) fn explorer_action(&mut self, action: ExplorerAction) -> Flow {
        match action {
            ExplorerAction::Handled => Flow::Running,
            ExplorerAction::Close => {
                self.close_explorer();
                Flow::Running
            },
            ExplorerAction::Open(path) => self.open_file(&path),
            ExplorerAction::Report(message) => {
                self.message = Some(Message::error(message));
                Flow::Running
            },
        }
    }

    pub(super) fn explorer_mouse(&mut self, event: MouseEvent) -> Option<Flow> {
        if self.focus != crate::app::sidebar::Focus::Explorer && self.sidebar_columns() == 0 {
            return None;
        }
        if self.mouse_anchor.is_some()
            && matches!(event.kind, MouseEventKind::Drag(_) | MouseEventKind::Up(_))
        {
            return None;
        }
        let rows = self.band_rows();
        let theme = self.editor()?.get_theme().clone();
        let panel = self.explorer.as_mut()?;
        let action = panel.handle_mouse(event, self.columns, self.rows, rows, &theme);
        if let Some(action) = action {
            if matches!(
                event.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Scroll(_)
            ) {
                self.focus = crate::app::sidebar::Focus::Explorer;
                self.stop_mouse_drag();
            }
            return Some(self.explorer_action(action));
        }
        if panel.is_sidebar() {
            if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
                self.focus = crate::app::sidebar::Focus::Document;
            }
            None
        } else {
            Some(Flow::Running)
        }
    }
}
