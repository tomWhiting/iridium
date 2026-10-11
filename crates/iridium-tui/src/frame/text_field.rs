//! Public host fields reuse the search overlay's exact text and grapheme rules.
use super::{
    Palette, TextArea,
    field::{Field, Motion, scroll_for},
    line::LineLayout,
};
use crate::cell::CellBuffer;
use iridium_editor::{KeyCode, KeyEvent};

/// A query/replacement field with selection, word motion and exact pasted text.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct TextField {
    field: Field,
    identity: std::sync::Arc<()>,
    mouse_scroll: Option<usize>,
}
impl Clone for TextField {
    fn clone(&self) -> Self {
        Self {
            field: self.field.clone(),
            identity: std::sync::Arc::new(()),
            mouse_scroll: self.mouse_scroll,
        }
    }
}
/// A selected field snapshot; deletion waits for clipboard delivery.
#[derive(Clone, Debug)]
pub struct TextFieldCut {
    field: Field,
    identity: std::sync::Arc<()>,
    text: String,
}
impl TextFieldCut {
    /// Exact selected text to send to the clipboard.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}
impl TextField {
    /// Exact selected text, if this field has a nonempty selection.
    #[must_use]
    pub fn selected_text(&self) -> Option<&str> {
        self.field.selected_text()
    }
    /// Capture a cut without changing the field.
    #[must_use]
    pub fn capture_cut(&self) -> Option<TextFieldCut> {
        Some(TextFieldCut {
            field: self.field.clone(),
            identity: self.identity.clone(),
            text: self.selected_text()?.to_owned(),
        })
    }
    /// Reject outstanding cuts after a field's owning panel closes.
    pub fn invalidate_cut(&mut self) {
        self.identity = std::sync::Arc::new(());
    }
    /// Delete only after delivery to the clipboard and exact target validation.
    ///
    /// # Errors
    /// A replaced field or changed text/selection leaves the field unchanged.
    pub fn finish_cut(&mut self, cut: &TextFieldCut) -> Result<(), String> {
        if !std::sync::Arc::ptr_eq(&self.identity, &cut.identity) || self.field != cut.field {
            return Err("Text copied; field or selection changed, so cut was cancelled".into());
        }
        self.field.delete_selection();
        self.invalidate_cut();
        Ok(())
    }

