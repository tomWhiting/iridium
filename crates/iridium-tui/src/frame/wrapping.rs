//! Terminal measurement of the kernel's source-preserving wrap rows.
use super::units::{cell_units, whole_cells};
use super::{CellPosition, FrameLayout, LineLayout, TextArea};
use iridium_editor::render::{
    Viewport,
    wrapping::{self, Unit},
};
use iridium_editor::{Editor, Position};
use std::{collections::BTreeMap, ops::Range};

#[derive(Clone, Debug)]
pub(super) struct Row {
    pub line: usize,
    pub cells: Range<usize>,
    pub index: usize,
}
#[derive(Clone, Debug, Default)]
pub(super) struct Wrapped {
    pub rows: Vec<Row>,
    pub lines: BTreeMap<usize, LineLayout>,
    pub error: Option<String>,
}

pub(super) fn measure(
    document: &iridium_editor::Document,
    tab_width: usize,
    line: usize,
    width: usize,
) -> Result<(LineLayout, Vec<Range<usize>>), String> {
    let layout = LineLayout::document_prefix(document, line, tab_width, usize::MAX)
        .map_err(|error| error.to_string())?;
    let units: Vec<_> = layout
        .clusters()
        .iter()
        .map(|cluster| Unit {
            end: cluster.column().saturating_add(cluster.width()),
            break_after: cluster.break_after,
        })
        .collect();
    let rows = wrapping::rows(&units, width);
    Ok((layout, rows))
}
impl Wrapped {
    pub fn build(editor: &Editor, width: usize, height: usize) -> Self {
        let mut view = Self::default();
        if width == 0 {
            return view;
        }
        let state = editor.state();
        let mut visual = state.viewport.first_line;
        let mut skip = whole_cells(state.viewport.scroll_offset_y);
        while view.rows.len() < height {
            let line = state.fold_state.visual_to_document_line(visual);
            if line >= state.document.line_count() {
                break;
            }
            let (layout, ranges) = match measure(
                &editor.state().document,
                editor.state().config.tab_width,
                line,
                width,
            ) {
                Ok(value) => value,
                Err(error) => {
                    view.error = Some(error);
                    break;
                },
            };
            skip = skip.min(ranges.len().saturating_sub(1));
            view.rows.extend(
                ranges
                    .into_iter()
                    .enumerate()
                    .skip(skip)
                    .take(height - view.rows.len())
                    .map(|(index, cells)| Row { line, cells, index }),
            );
            view.lines.insert(line, layout);
            skip = 0;
            visual += 1;
        }
        view
    }
    pub fn caret(&self, position: Position, origin: usize, width: usize) -> Option<CellPosition> {
        let cell = self
            .lines
            .get(&position.line)?
            .column_to_cell(position.column);
        let (row, segment) = self
            .rows
            .iter()
            .enumerate()
            .find(|(_, row)| row.line == position.line && row.cells.contains(&cell))?;
        Some(CellPosition {
            row,
            column: origin
                + cell
                    .saturating_sub(segment.cells.start)
                    .min(width.saturating_sub(1)),
        })
    }
    pub fn position(&self, row: usize, column: usize) -> Option<Position> {
        let segment = self.rows.get(row).or_else(|| self.rows.last())?;
        let layout = self.lines.get(&segment.line)?;
        let cell = segment
            .cells
            .start
            .saturating_add(column)
            .min(segment.cells.end);
        Some(Position::new(segment.line, layout.cell_to_column(cell)))
    }
}
impl FrameLayout {
    /// Scroll by visible rows, including continuation rows when wrapping is on.
    ///
    /// # Errors
    /// Propagates line-reader and bounded layout failures without partial scrolling.
    pub fn scroll_rows(&self, editor: &mut Editor, down: bool, count: usize) -> Result<(), String> {
        let mut viewport = editor.state().viewport.clone();
        let state = editor.state();
        let last = state
            .fold_state
            .visible_line_count(state.document.line_count())
            .saturating_sub(1);
        if !state.config.word_wrap || self.text.width == 0 {
            viewport.first_line = if down {
                viewport.first_line.saturating_add(count).min(last)
            } else {
                viewport.first_line.saturating_sub(count)
            };
            viewport.scroll_offset_y = 0.0;
        } else {
            let mut offset = whole_cells(viewport.scroll_offset_y);
            for _ in 0..count {
                if down {
                    let line = state
                        .fold_state
                        .visual_to_document_line(viewport.first_line);
                    let (_, ranges) = measure(
                        &editor.state().document,
                        editor.state().config.tab_width,
                        line,
                        self.text.width,
                    )?;
                    if offset + 1 < ranges.len() {
                        offset += 1;
                    } else if viewport.first_line < last {
                        viewport.first_line += 1;
                        offset = 0;
                    }
                } else if offset > 0 {
                    offset -= 1;
                } else if viewport.first_line > 0 {
                    viewport.first_line -= 1;
                    let line = state
                        .fold_state
                        .visual_to_document_line(viewport.first_line);
                    offset = measure(
                        &editor.state().document,
                        editor.state().config.tab_width,
                        line,
                        self.text.width,
                    )?
                    .1
                    .len()
                    .saturating_sub(1);
                }
            }
            viewport.scroll_offset_y = cell_units(offset);
        }
        editor.set_viewport(viewport);
        Ok(())
    }
    /// Reveal the primary caret in both dimensions, without changing selections.
    ///
    /// # Errors
    /// Reports a line measurement failure instead of inventing a column.
    pub fn reveal_caret(&self, editor: &mut Editor) -> Result<(), String> {
        let head = editor.cursor();
        if self.text.width == 0 || self.text_rows == 0 {
            return Ok(());
        }
        if let Some(wrapped) = &self.wrapped {
            if wrapped
                .caret(head, self.text.origin, self.text.width)
                .is_some()
            {
                return Ok(());
            }
            let (layout, ranges) = measure(
                &editor.state().document,
                editor.state().config.tab_width,
                head.line,
                self.text.width,
            )?;
            let cell = layout.column_to_cell(head.column);
            let row = ranges
                .iter()
                .position(|range| range.contains(&cell))
                .ok_or("caret is outside measured rows")?;
            let mut viewport = editor.state().viewport.clone();
            viewport.first_line = editor
                .state()
                .fold_state
                .document_to_visual_line(head.line)
                .ok_or("caret is folded")?;
            viewport.scroll_offset_y = cell_units(row);
            viewport.scroll_offset_x = 0.0;
            editor.set_viewport(viewport);
            self.scroll_rows(editor, false, self.text_rows.saturating_sub(1))?;
        } else {
            editor.reveal_in_viewport(head);
            let end = head
                .column
                .saturating_add(1)
                .saturating_mul(editor.state().config.tab_width.max(2));
            let layout = LineLayout::document_prefix(
                &editor.state().document,
                head.line,
                editor.state().config.tab_width,
                end,
            )
            .map_err(|error| error.to_string())?;
            if head.column > layout.char_count() {
                return Err("caret exceeds measured prefix".into());
            }
            let cell = layout.column_to_cell(head.column);
            let mut viewport = editor.state().viewport.clone();
            let scroll = whole_cells(viewport.scroll_offset_x);
            if cell < scroll {
                viewport.scroll_offset_x = cell_units(cell);
            } else if cell >= scroll.saturating_add(self.text.width) {
                viewport.scroll_offset_x = cell_units(cell.saturating_sub(self.text.width - 1));
            }
            editor.set_viewport(viewport);
        }
        Ok(())
    }
    /// Bounded horizontal panning over visible document lines. Wrapping ignores it.
    ///
    /// # Errors
    /// Reports line measurement failures without partially changing the viewport.
    pub fn scroll_columns(
        &self,
        editor: &mut Editor,
        right: bool,
        count: usize,
    ) -> Result<(), String> {
        if editor.state().config.word_wrap {
            return Ok(());
        }
        let target = if right {
            let wanted = self.text.scroll.saturating_add(count);
            let mut maximum = 0;
            let state = editor.state();
            for line in self
                .viewport
                .visible_document_lines(&state.fold_state, state.document.line_count())
            {
                let layout = LineLayout::document_prefix(
                    &state.document,
                    line,
                    state.config.tab_width,
                    wanted.saturating_add(self.text.width),
                )
                .map_err(|error| error.to_string())?;
                maximum = maximum.max(
                    layout
                        .width()
                        .saturating_sub(self.text.width.saturating_sub(1)),
                );
            }
            wanted.min(maximum.max(self.text.scroll))
        } else {
            self.text.scroll.saturating_sub(count)
        };
        let mut viewport: Viewport = editor.state().viewport.clone();
        viewport.scroll_offset_x = cell_units(target);
        editor.set_viewport(viewport);
        Ok(())
    }
    pub(super) fn row_area(&self, row: &Row) -> TextArea {
        TextArea {
            origin: self.text.origin,
            width: self
                .text
                .width
                .min(row.cells.end.saturating_sub(row.cells.start)),
            scroll: row.cells.start,
        }
    }
}
