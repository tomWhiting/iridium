//! Cell-frame allocation and bounded caret hints from the borrowed kernel map.

use iridium_editor::Editor;
use iridium_editor::cell_layout::{CellPlacement, CellRowMap};

use super::{CellCaret, CellFrameLayout, CellFrameOptions, CellPosition, TextArea, geometry};

pub(super) fn layout(editor: &Editor, options: CellFrameOptions<'_>) -> CellFrameLayout {
    let allocation = geometry::allocate(editor, options.columns, options.rows, options.chrome);
    CellFrameLayout {
        columns: options.columns,
        rows: options.rows,
        sidebar_columns: allocation.sidebar_columns,
        gutter_width: allocation.gutter_width,
        text: TextArea {
            origin: allocation.sidebar_columns + allocation.gutter_width,
            width: allocation.text_width,
            scroll: 0,
        },
        text_rows: allocation.text_rows,
        first_row: options.first_row,
        total_rows: 0,
        search_rows: allocation.search_rows,
        status_row: allocation.status_row,
        primary_caret: None,
        search_caret: None,
    }
}

pub(super) fn caret(
    layout: &CellFrameLayout,
    map: &CellRowMap<'_>,
    placement: CellPlacement,
) -> Option<CellCaret> {
    if layout.text.width == 0 {
        return None;
    }
    let row = placement.row.0.checked_sub(layout.first_row.0)?;
    if row >= layout.text_rows {
        return None;
    }
    let trailing_edge = placement.column.0 >= layout.text.width;
    let column = if trailing_edge {
        // A full-width glyph's continuation cell is never a physical caret.
        // Overflow glyphs paint as blanks, so their final available blank is
        // a truthful physical location for the explicit trailing-edge hint.
        map.rows()
            .get(placement.row.0)?
            .clusters()
            .iter()
            .rev()
            .find(|cluster| cluster.width() > 0)
            .filter(|cluster| {
                cluster.kind() == iridium_editor::cell_layout::ClusterKind::Glyph
                    && cluster
                        .column()
                        .0
                        .checked_add(cluster.width())
                        .is_some_and(|end| end <= layout.text.width)
            })
            .map_or(layout.text.width - 1, |cluster| cluster.column().0)
    } else {
        placement.column.0
    };
    Some(CellCaret {
        position: CellPosition {
            column: layout.text.origin + column,
            row,
        },
        placement,
        trailing_edge,
    })
}
