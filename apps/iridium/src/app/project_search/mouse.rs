//! Termina mouse events stay inside project fields/results.
use super::panel::{Field, Panel};
use iridium_tui::input::{
    MouseButton, MouseEvent, MouseEventKind, MouseModifiers, ScrollDirection,
};
impl Panel {
    pub(in crate::app) fn mouse(&mut self, event: MouseEvent, width: usize, height: usize) {
        if matches!(event.kind, MouseEventKind::Down(_)) {
            self.clipboard_keys.clear();
            self.clipboard_message = None;
        }
        if let Some(review) = self.review.as_mut() {
            review.mouse(event, width, height);
            return;
        }
        let (column, row) = (usize::from(event.column), usize::from(event.row));
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.mouse_drag = None;
                self.clipboard_keys.clear();
                self.clipboard_message = None;
                let field = match row {
                    1 if height > 3 && column >= 6 && column < width => Some(Field::Pattern),
                    2 if height > 4 && column >= 7 && column < width => Some(Field::Globs),
                    _ => None,
                };
                if let Some(field) = field {
                    self.field = field;
                    self.mouse_drag = Some(field);
                    self.point_field(
                        field,
                        column,
                        width,
                        event.modifiers.contains(MouseModifiers::SHIFT),
                    );
                } else if !self.errors && row >= 3 && row < height.saturating_sub(2) {
                    let rows = height.saturating_sub(5);
                    let start = self.selected.saturating_sub(rows.saturating_sub(1));
                    let index = start + row - 3;
                    if index < self.matches.len() {
                        self.selected = index;
                    }
                }
            },
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left) => {
                if let Some(field) = self.mouse_drag {
                    self.point_field(field, column, width, true);
                }
                if matches!(event.kind, MouseEventKind::Up(_)) {
                    self.mouse_drag = None;
                }
            },
            MouseEventKind::Scroll(direction) if !self.errors => {
                self.mouse_drag = None;
                match direction {
                    ScrollDirection::Up => self.selected = self.selected.saturating_sub(3),
                    ScrollDirection::Down => {
                        self.selected = self
                            .selected
                            .saturating_add(3)
                            .min(self.matches.len().saturating_sub(1));
                    },
                    ScrollDirection::Left | ScrollDirection::Right => {},
                }
            },
            _ => {},
        }
    }
    fn point_field(&mut self, field: Field, column: usize, width: usize, extend: bool) {
        match field {
            Field::Pattern => self.entry.point(column, width, "Find: ", extend),
            Field::Globs => self.globs.point(column, width, "Globs: ", extend),
        }
    }
}

#[cfg(test)]
mod tests;
