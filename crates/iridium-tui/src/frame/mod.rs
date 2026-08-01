//! The frame: kernel state in, a filled back buffer out.
//!
//! Step 4 of `docs/TERMINAL-FACE-PLAN.md`. This is a pure transformation —
//! there is no terminal here, no escape sequence and no I/O — which is what
//! lets every rule below be tested by building an [`Editor`], rendering into a
//! [`CellBuffer`] and asserting on cells.
//!
//! # The kernel owns layout
//!
//! Which document lines are visible, and which row each one lands on, is the
//! kernel's fold-aware [`Viewport`]. The terminal drives that same viewport in
//! **cell units** — `line_height = 1.0`, `width = columns` — so one cell is one
//! unit and `screen_y_for_line` returns a row directly. There is no second
//! layout model and no second fold model; a competing one is exactly why
//! termwiz was rejected, and three separate bugs this year were a face
//! reimplementing a kernel verb.
//!
//! What is *not* the kernel's is the mapping from a document column to a screen
//! cell. The kernel expresses a line's geometry in pixels against a fixed
//! character width, and a terminal cannot: see [`line`] for the rules and
//! [`text`] for what a cut through a glyph does.
//!
//! # What is drawn
//!
//! Top to bottom: the gutter (line numbers and fold indicators), the document
//! text with syntax highlighting, selection backgrounds and a caret for
//! **every** cursor, and a statusline on the last row. The multi-cursor set is
//! rendered whole — a face that draws only the primary cursor is a bug this
//! codebase has already had twice.

mod gutter;
mod line;
mod palette;
mod status;
mod text;
mod units;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use iridium_editor::render::Viewport;
use iridium_editor::syntax::{Highlighter, Language};
use iridium_editor::span_index::SpanIndex;
use iridium_editor::{Editor, Position, Selection};

use self::units::{cell_units, whole_cells};
use crate::cell::{CellBuffer, Color, Style};

pub use self::line::{LineLayout, PlacedCluster};
pub use self::palette::Palette;
pub use self::status::Status;
pub use self::text::TextArea;

/// A position in the cell grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPosition {
    /// The column, counting from the left edge of the screen.
    pub column: usize,
    /// The row, counting from the top of the screen.
    pub row: usize,
}

/// Where everything went: the geometry one frame was drawn with.
///
/// The driver needs this to place the terminal's own cursor, and mouse hit
/// testing will need it to turn a click into a document position — which is
/// the one place the fixed character-width assumption still lives elsewhere in
/// the tree.
#[derive(Debug, Clone)]
pub struct FrameLayout {
    /// The number of columns the gutter occupies. Zero when line numbers are
    /// switched off.
    pub gutter_width: usize,
    /// The rectangle document text is painted into.
    pub text: TextArea,
    /// The number of rows available to document text.
    pub text_rows: usize,
    /// The row the statusline occupies, if the screen has one.
    pub status_row: Option<usize>,
    /// The kernel's viewport, driven in cell units.
    pub viewport: Viewport,
    /// Where the primary caret landed, if it is on screen.
    pub primary_caret: Option<CellPosition>,
}

/// The cached parse-derived state a frame is drawn from.
struct Highlighting {
    /// The language the spans were produced for.
    language: Language,
    /// The rules, borrowed from the process-wide query cache.
    highlighter: Highlighter,
    /// The document's spans, indexed for viewport queries.
    spans: SpanIndex,
    /// The tree and document state the spans were produced from.
    generation: Generation,
}

/// How current a set of highlight spans is.
///
/// The parse count moves whenever the kernel reparses, and the revision moves
/// whenever the document does. Comparing both means spans are rebuilt when
/// either changes, so a mutation that reached the document without reaching
/// the tree cannot leave last parse's colours on screen indefinitely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Generation {
    /// Full plus incremental parses the kernel has run.
    parses: u64,
    /// The document revision.
    revision: u64,
}

/// Renders editor state into a cell buffer.
///
/// The renderer is stateful only as a cache: it holds the syntax highlighter
/// and the span index so that a frame costs a viewport query rather than a
/// whole-document highlight. Rendering itself takes the editor by shared
/// reference and mutates nothing in it.
#[derive(Default)]
pub struct Frame {
    /// The highlighter and spans, once a language has been set.
    highlighting: Option<Highlighting>,
}

impl core::fmt::Debug for Frame {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Frame")
            .field(
                "language",
                &self.highlighting.as_ref().map(|state| state.language),
            )
            .finish_non_exhaustive()
    }
}

impl Frame {
    /// A renderer with nothing cached yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The geometry a screen of this size would be drawn with.
    ///
    /// Computed from editor state alone, so it agrees with what
    /// [`Frame::render`] will do without drawing anything.
    pub fn layout(editor: &Editor, columns: usize, rows: usize) -> FrameLayout {
        let state = editor.state();
        let total_lines = state.document.line_count();
        let gutter_width =
            gutter::width(total_lines, state.config.show_line_numbers).min(columns);
        let text_width = columns - gutter_width;
        let status_row = rows.checked_sub(1);
        let text_rows = rows.saturating_sub(1);
        let scroll = whole_cells(state.viewport.scroll_offset_x);

        let viewport = Viewport {
            first_line: state.viewport.first_line,
            scroll_offset_y: 0.0,
            scroll_offset_x: state.viewport.scroll_offset_x,
            width: cell_units(text_width),
            height: cell_units(text_rows),
            visible_lines: text_rows,
            line_height: 1.0,
        };

        let text = TextArea {
            origin: gutter_width,
            width: text_width,
            scroll,
        };

        let primary_caret = caret_cell(editor, &viewport, text, state.cursor.primary.head);

        FrameLayout {
            gutter_width,
            text,
            text_rows,
            status_row,
            viewport,
            primary_caret,
        }
    }

