//! Explicit source/replacement review with bounded visible snippets.
use super::Review;
use iridium_tui::{cell::CellBuffer, driver::CursorState, frame::Palette};
impl Review {
    pub fn paint(&self, buffer: &mut CellBuffer, palette: &Palette) -> CursorState {
        let (width, height) = (buffer.width(), buffer.height());
        if width == 0 || height == 0 {
            return CursorState::Hidden;
        }
        let blank = " ".repeat(width);
        for row in 0..height {
            buffer.set_str(0, row, &blank, palette.overlay());
        }
        if let Some(detail) = &self.detail {
            detail.paint(self, buffer, palette);
            return CursorState::Hidden;
        }
        let count = self.included.iter().filter(|value| **value).count();
        buffer.set_str(
            0,
            0,
            &format!(
                "Review replacement · {count}/{} matches · {} search issues · {}",
                self.hits.len(),
                self.issues,
                self.feedback
            ),
            palette.status(),
        );
        let caret = self.entry.paint(
            buffer,
            1,
            "Replace: ",
            self.focus.is_replacement() && !self.completed && self.operation.is_none(),
            palette,
        );
        let rows = height.saturating_sub(4);
        if self.completed || self.operation.is_some() {
            let start = if self.completed {
                self.selected.saturating_sub(rows.saturating_sub(1))
            } else {
                self.outcomes.len().saturating_sub(rows)
            };
            for (row, outcome) in self.outcomes.iter().skip(start).take(rows).enumerate() {
                buffer.set_str(0, row + 2, &visible(outcome, width), palette.text());
            }
        } else {
            let start = self.selected.saturating_sub(rows.saturating_sub(1));
            for (index, hit) in self.hits.iter().enumerate().skip(start).take(rows) {
                let old = hit
                    .line_text
                    .get(hit.range.clone())
                    .unwrap_or("<invalid range>");
                let new = self
                    .preview
                    .as_ref()
                    .and_then(|preview| preview.get(index))
                    .map_or("…", iridium_search::Replacement::text);
                let path = hit.path.strip_prefix(&self.root).unwrap_or(&hit.path);
                let line = format!(
                    "{} [{}] {}:{}  {} → {}",
                    if index == self.selected { "›" } else { " " },
                    if self.included[index] { "x" } else { " " },
                    path.display(),
                    hit.line + 1,
                    visible(old, 40),
                    visible(new, 60)
                );
                buffer.set_str(
                    0,
                    index - start + 2,
                    &visible(&line, width),
                    if index == self.selected {
                        palette.status()
                    } else {
                        palette.text()
                    },
                );
            }
        }
        if height >= 2 {
            buffer.set_str(
                0,
                height - 2,
                "Tab replacement/results · ↑↓ choose · Space include/exclude · F7 file · F4 inspect",
                palette.overlay_quiet(),
            );
            buffer.set_str(0,height-1,"F6 apply to buffers · F9 apply + save whole buffers · Esc cancel/close · undo once per changed document",palette.status());
        }
        caret
            .filter(|_| height > 3)
            .map_or(CursorState::Hidden, |column| CursorState::At {
                row: 1,
                column,
            })
    }
}
fn visible(text: &str, limit: usize) -> String {
    let mut visible = String::new();
    for ch in text.chars().take(limit) {
        match ch {
            '\n' => visible.push_str("\\n"),
            '\r' => visible.push_str("\\r"),
            '\t' => visible.push_str("\\t"),
            ch if ch.is_control() => visible.push('�'),
            ch => visible.push(ch),
        }
    }
    visible
}
