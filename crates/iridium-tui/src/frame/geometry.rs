//! Where everything on the screen goes, computed without drawing anything.
//!
//! Split from the painting so that a caller can ask what a frame *would* look
//! like — the driver needs the caret's cell before it writes one, and mouse hit
//! testing will need the text area — without a cell buffer in hand. Both halves
//! read the same kernel state, so they cannot disagree.
//!
//! # The rows, top to bottom
//!
//! ```text
//! ┌──────────────────────────────┐
//! │ document text                │  text_rows
//! │ search panel (when open)     │  SearchOverlay::rows
//! │ statusline                   │  one row
//! └──────────────────────────────┘
//! ```
//!
//! The panel's rows are taken *from* the document's rather than laid over them,
//! and the kernel's viewport is sized to what is left. See
//! [`search`](super::search) for why floating it would make the kernel scroll
//! matches underneath it.

use iridium_editor::render::Viewport;
use iridium_editor::{Editor, Position};

use super::units::{cell_units, whole_cells};
use super::{LineLayout, SearchOverlay, Status, TextArea, gutter};

/// A position in the cell grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPosition {
    /// The column, counting from the left edge of the screen.
    pub column: usize,
    /// The row, counting from the top of the screen.
    pub row: usize,
}

/// The state the host owns and the frame draws, beside the document itself.
///
/// One parameter rather than a growing list of them: every function that lays
/// the screen out needs all of it, and a caller that passed last frame's panel
/// with this frame's statusline would produce a geometry that matches neither.
#[derive(Debug, Clone, Copy, Default)]
pub struct Chrome<'a> {
    /// What the host knows about the document that the kernel does not.
    pub status: Status<'a>,
    /// The search panel, when the host has one open.
    pub search: Option<&'a SearchOverlay>,
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
    /// The first row the search panel occupies, and how many it takes.
    ///
    /// `None` when no panel is open, or when the screen is too small to show
    /// one row of it.
    pub search_rows: Option<(usize, usize)>,
    /// The row the statusline occupies, if the screen has one.
    pub status_row: Option<usize>,
    /// The kernel's viewport, driven in cell units.
    pub viewport: Viewport,
    /// Where the primary caret landed, if it is on screen.
    pub primary_caret: Option<CellPosition>,
    /// Where the caret in the search panel's focused field landed.
    pub search_caret: Option<CellPosition>,
}

impl FrameLayout {
    /// The cell the terminal's own cursor belongs in.
    ///
    /// The panel's field wins while one is open: that is where typing goes, and
    /// a cursor left in the document would say otherwise. `None` means no
    /// cursor should be shown — the caret is scrolled off, or there is no room
    /// for the field it belongs to.
    #[must_use]
    pub const fn caret(&self) -> Option<CellPosition> {
        match self.search_caret {
            Some(position) => Some(position),
            None => self.primary_caret,
        }
    }
}

/// The geometry a screen of this size would be drawn with.
pub fn layout(editor: &Editor, columns: usize, rows: usize, chrome: Chrome<'_>) -> FrameLayout {
    let state = editor.state();
    let total_lines = state.document.line_count();
    let gutter_width = gutter::width(total_lines, state.config.show_line_numbers).min(columns);
    let text_width = columns - gutter_width;
    let status_row = rows.checked_sub(1);
    let below_status = rows.saturating_sub(1);

    let panel_rows = chrome
        .search
        .map_or(0, |_| SearchOverlay::rows(below_status));
    let text_rows = below_status - panel_rows;
    let search_rows = if panel_rows == 0 {
        None
    } else {
        Some((text_rows, panel_rows))
    };
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
        search_rows,
        status_row,
        viewport,
        primary_caret,
        // Filled in by the render pass, which is what knows whether the
        // panel's focused field had room to be drawn.
        search_caret: None,
    }
}

/// Writes the cell-unit geometry of a screen of this size into the kernel's
/// viewport.
///
/// The kernel's scroll verbs — `ensure_cursor_visible`,
/// `scroll_to_position_with_folds` — work from the viewport's own size, so a
/// host that never calls this scrolls against a viewport still measured in
/// pixels and will disagree with what is on screen. Call it when the terminal
/// is sized, whenever it is resized, and whenever the search panel opens or
/// closes: the panel changes how many rows the document has.
///
/// Scroll position is left alone: where the document is scrolled to is kernel
/// state, and a resize does not move it.
pub fn sync_viewport(editor: &mut Editor, columns: usize, rows: usize, chrome: Chrome<'_>) {
    let geometry = layout(editor, columns, rows, chrome);
    let state = editor.state_mut();
    state.viewport.width = geometry.viewport.width;
    state.viewport.height = geometry.viewport.height;
    state.viewport.line_height = 1.0;
    state.viewport.visible_lines = geometry.viewport.visible_lines;
    state.viewport.scroll_offset_y = 0.0;
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
