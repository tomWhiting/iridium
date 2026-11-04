//! Pointer coordinates use the same placement and body as the painter.
use super::{Anchor, CentredBox, ExplorerAction, FileExplorerPanel, PanelFit, SidebarBox};
use crate::input::{MouseButton, MouseEvent, MouseEventKind, ScrollDirection};
use iridium_editor::{KeyCode, KeyEvent, Theme};

impl FileExplorerPanel {
    /// Routes clicks and wheel movement inside the visible browser.
    /// `None` means the pointer is outside its placement.
    pub fn handle_mouse(
        &mut self,
        event: MouseEvent,
        columns: usize,
        _rows: usize,
        band_rows: usize,
        theme: &Theme,
    ) -> Option<ExplorerAction> {
        let band = (self.anchor == Anchor::Sidebar)
            .then(|| SidebarBox::fitted(columns, band_rows, self.sidebar_width))
            .flatten();
        let (left, width, top, fit) = if let Some(band) = band {
            (
                0,
                band.width,
                0,
                PanelFit::sidebar(band.content_width(), band.rows),
            )
        } else {
            // The same box the painter draws: see `floating` in the parent.
            let centred = CentredBox::fitted(columns, band_rows)?;
            (
                centred.frame.left,
                centred.frame.width,
                centred.top + 1,
                PanelFit::sidebar(centred.frame.content_width(), centred.interior_rows),
            )
        };
        let row = usize::from(event.row);
        let column = usize::from(event.column);
        let body = self.explorer.body(theme, fit);
        let height = band.map_or(body.rows.len(), |band| band.rows);
        if column < left || column >= left + width || row < top || row >= top + height {
            return None;
        }
        Some(match event.kind {
            MouseEventKind::Down(MouseButton::Left) if row - top < body.rows.len() => {
                ExplorerAction::from_outcome(self.explorer.click_row(row - top))
            },
            MouseEventKind::Scroll(ScrollDirection::Up | ScrollDirection::Down) => {
                let key = if event.kind == MouseEventKind::Scroll(ScrollDirection::Up) {
                    KeyCode::Up
                } else {
                    KeyCode::Down
                };
                self.handle_key(&KeyEvent::simple(key))
            },
            _ => ExplorerAction::Handled,
        })
    }
}