    /// Writes the cell-unit geometry of a screen of this size into the kernel's
    /// viewport.
    ///
    /// The kernel's scroll verbs — `ensure_cursor_visible`,
    /// `scroll_to_position_with_folds` — work from the viewport's own size, so
    /// a host that never calls this scrolls against a viewport still measured
    /// in pixels and will disagree with what is on screen. Call it when the
    /// terminal is sized and whenever it is resized.
    ///
    /// Scroll position is left alone: where the document is scrolled to is
    /// kernel state, and a resize does not move it.
    pub fn sync_viewport(editor: &mut Editor, columns: usize, rows: usize) {
        let geometry = Self::layout(editor, columns, rows);
        let state = editor.state_mut();
        state.viewport.width = geometry.viewport.width;
        state.viewport.height = geometry.viewport.height;
        state.viewport.line_height = 1.0;
        state.viewport.visible_lines = geometry.viewport.visible_lines;
        state.viewport.scroll_offset_y = 0.0;
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
        status: Status<'_>,
        buffer: &mut CellBuffer,
    ) -> FrameLayout {
        let geometry = Self::layout(editor, buffer.width(), buffer.height());
        self.refresh(editor);

        let palette = Palette::from_theme(editor.get_theme());
        buffer.fill(palette.text());

        let state = editor.state();
        let cursor_lines: HashSet<usize> = state
            .cursor
            .all_selections()
            .map(|selection| selection.head.line)
            .collect();

        let total_lines = state.document.line_count();
        let visible = geometry
            .viewport
            .visible_document_lines(&state.fold_state, total_lines);

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
                document_line,
                row,
                cursor_lines.contains(&document_line),
            );
        }

        if let Some(row) = geometry.status_row {
            status::paint(buffer, row, editor, status, &palette);
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
        document_line: usize,
        row: usize,
        is_active: bool,
    ) {
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
            geometry.gutter_width,
        );

        let line_start = state.document.line_to_byte_offset(document_line).unwrap_or(0);
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
    fn cluster_styles(
        &self,
        line_start: usize,
        layout: &LineLayout,
        palette: &Palette,
        background: Color,
    ) -> Vec<Style> {
        let default = palette.text().with_background(background);
        let mut styles = vec![default; layout.clusters().len()];
        let Some(highlighting) = self.highlighting.as_ref() else {
            return styles;
        };

        let line_end = line_start + layout.byte_count();
        let mut spans: Vec<_> = highlighting.spans.query(line_start, line_end).collect();
        spans.sort_by(|left, right| {
            left.start
                .cmp(&right.start)
                .then_with(|| right.end.cmp(&left.end))
        });

        let clusters = layout.clusters();
        for span in spans {
            let style = palette.highlighted(span.highlight, background);
            let first =
                clusters.partition_point(|cluster| line_start + cluster.byte_index() < span.start);
            for (index, cluster) in clusters.iter().enumerate().skip(first) {
                if line_start + cluster.byte_index() >= span.end {
                    break;
                }
                if let Some(slot) = styles.get_mut(index) {
                    *slot = style;
                }
            }
        }
        styles
    }

    /// Brings the cached highlighter and span index up to date.
    ///
    /// The tree itself is the kernel's — the editor keeps one parse tree for
    /// the document and refreshes it after every content change — so nothing
    /// here parses. This reads that tree and turns it into spans.
    fn refresh(&mut self, editor: &Editor) {
        let state = editor.state();
        let Some(language) = state.syntax.language() else {
            self.highlighting = None;
            return;
        };
        let generation = Generation {
            parses: state.syntax.full_parses() + state.syntax.incremental_parses(),
            revision: state.document.revision(),
        };

        let reusable = match self.highlighting.take() {
            Some(existing) if existing.language == language => {
                if existing.generation == generation {
                    self.highlighting = Some(existing);
                    return;
                }
                Some(existing.highlighter)
            },
            _ => None,
        };

        // A language whose bundled highlight query does not compile leaves the
        // face with no spans rather than no frame: unhighlighted text is a
        // degraded editor, a missing frame is no editor at all.
        let Some(highlighter) = reusable.or_else(|| Highlighter::try_new(language)) else {
            return;
        };

        let spans = state.syntax.tree().map_or_else(SpanIndex::empty, |tree| {
            SpanIndex::new(highlighter.spans_in(tree, &state.document.text()))
        });

        self.highlighting = Some(Highlighting {
            language,
            highlighter,
            spans,
            generation,
        });
    }
}

/// Fills one row's text area with blanks in `style`.
fn fill_text_area(buffer: &mut CellBuffer, row: usize, area: TextArea, style: Style) {
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
fn fold_placeholder(editor: &Editor, document_line: usize) -> String {
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

/// Where a document position lands on screen, if it is visible at all.
fn caret_cell(
    editor: &Editor,
    viewport: &Viewport,
    area: TextArea,
    position: Position,
) -> Option<CellPosition> {
    let state = editor.state();
    let y = viewport.screen_y_for_line(position.line, &state.fold_state)?;
    let row = whole_cells(y);
    if row >= viewport.visible_lines {
        return None;
    }
    let text = state.document.line(position.line)?;
    let cell = LineLayout::new(&text, state.config.tab_width).column_to_cell(position.column);
    if cell < area.scroll || cell >= area.scroll + area.width {
        return None;
    }
    Some(CellPosition {
        column: area.origin + (cell - area.scroll),
        row,
    })
}
