//! Document mouse selection uses the renderer's cell geometry.
use super::{App, Flow};
use iridium_editor::Position;
use iridium_tui::frame::{CellPosition, Chrome, Frame};
use iridium_tui::input::{
    MouseButton, MouseEvent, MouseEventKind, MouseModifiers, ScrollDirection,
};

impl App {
    pub(super) fn handle_mouse(&mut self, event: MouseEvent) -> Flow {
        if self.similar.is_some() {
            self.stop_mouse_drag();
            return Flow::Running;
        }
        if self.project_search_open() {
            self.stop_mouse_drag();
            if let Some(panel) = self.project_search.as_mut() {
                panel.mouse(event, self.columns, self.rows);
            }
            return Flow::Running;
        }
        if self.palette_open || self.prompt.is_some() || self.history_open || self.search_open {
            self.stop_mouse_drag();
            return Flow::Running;
        }
        if self.resize_sidebar_mouse(event) {
            return Flow::Running;
        }
        if self.tab_mouse(event) {
            return Flow::Running;
        }
        if self.search_open {
            return Flow::Running;
        }
        if let Some(flow) = self.explorer_mouse(event) {
            return flow;
        }
        let Some(layout) = self.mouse_layout() else {
            return Flow::Running;
        };
        let Some(editor) = self.editor() else {
            return Flow::Running;
        };
        let mut cell = CellPosition {
            column: usize::from(event.column),
            row: usize::from(event.row),
        };
        let pointer = cell;
        if matches!(
            event.kind,
            MouseEventKind::Drag(MouseButton::Left) | MouseEventKind::Up(MouseButton::Left)
        ) && self.mouse_anchor.is_some()
        {
            cell.row = cell.row.min(layout.text_rows.saturating_sub(1));
            cell.column = cell.column.max(layout.sidebar_columns);
        }
        let position = layout.document_position(editor, cell);
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let anchor = position.map(|head| {
                    if event.modifiers.contains(MouseModifiers::SHIFT) {
                        editor.state().cursor.primary.anchor
                    } else {
                        head
                    }
                });
                self.mouse_scroll = None;
                self.mouse_anchor = anchor;
                self.select_mouse(position);
            },
            MouseEventKind::Drag(MouseButton::Left) => {
                self.select_mouse(position);
                self.track_drag_edge(pointer, &layout);
            },
            MouseEventKind::Up(MouseButton::Left) => {
                self.select_mouse(position);
                self.stop_mouse_drag();
            },
            MouseEventKind::Scroll(direction) if position.is_some() => {
                if let Some(editor) = self.workspace.active_editor_mut() {
                    let direction = if event.modifiers.contains(MouseModifiers::SHIFT) {
                        match direction {
                            ScrollDirection::Up => ScrollDirection::Left,
                            ScrollDirection::Down => ScrollDirection::Right,
                            other => other,
                        }
                    } else {
                        direction
                    };
                    let result = match direction {
                        ScrollDirection::Up => layout.scroll_rows(editor, false, 3),
                        ScrollDirection::Down => layout.scroll_rows(editor, true, 3),
                        ScrollDirection::Left => layout.scroll_columns(editor, false, 3),
                        ScrollDirection::Right => layout.scroll_columns(editor, true, 3),
                    };
                    if let Err(error) = result {
                        self.message = Some(super::Message::error(error));
                    }
                }
            },
            MouseEventKind::Moved => self.stop_mouse_drag(),
            _ => {},
        }
        Flow::Running
    }

    pub(super) fn select_mouse(&mut self, position: Option<Position>) {
        if let (Some(anchor), Some(head), Some(editor)) = (
            self.mouse_anchor,
            position,
            self.workspace.active_editor_mut(),
        ) {
            editor.set_selection(anchor, head);
            if anchor != head {
                self.select_mode();
            }
        }
    }
}

impl App {
    pub(super) fn mouse_layout(&self) -> Option<iridium_tui::frame::FrameLayout> {
        self.editor().map(|editor| {
            Frame::hit_layout(
                editor,
                self.columns,
                self.rows,
                Chrome {
                    search: if self.search_open {
                        self.workspace
                            .active_payload()
                            .map(|session| &session.search)
                    } else {
                        None
                    },
                    sidebar_columns: self.sidebar_columns(),
                    tab_bar: true,
                    ..Chrome::default()
                },
            )
        })
    }
}
