//! Resolve painted cells to source positions; decorations never become text.
use super::units::cell_units;
use super::{CellPosition, FrameLayout, LineLayout};
use iridium_editor::{Editor, Position};

impl FrameLayout {
    /// Resolves a document row to a source position. The gutter clamps to the
    /// visible text's left edge; line padding clamps to the line's end. Sidebar,
    /// tab, search and status cells are outside the document and return `None`.
    #[must_use]
    pub fn document_position(&self, editor: &Editor, cell: CellPosition) -> Option<Position> {
        if self.text.width == 0 || cell.row >= self.text_rows || cell.column < self.sidebar_columns
        {
            return None;
        }
        if let Some(wrapped) = &self.wrapped {
            return wrapped.position(
                cell.row,
                cell.column
                    .saturating_sub(self.text.origin)
                    .min(self.text.width.saturating_sub(1)),
            );
        }
        let state = editor.state();
        let line = self
            .viewport
            .document_line_at_y(cell_units(cell.row), &state.fold_state);
        let last = state.document.line_count().checked_sub(1)?;
        let layout = match LineLayout::document_prefix(
            &state.document,
            line.min(last),
            state.config.tab_width,
            self.text
                .scroll
                .saturating_add(self.text.width)
                .saturating_add(1),
        ) {
            Ok(layout) => layout,
            Err(error) => {
                tracing::warn!(%error,"cannot hit-test the displayed line");
                return None;
            },
        };
        let column = if line > last {
            state.document.line_len(last)?
        } else {
            let offset = cell
                .column
                .saturating_sub(self.text.origin)
                .min(self.text.width);
            layout.cell_to_column(self.text.scroll.saturating_add(offset))
        };
        Some(Position::new(line.min(last), column))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{Chrome, Frame};

    #[test]
    fn hit_geometry_agrees_with_draw_geometry_without_computing_a_caret() {
        let mut editor = Editor::new(iridium_editor::EditorConfig::default());
        editor.set_content("one\n\t漢e\u{301}\nthree");
        editor.set_cursor(Position::new(1, 1));
        let chrome = Chrome {
            tab_bar: true,
            sidebar_columns: 4,
            ..Chrome::default()
        };
        let drawn = Frame::layout(&editor, 35, 10, chrome);
        let hit = Frame::hit_layout(&editor, 35, 10, chrome);
        assert!(drawn.primary_caret.is_some());
        assert!(hit.primary_caret.is_none());
        assert_eq!(
            (drawn.text_rows, drawn.tab_row, drawn.status_row),
            (hit.text_rows, hit.tab_row, hit.status_row)
        );
        for row in 0..10 {
            for column in 0..35 {
                let cell = CellPosition { column, row };
                assert_eq!(
                    drawn.document_position(&editor, cell),
                    hit.document_position(&editor, cell)
                );
            }
        }
    }
}
