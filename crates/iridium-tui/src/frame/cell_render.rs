//! Paints prepared cell rows; wrapping and source freshness belong to the kernel borrow.

use iridium_editor::Range;

use super::prepared_cells::PreparedRow;
use super::render::fill_text_area;
use super::{
    CellFrameError, CellFrameLayout, Frame, Palette, PreparedCellFrame, gutter, status, text,
};
use crate::cell::CellBuffer;

impl Frame {
    /// Paints a prepared borrow into its exact local extent. An extent error
    /// occurs before the buffer or highlighter cache is changed.
    pub fn render_cells(
        &mut self,
        prepared: &PreparedCellFrame<'_>,
        buffer: &mut CellBuffer,
    ) -> Result<CellFrameLayout, CellFrameError> {
        self.render_cells_with_primary_caret(prepared, buffer, true)
    }

    /// Paints the same borrowed frame with explicit ownership of primary caret
    /// paint. Set `paint_primary` to false when the host supplies its own cursor.
    /// Secondary carets, selection/search layers and layout caret hints remain
    /// unchanged, including secondary carets coincident with the primary.
    ///
    /// An extent error occurs before the buffer or highlighter cache is changed.
    pub fn render_cells_with_primary_caret(
        &mut self,
        prepared: &PreparedCellFrame<'_>,
        buffer: &mut CellBuffer,
        paint_primary: bool,
    ) -> Result<CellFrameLayout, CellFrameError> {
        let expected = (prepared.layout.columns, prepared.layout.rows);
        let actual = (buffer.width(), buffer.height());
        if expected != actual {
            return Err(CellFrameError::ExtentMismatch {
                prepared: expected,
                buffer: actual,
            });
        }
        #[cfg(feature = "syntax")]
        {
            let window = prepared.visible.first().map_or(0, |row| row.document_line)
                ..prepared
                    .visible
                    .last()
                    .map_or(0, |row| row.document_line.saturating_add(1));
            self.refresh(prepared.editor, window);
        }
        let palette = Palette::from_theme(prepared.editor.get_theme());
        buffer.fill(palette.text());
        for (index, row) in prepared.visible.iter().enumerate() {
            Self::paint_cell_row(
                #[cfg(feature = "syntax")]
                self,
                prepared,
                row,
                index,
                buffer,
                &palette,
            );
        }
        // The primary is first only when it is visible in this prepared frame.
        let skip_primary = usize::from(!paint_primary && prepared.layout.primary_caret.is_some());
        for caret in prepared.carets.iter().skip(skip_primary) {
            if let Some(cell) = buffer.get(caret.position.column, caret.position.row) {
                buffer.set_style(
                    caret.position.column,
                    caret.position.row,
                    palette.caret(cell.style()),
                );
            }
        }
        let mut layout = prepared.layout.clone();
        if let Some(chrome) = prepared.options.chrome {
            if let (Some(overlay), Some((first, rows))) = (chrome.search, layout.search_rows) {
                layout.search_caret = overlay.paint(buffer, first, rows, prepared.editor, &palette);
            }
            if let Some(row) = layout.status_row {
                status::paint(buffer, row, prepared.editor, chrome.status, &palette);
            }
        }
        Ok(layout)
    }

    fn paint_cell_row(
        #[cfg(feature = "syntax")] frame: &Self,
        prepared: &PreparedCellFrame<'_>,
        row: &PreparedRow,
        index: usize,
        buffer: &mut CellBuffer,
        palette: &Palette,
    ) {
        let background = if row.active && prepared.editor.state().config.highlight_current_line {
            palette.current_line()
        } else {
            palette.text().background
        };
        let area = prepared.layout.text;
        fill_text_area(
            buffer,
            index,
            area,
            palette.text().with_background(background),
        );
        if row.first {
            gutter::paint(
                buffer,
                prepared.editor,
                row.document_line,
                index,
                row.active,
                palette,
                gutter::GutterArea {
                    origin: prepared.layout.sidebar_columns,
                    width: prepared.layout.gutter_width,
                },
            );
        }
        #[cfg(feature = "syntax")]
        let styles = frame.cluster_styles(row.byte_start, &row.layout, palette, background);
        #[cfg(not(feature = "syntax"))]
        let styles = std::iter::repeat_n(
            palette.text().with_background(background),
            row.layout.clusters().len(),
        );
        for (cluster, style) in row.layout.clusters().iter().zip(styles) {
            text::paint_cluster(buffer, index, cluster, style, area);
        }
        if let Some(placeholder) = &row.placeholder {
            if let Some(offset) = row
                .layout
                .width()
                .checked_add(1)
                .filter(|offset| *offset < area.width)
            {
                let placeholder_area = super::TextArea {
                    origin: area.origin + offset,
                    width: area.width - offset,
                    scroll: 0,
                };
                for cluster in placeholder.clusters() {
                    text::paint_cluster(
                        buffer,
                        index,
                        cluster,
                        palette.gutter(false),
                        placeholder_area,
                    );
                }
            }
        }
        for selection in prepared.editor.state().cursor.all_selections() {
            if !selection.is_collapsed() {
                paint_range(
                    buffer,
                    index,
                    row,
                    Range::new(selection.start(), selection.end()),
                    area,
                    |style| palette.selected(style),
                );
            }
        }
        // Preserve existing order: all non-current matches, then current.
        for current in [false, true] {
            for &(range, is_current) in &prepared.matches {
                if current == is_current {
                    paint_range(buffer, index, row, range, area, |style| {
                        palette.search_match(style, current)
                    });
                }
            }
        }
    }
}

fn paint_range(
    buffer: &mut CellBuffer,
    index: usize,
    row: &PreparedRow,
    range: Range,
    area: super::TextArea,
    restyle: impl Fn(crate::cell::Style) -> crate::cell::Style,
) {
    if row.document_line < range.start.line || row.document_line > range.end.line {
        return;
    }
    let from = if row.document_line == range.start.line {
        row.layout.column_to_cell(range.start.column)
    } else {
        0
    };
    let to = if row.document_line == range.end.line {
        row.layout.column_to_cell(range.end.column)
    } else {
        // Only the last continuation owns the real newline. Blank cells after
        // a short soft-wrapped row are not source characters or line breaks.
        row.layout
            .width()
            .min(area.width)
            .saturating_add(usize::from(row.last))
    };
    text::restyle_columns(buffer, index, from..to, area, restyle);
}
