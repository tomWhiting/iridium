//! Legacy unwrapped frame rendering, preserving the existing viewport and chrome.

use super::highlight::Highlighting;
use super::search::{MatchHighlights, MatchedLine};
use super::types::{HighlightSource, PaintedLine};
use super::units::whole_cells;
use super::{Chrome, Frame, FrameLayout, LineLayout, Palette, TextArea};
use super::{geometry, gutter, highlight, status, text};
use crate::cell::{CellBuffer, Color, Style};
use iridium_editor::{Editor, Selection};
use std::collections::HashSet;

impl Frame {
    /// A renderer with nothing cached yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The geometry a screen of this size would be drawn with.
    ///
    /// Computed from editor state alone, so it agrees with what
    /// [`Frame::render`] will do without drawing anything. `search_caret` is
    /// the one field it cannot fill: whether the panel's focused field had room
    /// to be drawn is decided while drawing it.
    pub fn layout(editor: &Editor, columns: usize, rows: usize, chrome: Chrome<'_>) -> FrameLayout {
        geometry::layout(editor, columns, rows, chrome)
    }

    /// Writes the cell-unit geometry of a screen of this size into the kernel's
    /// viewport.
    ///
    /// The kernel's scroll verbs — `ensure_cursor_visible`,
    /// `scroll_to_position_with_folds` — work from the viewport's own size, so
    /// a host that never calls this scrolls against a viewport still measured
    /// in pixels and will disagree with what is on screen. Call it when the
    /// terminal is sized, whenever it is resized, and whenever the search panel
    /// opens or closes.
    ///
    /// Scroll position is left alone: where the document is scrolled to is
    /// kernel state, and a resize does not move it.
    pub fn sync_viewport(editor: &mut Editor, columns: usize, rows: usize, chrome: Chrome<'_>) {
        geometry::sync_viewport(editor, columns, rows, chrome);
    }

    /// Draws one frame into `buffer`, replacing everything in it.
    ///
    /// The buffer is filled rather than patched: damage is computed by the
    /// [`Surface`](crate::cell::Surface) from the difference between frames,
    /// and a renderer that skipped unchanged cells here would be declaring
    /// damage — which is the one thing nothing outside the buffer gets to do.
    pub fn render(
        &mut self,
        editor: &Editor,
        chrome: Chrome<'_>,
        buffer: &mut CellBuffer,
    ) -> FrameLayout {
        let mut geometry = Self::layout(editor, buffer.width(), buffer.height(), chrome);

        let state = editor.state();
        let total_lines = state.document.line_count();
        let visible: Vec<usize> = geometry
            .viewport
            .visible_document_lines(&state.fold_state, total_lines)
            .collect();
        // The highlight cache derives spans for this frame's window only —
        // the whole-document derive was the parser tax
        // (docs/design/PARSER-TAX-MAP.md). An empty screen requests an empty
        // window; nothing will be painted from it.
        let window = visible.first().copied().unwrap_or(0)
            ..visible.last().map_or(0, |&last| last.saturating_add(1));
        self.refresh(editor, window);

        let palette = Palette::from_theme(editor.get_theme());
        buffer.fill(palette.text());

        let cursor_lines: HashSet<usize> = state
            .cursor
            .all_selections()
            .map(|selection| selection.head.line)
            .collect();
        let highlights = match (visible.first(), visible.last()) {
            (Some(&first), Some(&last)) => MatchHighlights::collect(editor, first, last),
            _ => MatchHighlights::default(),
        };

        for document_line in visible {
            let Some(y) = geometry
                .viewport
                .screen_y_for_line(document_line, &state.fold_state)
            else {
                continue;
            };
            let row = whole_cells(y);
            if row >= geometry.text_rows {
                continue;
            }
            self.paint_line(
                buffer,
                editor,
                &palette,
                &geometry,
                &highlights,
                PaintedLine {
                    document_line,
                    row,
                    is_active: cursor_lines.contains(&document_line),
                },
            );
        }

        if let (Some(overlay), Some((first_row, row_count))) = (chrome.search, geometry.search_rows)
        {
            geometry.search_caret = overlay.paint(buffer, first_row, row_count, editor, &palette);
        }

        if let Some(row) = geometry.status_row {
            status::paint(buffer, row, editor, chrome.status, &palette);
        }

        geometry
    }

