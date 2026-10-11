//! Mouse inspection, result choice and replacement-field selection.
use super::{Focus, Review};
use iridium_editor::{KeyCode, KeyEvent};
use iridium_tui::input::{
    MouseButton, MouseEvent, MouseEventKind, MouseModifiers, ScrollDirection,
};
impl Review {
    pub(in crate::app::project_search) fn mouse(
        &mut self,
        event: MouseEvent,
        width: usize,
        height: usize,
    ) {
        let (column, row) = (usize::from(event.column), usize::from(event.row));
        if self.detail.is_some() {
            if let MouseEventKind::Scroll(direction) = event.kind {
                let key = match direction {
                    ScrollDirection::Up => Some(KeyCode::PageUp),
                    ScrollDirection::Down => Some(KeyCode::PageDown),
                    ScrollDirection::Left | ScrollDirection::Right => None,
                };
                if let Some(key) = key {
                    self.key(&KeyEvent::simple(key), width, height);
                }
            }
            return;
        }
        if self.operation.is_some() {
            return;
        }
        let rows = height.saturating_sub(4);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.stop_drag();
                if row == 1 && height > 3 && column >= 9 && column < width && !self.completed {
                    self.focus = Focus::SelectingReplacement;
                    self.entry.point(
                        column,
                        width,
                        "Replace: ",
                        event.modifiers.contains(MouseModifiers::SHIFT),
                    );
                } else if row >= 2 && row < height.saturating_sub(2) {
                    let start = self.selected.saturating_sub(rows.saturating_sub(1));
                    let index = start + row - 2;
                    let count = if self.completed {
                        self.outcomes.len()
                    } else {
                        self.hits.len()
                    };
                    if index < count {
                        self.selected = index;
                        if !self.completed {
                            self.focus = Focus::Results;
                            if (2..=4).contains(&column) {
                                self.included[index] = !self.included[index];
                            }
                        }
                    }
                }
            },
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                if self.focus == Focus::SelectingReplacement {
                    self.entry.point(column, width, "Replace: ", true);
                }
                if matches!(event.kind, MouseEventKind::Up(_)) {
                    self.stop_drag();
                }
            },
            MouseEventKind::Scroll(direction) => {
                self.stop_drag();
                let count = if self.completed {
                    self.outcomes.len()
                } else {
                    self.hits.len()
                };
                match direction {
                    ScrollDirection::Up => self.selected = self.selected.saturating_sub(3),
                    ScrollDirection::Down => {
                        self.selected =
                            self.selected.saturating_add(3).min(count.saturating_sub(1));
                    },
                    ScrollDirection::Left | ScrollDirection::Right => {},
                }
            },
            _ => {},
        }
    }
}
