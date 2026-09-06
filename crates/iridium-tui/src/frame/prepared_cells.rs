//! Prepared cell frames borrow the editor that their geometry and glyphs describe.

use iridium_editor::cell_layout::{
    CellColumn, CellHit, CellLayoutError, CellRowMap, CellWrapParameters, ScreenRow,
};
use iridium_editor::editor::CellInputOptions;
use iridium_editor::{Editor, Range};

use super::render::fold_placeholder;
use super::{
    CellCaret, CellFrameError, CellFrameLayout, CellFrameOptions, Frame, LineLayout, cell_geometry,
};

/// One frame borrows its actual editor through paint and hit testing.
/// Mutating the editor requires dropping this frame and preparing afresh.
///
/// ```compile_fail
/// use iridium_editor::Editor;
/// use iridium_editor::cell_layout::ScreenRow;
/// use iridium_tui::frame::{Frame, CellFrameOptions};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut editor = Editor::with_defaults();
/// let prepared = Frame::prepare_cells(&editor, CellFrameOptions {
///     columns: 20, rows: 4, first_row: ScreenRow(0), chrome: None,
/// })?;
/// editor.paste_cells("changed")?;
/// prepared.position_at(0, 0)?;
/// # Ok(())
/// # }
/// ```
///
/// ```compile_fail
/// use iridium_editor::Editor;
/// use iridium_editor::cell_layout::ScreenRow;
/// use iridium_tui::{cell::CellBuffer, frame::{Frame, CellFrameOptions}};
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let mut editor = Editor::with_defaults();
/// let prepared = Frame::prepare_cells(&editor, CellFrameOptions {
///     columns: 4, rows: 2, first_row: ScreenRow(0), chrome: None,
/// })?;
/// editor.set_content("replacement");
/// Frame::new().render_cells(&prepared, &mut CellBuffer::new(4, 2))?;
/// # Ok(())
/// # }
/// ```
pub struct PreparedCellFrame<'a> {
    pub(super) editor: &'a Editor,
    pub(super) options: CellFrameOptions<'a>,
    pub(super) layout: CellFrameLayout,
    pub(super) map: CellRowMap<'a>,
    pub(super) visible: Vec<PreparedRow>,
    pub(super) carets: Vec<CellCaret>,
    pub(super) matches: Vec<(Range, bool)>,
}

impl std::fmt::Debug for PreparedCellFrame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedCellFrame")
            .field("layout", &self.layout)
            .field("visible_rows", &self.visible.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(super) struct PreparedRow {
    pub(super) document_line: usize,
    #[cfg(feature = "syntax")]
    pub(super) byte_start: usize,
    pub(super) layout: LineLayout,
    pub(super) first: bool,
    pub(super) last: bool,
    pub(super) active: bool,
    pub(super) placeholder: Option<LineLayout>,
}

impl Frame {
    /// Prepares one local rectangle without mutating the editor or owning I/O.
    pub fn prepare_cells<'a>(
        editor: &'a Editor,
        options: CellFrameOptions<'a>,
    ) -> Result<PreparedCellFrame<'a>, CellFrameError> {
        let state = editor.state();
        let mut layout = cell_geometry::layout(editor, options);
        let input = CellInputOptions {
            wrap: CellWrapParameters::new(layout.text.width, state.config.tab_width),
            visible_rows: layout.text_rows,
        };
        let map = CellRowMap::prepare(&state.document, &state.fold_state, input.wrap)?;
        layout.total_rows = map.total_rows();
        let mut carets = Vec::new();
        for (index, selection) in state.cursor.all_selections().enumerate() {
            let affinity = editor.cell_cursor_affinity(index, input).ok_or(
                CellLayoutError::InvalidPosition {
                    line: selection.head.line,
                    column: selection.head.column,
                },
            )?;
            if let Some(placement) = map.place(selection.head, affinity)? {
                if let Some(caret) = cell_geometry::caret(&layout, &map, placement) {
                    if index == 0 {
                        layout.primary_caret = Some(caret);
                    }
                    carets.push(caret);
                }
            }
        }
        let visible = prepare_rows(editor, &map, &layout)?;
        let matches = match (visible.first(), visible.last()) {
            (Some(first), Some(last)) => editor
                .search_state()
                .all_matches()
                .iter()
                .enumerate()
                .filter(|(_, range)| {
                    range.start.line <= last.document_line && range.end.line >= first.document_line
                })
                .map(|(index, range)| (*range, editor.current_match_index() == Some(index)))
                .collect(),
            _ => Vec::new(),
        };
        Ok(PreparedCellFrame {
            editor,
            options,
            layout,
            map,
            visible,
            carets,
            matches,
        })
    }
}

