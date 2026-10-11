//! A cell-width tab strip that keeps the active tab visible on narrow screens.
use super::App;
use iridium_tui::cell::{CellBuffer, Style};
use iridium_tui::frame::{LineLayout, Palette};

pub(super) struct TabStrip {
    labels: Vec<String>,
    active: usize,
}

impl App {
    pub(super) fn tab_strip(&self) -> TabStrip {
        let tabs = self.workspace.tabs();
        let active = tabs
            .iter()
            .position(|id| Some(*id) == self.workspace.active())
            .unwrap_or(0);
        let labels: Vec<String> = tabs
            .iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let node = self.workspace.node(*id)?;
                let dirty = node.document().is_some_and(|id| self.document_is_dirty(id));
                let marker = if dirty { "*" } else { "" };
                Some(if index == active {
                    format!("[{}:{}{}]", index + 1, marker, node.label())
                } else {
                    format!(" {}:{}{} ", index + 1, marker, node.label())
                })
            })
            .collect();
        TabStrip { labels, active }
    }
}

impl TabStrip {
    fn visible(&self, width: usize) -> (usize, Vec<usize>) {
        let Self { labels, active } = self;
        let active = *active;
        let widths: Vec<usize> = labels
            .iter()
            .map(|label| LineLayout::new(label, 1).width())
            .collect();
        // Backfill as many complete tabs before the active one as fit. With
        // one very long active name the index and dirty marker still lead it.
        let mut start = active;
        let mut used = widths.get(active).copied().unwrap_or(0).min(width);
        while let Some(previous) = start.checked_sub(1) {
            let Some(previous_width) = widths.get(previous) else {
                break;
            };
            if used.saturating_add(*previous_width).saturating_add(2) > width {
                break;
            }
            used += previous_width;
            start = previous;
        }
        (start, widths)
    }

    pub(super) fn hit(&self, column: usize, width: usize) -> Option<usize> {
        if column >= width {
            return None;
        }
        let (start, widths) = self.visible(width);
        let mut left = if start > 0 && width > 2 { 2 } else { 0 };
        if column < left {
            return start.checked_sub(1);
        }
        for (index, size) in widths.iter().enumerate().skip(start) {
            let right = left.saturating_add(*size).min(width);
            if column < right {
                return Some(index);
            }
            left = right;
            if left == width {
                break;
            }
        }
        None
    }

    pub(super) fn paint(&self, buffer: &mut CellBuffer, row: usize, palette: &Palette) {
        let Self { labels, active } = self;
        let active = *active;
        let width = buffer.width();
        let (start, widths) = self.visible(width);
        let quiet = palette.overlay_quiet();
        buffer.set_str(0, row, &" ".repeat(width), quiet);
        let mut column = if start > 0 && width > 2 {
            buffer.set_str(0, row, "< ", quiet)
        } else {
            0
        };
        for (index, label) in labels.iter().enumerate().skip(start) {
            let style = if index == active {
                palette.overlay_toggle(true)
            } else {
                quiet
            };
            let remaining = width.saturating_sub(column);
            if remaining == 0 {
                break;
            }
            let Some(label_width) = widths.get(index) else {
                break;
            };
            if *label_width > remaining {
                paint_clipped(buffer, row, column, label, remaining, style);
                break;
            }
            column = buffer.set_str(column, row, label, style);
        }
    }
}

fn paint_clipped(
    buffer: &mut CellBuffer,
    row: usize,
    column: usize,
    label: &str,
    width: usize,
    style: Style,
) {
    let layout = LineLayout::new(label, 1);
    let end = width.saturating_sub(1);
    for cluster in layout.clusters() {
        if cluster.column().saturating_add(cluster.width()) > end {
            break;
        }
        if let Some(grapheme) = cluster.grapheme() {
            // Layout proved these whole clusters fit before the ellipsis.
            match buffer.set_grapheme(column + cluster.column(), row, grapheme, style) {
                iridium_tui::cell::WriteOutcome::Written(_) => {},
                iridium_tui::cell::WriteOutcome::Truncated
                | iridium_tui::cell::WriteOutcome::OutOfBounds => return,
            }
        }
    }
    if width > 0 {
        buffer.set_str(column + end, row, "…", style);
    }
}
