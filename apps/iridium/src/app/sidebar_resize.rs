//! Sidebar geometry controls; content navigation does not own the divider.
use super::{App, Flow, Message};
use iridium_tui::input::{MouseButton, MouseEvent, MouseEventKind};

impl App {
    pub(super) fn resize_sidebar_command(&mut self, wider: bool) -> Flow {
        let width = self.sidebar_columns();
        if width == 0 {
            self.message = Some(Message::notice("Open a sidebar before resizing it"));
        } else {
            self.set_sidebar_columns(if wider {
                width.saturating_add(4)
            } else {
                width.saturating_sub(4)
            });
        }
        Flow::Running
    }

    fn set_sidebar_columns(&mut self, requested: usize) {
        let before = self.sidebar_columns();
        if let Some(panel) = self.explorer.as_mut() {
            // The shared geometry enforces the minimum. Cap the stored request
            // at the current screen so a drag cannot accumulate hidden width.
            panel.set_sidebar_width(requested.min(self.columns / 2));
        }
        self.resync_after_band_change(before);
        self.ensure_caret_visible();
    }

    pub(super) fn resize_sidebar_mouse(&mut self, event: MouseEvent) -> bool {
        if let Some((start_column, start_width)) = self.sidebar_drag {
            match event.kind {
                MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                    let delta = usize::from(event.column.abs_diff(start_column));
                    self.set_sidebar_columns(if event.column >= start_column {
                        start_width.saturating_add(delta)
                    } else {
                        start_width.saturating_sub(delta)
                    });
                    if matches!(event.kind, MouseEventKind::Up(_)) {
                        self.sidebar_drag = None;
                    }
                    return true;
                },
                _ => self.sidebar_drag = None,
            }
        }
        let width = self.sidebar_columns();
        if event.kind == MouseEventKind::Down(MouseButton::Left)
            && width > 0
            && usize::from(event.column) == width - 1
            && usize::from(event.row) < self.band_rows()
        {
            self.stop_mouse_drag();
            self.sidebar_drag = Some((event.column, width));
            return true;
        }
        false
    }
}
