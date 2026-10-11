//! Read-only, paged inspection of every source/replacement scalar.
use super::Review;
use iridium_editor::{KeyCode, KeyEvent};
use iridium_tui::{
    cell::CellBuffer,
    frame::{LineLayout, Palette},
};

#[derive(Default)]
pub(super) struct Detail {
    after: bool,
    offset: usize,
    previous: Vec<usize>,
}
struct Page {
    rows: Vec<String>,
    end: usize,
}
impl Detail {
    fn text<'a>(&self, review: &'a Review) -> Option<&'a str> {
        let replacement = review.preview.as_ref()?.get(review.selected)?;
        Some(if self.after {
            replacement.text()
        } else {
            replacement.original()
        })
    }
    /// All keys stay in the inspector; saving requires returning to review.
    pub fn key(&mut self, event: &KeyEvent, review: &Review, width: usize, height: usize) -> bool {
        if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.meta {
            return true;
        }
        match event.key {
            KeyCode::Escape | KeyCode::F4 | KeyCode::Enter => return false,
            KeyCode::Tab => {
                self.after = !self.after;
                self.offset = 0;
                self.previous.clear();
            },
            KeyCode::Home => {
                self.offset = 0;
                self.previous.clear();
            },
            KeyCode::Down | KeyCode::Right | KeyCode::PageDown => {
                if let Some(text) = self.text(review) {
                    let next = page(text, self.offset, width, height.saturating_sub(4)).end;
                    if next > self.offset && next < text.len() {
                        self.previous.push(self.offset);
                        self.offset = next;
                    }
                }
            },
            KeyCode::Up | KeyCode::Left | KeyCode::PageUp => {
                if let Some(previous) = self.previous.pop() {
                    self.offset = previous;
                }
            },
            _ => {},
        }
        true
    }
    pub fn paint(&self, review: &Review, buffer: &mut CellBuffer, palette: &Palette) {
        let (width, height) = (buffer.width(), buffer.height());
        let Some(text) = self.text(review) else {
            buffer.set_str(
                0,
                0,
                "Preview unavailable · Esc returns to review",
                palette.status(),
            );
            return;
        };
        let part = page(text, self.offset, width, height.saturating_sub(4));
        let side = if self.after { "After" } else { "Before" };
        buffer.set_str(
            0,
            0,
            &format!(
                "Inspect {side} · bytes {}..{} / {} · {}",
                self.offset,
                part.end,
                text.len(),
                if part.end == text.len() {
                    "end"
                } else {
                    "more ↓"
                }
            ),
            palette.status(),
        );
        if let Some(hit) = review.hits.get(review.selected) {
            buffer.set_str(
                0,
                1,
                &format!(
                    "{}:{} · escaped controls, backslashes and zero-width scalars",
                    hit.path.display(),
                    hit.line + 1
                ),
                palette.overlay_quiet(),
            );
        }
        if width < 12 || height < 5 {
            buffer.set_str(
                0,
                0,
                "Enlarge terminal to inspect (12×5 minimum)",
                palette.status(),
            );
        } else if text.is_empty() {
            buffer.set_str(0, 2, "<empty>", palette.text());
        } else {
            for (row, line) in part.rows.iter().enumerate() {
                buffer.set_str(0, row + 2, line, palette.text());
            }
        }
        if height >= 2 {
            buffer.set_str(
                0,
                height - 2,
                "Tab before/after · ↑↓ or PgUp/PgDn pages · Home start",
                palette.overlay_quiet(),
            );
            buffer.set_str(
                0,
                height - 1,
                "Esc / Enter / F4 returns · read-only inspection",
                palette.status(),
            );
        }
    }
}
/// Work and allocations are bounded by the visible page, not the full value.
/// Tokens never straddle a row. Escapes make invisible characters inspectable
/// and distinguish a literal backslash+n from an actual newline.
fn page(text: &str, start: usize, width: usize, height: usize) -> Page {
    let mut result = Page {
        rows: Vec::new(),
        end: start,
    };
    if width < 12 || height == 0 {
        return result;
    }
    let Some(tail) = text.get(start..) else {
        return result;
    };
    let mut row = String::new();
    let mut cells = 0;
    for ch in tail.chars() {
        let mut encoded = [0; 4];
        let raw = ch.encode_utf8(&mut encoded);
        let raw_width = LineLayout::new(raw, 1).width();
        let token = if ch == '\\' || ch.is_control() || raw_width == 0 {
            ch.escape_default().to_string()
        } else {
            raw.to_owned()
        };
        let token_width = LineLayout::new(&token, 1).width();
        if cells + token_width > width {
            result.rows.push(std::mem::take(&mut row));
            cells = 0;
            if result.rows.len() == height {
                return result;
            }
        }
        row.push_str(&token);
        cells += token_width;
        result.end += ch.len_utf8();
        if ch == '\n' {
            result.rows.push(std::mem::take(&mut row));
            cells = 0;
            if result.rows.len() == height {
                return result;
            }
        }
    }
    if !row.is_empty() {
        result.rows.push(row);
    }
    result
}
#[cfg(test)]
mod tests;