    /// Paints one document line: its gutter entry, its text, its selections
    /// and its carets.
    fn paint_line(
        &self,
        buffer: &mut CellBuffer,
        editor: &Editor,
        palette: &Palette,
        geometry: &FrameLayout,
        highlights: &MatchHighlights,
        placement: PaintedLine,
    ) {
        let PaintedLine {
            document_line,
            row,
            is_active,
        } = placement;
        let state = editor.state();
        let text = state.document.line(document_line).unwrap_or_default();
        let layout = LineLayout::new(&text, state.config.tab_width);

        let background = if is_active && state.config.highlight_current_line {
            palette.current_line()
        } else {
            palette.text().background
        };
        let blank = palette.text().with_background(background);
        fill_text_area(buffer, row, geometry.text, blank);
        gutter::paint(
            buffer,
            editor,
            document_line,
            row,
            is_active,
            palette,
            geometry.gutter_area(),
        );

        let line_start = state
            .document
            .line_to_byte_offset(document_line)
            .unwrap_or(0);
        let styles = self.cluster_styles(line_start, &layout, palette, background);
        for (cluster, style) in layout.clusters().iter().zip(styles) {
            text::paint_cluster(buffer, row, cluster, style, geometry.text);
        }

        if editor.is_folded(document_line) {
            let placeholder = fold_placeholder(editor, document_line);
            text::paint_text(
                buffer,
                row,
                layout.width() + 1,
                &placeholder,
                palette.gutter(false),
                geometry.text,
            );
        }

        for selection in state.cursor.all_selections() {
            paint_selection(
                buffer,
                row,
                document_line,
                &layout,
                selection,
                palette,
                geometry.text,
            );
        }

        highlights.paint(
            buffer,
            row,
            MatchedLine {
                number: document_line,
                layout: &layout,
            },
            palette,
            geometry.text,
        );

        for selection in state.cursor.all_selections() {
            if selection.head.line != document_line {
                continue;
            }
            let cell = layout.column_to_cell(selection.head.column);
            text::restyle_columns(buffer, row, cell..cell + 1, geometry.text, |style| {
                palette.caret(style)
            });
        }
    }

    /// The style of every cluster of one line, syntax highlighting included.
    ///
    /// Highlight spans are byte ranges into the whole document, so each cluster
    /// is resolved by its own absolute byte offset. Overlapping spans — one
    /// capture nested inside another — are painted outermost first, so the
    /// innermost wins, which is why the query result is sorted by start
    /// ascending and end *descending*.
    /// The style of every cluster on one line, from the highlight cache.
    pub(super) fn cluster_styles(
        &self,
        line_start: usize,
        layout: &LineLayout,
        palette: &Palette,
        background: Color,
    ) -> Vec<Style> {
        highlight::cluster_styles(
            self.highlighting.as_ref(),
            line_start,
            layout,
            palette,
            background,
        )
    }

    /// Brings the cached highlighter and span index up to date for the
    /// frame's window of document lines.
    pub(super) fn refresh(&mut self, editor: &Editor, viewport_lines: core::ops::Range<usize>) {
        let document = &editor.state().document;
        let same_source = self
            .highlight_source
            .as_ref()
            .is_some_and(|source| source.matches(document));
        if !same_source {
            self.highlighting = None;
        }
        self.highlighting =
            Highlighting::refreshed(self.highlighting.take(), editor, viewport_lines);
        if self.highlighting.is_none() {
            self.highlight_source = None;
        } else if !same_source {
            self.highlight_source = Some(HighlightSource::new(document));
        }
    }
}

/// Fills one row's text area with blanks in `style`.
pub(super) fn fill_text_area(buffer: &mut CellBuffer, row: usize, area: TextArea, style: Style) {
    for offset in 0..area.width {
        buffer.set_str(area.origin + offset, row, " ", style);
    }
}

/// Paints one selection's contribution to one line.
///
/// A selection that continues onto the next line is drawn one cell past the
/// end of this one: that cell is the line break, and it is as selected as the
/// characters before it.
fn paint_selection(
    buffer: &mut CellBuffer,
    row: usize,
    document_line: usize,
    layout: &LineLayout,
    selection: &Selection,
    palette: &Palette,
    area: TextArea,
) {
    if selection.is_collapsed() {
        return;
    }
    let start = selection.start();
    let end = selection.end();
    if document_line < start.line || document_line > end.line {
        return;
    }

    let from = if document_line == start.line {
        layout.column_to_cell(start.column)
    } else {
        0
    };
    let to = if document_line == end.line {
        layout.column_to_cell(end.column)
    } else {
        layout.width() + 1
    };

    text::restyle_columns(buffer, row, from..to, area, |style| palette.selected(style));
}

/// The placeholder text for a folded line.
///
/// The wording is the kernel's: the GPU face draws the same string, and a face
/// with its own would drift the first time the kernel's changed.
pub(super) fn fold_placeholder(editor: &Editor, document_line: usize) -> String {
    use iridium_editor::render::FoldPlaceholderRenderer;

    let state = editor.state();
    FoldPlaceholderRenderer::new()
        .compute_placeholders(
            &state.fold_state,
            |_| 0,
            1.0,
            1.0,
            0.0,
            document_line,
            1,
            state.theme.editor.gutter,
            state.theme.editor.line_number,
        )
        .into_iter()
        .find(|placeholder| placeholder.line == document_line)
        .map_or_else(String::new, |placeholder| placeholder.text)
}