impl PreparedCellFrame<'_> {
    /// Allocation and logical/physical primary caret computed from this borrow.
    #[must_use]
    pub const fn layout(&self) -> &CellFrameLayout {
        &self.layout
    }

    /// The exact borrowed kernel geometry used for this frame.
    #[must_use]
    pub const fn text_view(&self) -> &CellRowMap<'_> {
        &self.map
    }

    /// Geometry options for a fresh cell-key/pointer call after this borrow ends.
    #[must_use]
    pub const fn input_options(&self) -> CellInputOptions {
        CellInputOptions {
            wrap: self.map.parameters(),
            visible_rows: self.layout.text_rows,
        }
    }

    /// Resolves a local cell. Chrome/outside-document cells return None.
    /// The returned hit is read-only; selection changes must use the fresh
    /// `Editor::set_cell_pointer` entry point after dropping this frame.
    pub fn position_at(
        &self,
        column: usize,
        row: usize,
    ) -> Result<Option<CellHit>, CellFrameError> {
        let Some(column) = column.checked_sub(self.layout.text.origin) else {
            return Ok(None);
        };
        if column >= self.layout.text.width || row >= self.layout.text_rows {
            return Ok(None);
        }
        let Some(row) = self.layout.first_row.0.checked_add(row) else {
            return Ok(None);
        };
        if row >= self.map.total_rows() {
            return Ok(None);
        }
        Ok(Some(
            self.map.position_at(ScreenRow(row), CellColumn(column))?,
        ))
    }
}

fn prepare_rows(
    editor: &Editor,
    map: &CellRowMap<'_>,
    layout: &CellFrameLayout,
) -> Result<Vec<PreparedRow>, CellFrameError> {
    let document = &editor.state().document;
    let mut source = None;
    let mut result = Vec::new();
    for row in map
        .rows()
        .iter()
        .skip(layout.first_row.0)
        .take(layout.text_rows)
    {
        let document_line = row.document_line();
        // A long logical line is copied once for this visible slice, rather
        // than once for each wrapped row. The prepared glyphs retain no text
        // identity key and painting never measures source text again.
        if source
            .as_ref()
            .is_none_or(|(line, _, _)| *line != document_line)
        {
            let text = document
                .line(document_line)
                .ok_or(CellLayoutError::InvalidPosition {
                    line: document_line,
                    column: 0,
                })?;
            let scalars = text.chars().count();
            source = Some((document_line, text, scalars));
        }
        let (_, text, scalars) = source.as_ref().ok_or(CellLayoutError::InvalidPosition {
            line: document_line,
            column: 0,
        })?;
        let last = row.scalar_range().end == *scalars;
        let placeholder = if last && editor.is_folded(document_line) {
            Some(LineLayout::new(
                &fold_placeholder(editor, document_line),
                editor.state().config.tab_width,
            ))
        } else {
            None
        };
        result.push(PreparedRow {
            document_line,
            #[cfg(feature = "syntax")]
            byte_start: document.line_to_byte_offset(document_line).ok_or(
                CellLayoutError::InvalidPosition {
                    line: document_line,
                    column: 0,
                },
            )?,
            layout: LineLayout::from_cell_row(text, row)?,
            first: row.scalar_range().start == 0,
            last,
            placeholder,
            active: editor
                .state()
                .cursor
                .all_selections()
                .any(|selection| selection.head.line == document_line),
        });
    }
    Ok(result)
}