    /// Current value. Display markers never replace these bytes.
    #[must_use]
    pub fn text(&self) -> &str {
        self.field.text()
    }
    /// Replace the field's contents and reset its selection.
    ///
    /// # Errors
    /// Refuses oversized input without changing the existing field.
    pub fn set_text(&mut self, text: &str, limit: usize) -> Result<(), String> {
        if text.len() > limit {
            return Err(format!("field exceeds {limit} bytes"));
        }
        let mut next = Field::default();
        next.insert_text(text);
        self.field = next;
        self.mouse_scroll = None;
        Ok(())
    }
    /// Paste exact text over the selection, including newlines/tabs and controls.
    ///
    /// # Errors
    /// Refuses a result exceeding the byte limit; field state remains unchanged.
    pub fn paste(&mut self, text: &str, limit: usize) -> Result<bool, String> {
        let removed = self.field.selection().map_or(0, |range| range.len());
        let size = self
            .field
            .text()
            .len()
            .saturating_sub(removed)
            .saturating_add(text.len());
        if size > limit {
            return Err(format!("field exceeds {limit} bytes; paste refused"));
        }
        self.mouse_scroll = None;
        Ok(self.field.insert_text(text))
    }
    /// Standard field editing: Ctrl/Command+A, Shift extension, word movement
    /// with Ctrl/Option, and grapheme/word deletion. Panel commands remain host-owned.
    /// Returns whether the key was handled, including motions at a boundary.
    ///
    /// # Errors
    /// Typed insertion exceeding the byte limit leaves the value unchanged.
    pub fn handle_key(&mut self, event: &KeyEvent, limit: usize) -> Result<bool, String> {
        self.mouse_scroll = None;
        let modifiers = event.modifiers;
        if matches!(event.key, KeyCode::Char('a' | 'A'))
            && (modifiers.ctrl || modifiers.meta)
            && !modifiers.alt
        {
            self.field.select_all();
            return Ok(true);
        }
        let word = modifiers.ctrl || modifiers.alt;
        let motion = match event.key {
            KeyCode::Left if modifiers.meta => Some(Motion::Home),
            KeyCode::Right if modifiers.meta => Some(Motion::End),
            KeyCode::Left => Some(if word { Motion::WordLeft } else { Motion::Left }),
            KeyCode::Right => Some(if word {
                Motion::WordRight
            } else {
                Motion::Right
            }),
            KeyCode::Home => Some(Motion::Home),
            KeyCode::End => Some(Motion::End),
            _ => None,
        };
        if let Some(motion) = motion {
            self.field.navigate(motion, modifiers.shift);
            return Ok(true);
        }
        match event.key {
            KeyCode::Backspace | KeyCode::Delete => {
                if modifiers.meta {
                    self.field.navigate(
                        if event.key == KeyCode::Backspace {
                            Motion::Home
                        } else {
                            Motion::End
                        },
                        true,
                    );
                    self.field.delete_selection();
                } else if word {
                    self.field.delete_word(event.key == KeyCode::Delete);
                } else if event.key == KeyCode::Backspace {
                    self.field.backspace();
                } else {
                    self.field.delete();
                }
                Ok(true)
            },
            KeyCode::Char(ch)
                if !modifiers.ctrl
                    && !modifiers.meta
                    && (!modifiers.alt || modifiers.alt_graph)
                    && !ch.is_control() =>
            {
                let mut encoded = [0; 4];
                self.paste(ch.encode_utf8(&mut encoded), limit)?;
                Ok(true)
            },
            _ => Ok(false),
        }
    }
    fn area(&self, width: usize, label: &str) -> TextArea {
        let origin = LineLayout::new(label, 1).width().min(width);
        let width = width.saturating_sub(origin);
        TextArea {
            origin,
            width,
            scroll: self.mouse_scroll.map_or_else(
                || scroll_for(self.field.caret_cell(), width),
                |scroll| {
                    let caret = self.field.caret_cell();
                    if caret < scroll {
                        caret
                    } else if caret >= scroll.saturating_add(width) {
                        scroll_for(caret, width)
                    } else {
                        scroll
                    }
                },
            ),
        }
    }
    /// Place or extend the caret using the same label/scroll geometry as painting.
    /// Drag coordinates outside the field clamp to its visible boundaries.
    pub fn point(&mut self, column: usize, width: usize, label: &str, extend: bool) {
        let area = self.area(width, label);
        if area.width == 0 {
            return;
        }
        let cell = column.saturating_sub(area.origin).min(area.width) + area.scroll;
        self.mouse_scroll = Some(area.scroll);
        self.field.point_cell(cell, extend);
    }
    /// Paint a labelled field with horizontal caret following and selection.
    /// Returns the caret's screen column when focused and visible.
    pub fn paint(
        &self,
        buffer: &mut CellBuffer,
        row: usize,
        label: &str,
        focused: bool,
        palette: &Palette,
    ) -> Option<usize> {
        if row >= buffer.height() {
            return None;
        }
        let area = self.area(buffer.width(), label);
        buffer.set_str(0, row, label, palette.overlay_quiet());
        let caret = self.field.caret_cell();
        self.field
            .paint(buffer, row, area, palette.overlay_field(focused));
        self.field
            .paint_selection(buffer, row, area, palette.overlay_toggle(true));
        (focused && area.width > 0).then_some(area.origin + caret.saturating_sub(area.scroll))
    }
}
#[cfg(test)]
mod tests;
