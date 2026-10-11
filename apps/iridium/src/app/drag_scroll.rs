//! Timed scrolling while a source selection is held at a viewport edge.
use super::App;
use iridium_editor::workspace::NodeId;
use iridium_tui::frame::{CellPosition, FrameLayout};
use std::time::{Duration, Instant};

const INTERVAL: Duration = Duration::from_millis(50);

pub(super) struct DragScroll {
    cell: CellPosition,
    tab: NodeId,
    revision: u64,
    due: Instant,
}

impl App {
    pub(super) const fn stop_mouse_drag(&mut self) {
        self.sidebar_drag = None;
        self.mouse_anchor = None;
        self.mouse_scroll = None;
    }

    pub(super) fn track_drag_edge(&mut self, cell: CellPosition, layout: &FrameLayout) {
        if self.mouse_anchor.is_none() || !at_edge(cell, layout) {
            self.mouse_scroll = None;
            return;
        }
        if let Some(scroll) = &mut self.mouse_scroll {
            scroll.cell = cell;
            return;
        }
        if let (Some(tab), Some(editor)) = (self.workspace.active(), self.editor()) {
            self.mouse_scroll = Some(DragScroll {
                cell,
                tab,
                revision: editor.state().document.revision(),
                due: Instant::now() + INTERVAL,
            });
        }
    }

    pub(super) fn poll_mouse_scroll(&mut self, now: Instant) {
        let Some(mut scroll) = self.mouse_scroll.take() else {
            return;
        };
        if self.mouse_anchor.is_none()
            || self.workspace.active() != Some(scroll.tab)
            || self.palette_open
            || self.prompt.is_some()
            || self.explorer.is_some()
            || self.history_open
            || self.search_open
            || self
                .editor()
                .is_none_or(|editor| editor.state().document.revision() != scroll.revision)
        {
            self.stop_mouse_drag();
            return;
        }
        if now < scroll.due {
            self.mouse_scroll = Some(scroll);
            return;
        }
        let Some(layout) = self.mouse_layout() else {
            return;
        };
        if layout.text_rows == 0 || layout.text.width == 0 {
            return;
        }
        let Some(editor) = self.workspace.active_editor_mut() else {
            return;
        };
        let before = editor.state().viewport.clone();
        let result = (|| {
            if scroll.cell.row == 0 {
                layout.scroll_rows(editor, false, 1)?;
            } else if scroll.cell.row >= layout.text_rows.saturating_sub(1) {
                layout.scroll_rows(editor, true, 1)?;
            }
            if scroll.cell.column <= layout.text.origin {
                layout.scroll_columns(editor, false, 2)?;
            } else if scroll.cell.column
                >= layout
                    .text
                    .origin
                    .saturating_add(layout.text.width)
                    .saturating_sub(1)
            {
                layout.scroll_columns(editor, true, 2)?;
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            editor.set_viewport(before);
            self.message = Some(super::Message::error(error));
            return;
        }
        let viewport = &editor.state().viewport;
        let changed = viewport.first_line != before.first_line
            || viewport.scroll_offset_x.to_bits() != before.scroll_offset_x.to_bits()
            || viewport.scroll_offset_y.to_bits() != before.scroll_offset_y.to_bits();
        let row = scroll.cell.row.min(layout.text_rows.saturating_sub(1));
        let Some(layout) = self.mouse_layout() else {
            return;
        };
        let cell = CellPosition {
            row,
            column: scroll
                .cell
                .column
                .clamp(layout.sidebar_columns, self.columns.saturating_sub(1)),
        };
        let position = self
            .editor()
            .and_then(|editor| layout.document_position(editor, cell));
        self.select_mouse(position);
        if changed {
            scroll.due = now + INTERVAL;
            self.mouse_scroll = Some(scroll);
        }
    }
}

const fn at_edge(cell: CellPosition, layout: &FrameLayout) -> bool {
    layout.text_rows > 0
        && layout.text.width > 0
        && (cell.row == 0
            || cell.row >= layout.text_rows.saturating_sub(1)
            || cell.column <= layout.text.origin
            || cell.column
                >= layout
                    .text
                    .origin
                    .saturating_add(layout.text.width)
                    .saturating_sub(1))
}

impl DragScroll {
    pub(super) fn remaining(&self) -> Duration {
        self.due.saturating_duration_since(Instant::now())
    }
}
