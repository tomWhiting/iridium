//! Exact query or replacement text presented in a single-row search field.
//!
//! # Why this is not a kernel verb
//!
//! Editing text is the kernel's job and this crate must not have a second copy
//! of it — but an overlay's query is not a document. The kernel's editing verbs
//! act on a [`Document`](iridium_editor::Document) through reversible commands
//! and an undo tree; the query is a `&str` argument to
//! [`Editor::find`](iridium_editor::Editor::find) or
//! [`palette::search_text`](iridium_editor::commands::palette::search_text) and
//! the kernel offers no way to edit one. So this is a field, not a buffer: no
//! history, no document commands, and nothing here is reachable from
//! document text.
//!
//! # The caret is a byte offset on a cluster boundary
//!
//! It is deliberately not a `char` index and not a cell. A `char` index would
//! let the caret land between the two `char`s of `e` + combining acute, which
//! is a position no terminal can draw a cursor at; a cell would be ambiguous
//! across the two halves of a double-width glyph. Every motion here moves whole
//! grapheme clusters, measured by [`LineLayout`] — the crate's single place for
//! cluster and cell arithmetic — so backspacing `👨‍👩‍👧‍👦` removes seven `char`s and
//! two cells in one press, and never leaves a dangling joiner behind.

mod selection;
pub(super) use selection::Motion;

use core::ops::Range;
use std::borrow::Cow;

use crate::cell::{CellBuffer, Style};
use crate::frame::line::LineLayout;
use crate::frame::text::{self, TextArea};

/// Editable text presented on one row, with visible markers for controls.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Field {
    /// Exact field contents; pasted controls are escaped only for display.
    text: String,
    /// The caret, as a byte offset that is always on a cluster boundary.
    caret: usize,
    /// Fixed end of an extending selection, in grapheme-aligned bytes.
    anchor: Option<usize>,
}

impl Field {
    /// The field's text.
    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// Whether the field is empty.
    pub(super) fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Inserts a character at the caret, which then sits after it.
    ///
    /// Typed controls remain commands. A paste uses `insert_text` instead and
    /// represents controls with visible markers without changing their value.
    ///
    /// Returns whether anything was inserted.
    pub(super) fn insert(&mut self, character: char) -> bool {
        if character.is_control() {
            return false;
        }
        let mut bytes = [0; 4];
        self.insert_text(character.encode_utf8(&mut bytes))
    }

    /// Inserts a complete paste without interpreting or discarding its bytes.
    /// Display escapes are separate from the value sent to search/replace.
    pub(super) fn insert_text(&mut self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if !self.text.is_char_boundary(self.caret) {
            self.caret = self.text.len();
        }
        let range = self.selection().unwrap_or(self.caret..self.caret);
        self.caret = range.start + text.len();
        self.anchor = None;
        self.text.replace_range(range, text);
        self.snap_caret_after_cluster();
        true
    }

    /// Removes the whole grapheme cluster before the caret.
    ///
    /// Returns whether anything was removed.
    pub(super) fn backspace(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        let Some(range) = self.cluster_before() else {
            return false;
        };
        self.caret = range.start;
        self.remove(range)
    }

    /// Removes the whole grapheme cluster at the caret, leaving it in place.
    ///
    /// Returns whether anything was removed.
    pub(super) fn delete(&mut self) -> bool {
        if self.delete_selection() {
            return true;
        }
        let Some(range) = self.cluster_at() else {
            return false;
        };
        self.remove(range)
    }

    /// Moves left, collapsing a selection to its beginning.
    pub(super) fn move_left(&mut self) -> bool {
        self.navigate(Motion::Left, false)
    }

    /// Moves right, collapsing a selection to its end.
    pub(super) fn move_right(&mut self) -> bool {
        self.navigate(Motion::Right, false)
    }

    /// Moves to the beginning of the field.
    pub(super) fn move_home(&mut self) -> bool {
        self.navigate(Motion::Home, false)
    }

    /// Moves to the end of the field.
    pub(super) fn move_end(&mut self) -> bool {
        self.navigate(Motion::End, false)
    }

    /// The display column the caret sits at, counted from the field's start.
    ///
    /// This is a cell count, not a `char` count: a caret after one CJK
    /// ideograph is at column two.
    pub(super) fn caret_cell(&self) -> usize {
        let prefix = self.text.get(..self.caret).unwrap_or(&self.text);
        LineLayout::new(&visible_text(prefix), 1).width()
    }

    /// Paints the field into `area`, blanking whatever it does not fill.
    ///
    /// The area's `scroll` decides which part of a field wider than its box is
    /// shown; [`scroll_for`] computes one that keeps the caret in view. A
    /// double-width glyph the scroll or the right edge cuts through is painted
    /// as a space in its own style, which is [`text::paint_text`]'s rule and
    /// the only one a terminal can honour.
    pub(super) fn paint(&self, buffer: &mut CellBuffer, row: usize, area: TextArea, style: Style) {
        for offset in 0..area.width {
            buffer.set_str(area.origin + offset, row, " ", style);
        }
        text::paint_text(buffer, row, 0, &visible_text(&self.text), style, area);
    }

    /// The cluster layout of the field's text.
    ///
    /// Raw cluster boundaries drive editing; display markers drive painting.
    /// Tabs are given a nominal width because only byte ranges are used here.
    fn layout(&self) -> LineLayout {
        LineLayout::new(&self.text, 1)
    }

    /// Editing can join adjacent graphemes, including a newly adjacent CR/LF.
    fn snap_caret_after_cluster(&mut self) {
        if let Some(cluster) = self.layout().clusters().iter().find(|cluster| {
            cluster.byte_index() < self.caret
                && cluster.byte_index() + cluster.byte_count() > self.caret
        }) {
            self.caret = cluster.byte_index() + cluster.byte_count();
        }
    }

    /// The byte range of the cluster ending at the caret, if there is one.
    fn cluster_before(&self) -> Option<Range<usize>> {
        self.layout()
            .clusters()
            .iter()
            .find(|cluster| cluster.byte_index() + cluster.byte_count() == self.caret)
            .map(|cluster| cluster.byte_index()..self.caret)
    }

    /// The byte range of the cluster starting at the caret, if there is one.
    fn cluster_at(&self) -> Option<Range<usize>> {
        self.layout()
            .clusters()
            .iter()
            .find(|cluster| cluster.byte_index() == self.caret)
            .map(|cluster| cluster.byte_index()..cluster.byte_index() + cluster.byte_count())
    }

    /// Removes a byte range, refusing one that is not a valid slice.
    ///
    /// Every range this module produces comes from a cluster and is valid. The
    /// check is here because `String::drain` panics on one that is not, and a
    /// panic in raw mode is the worst failure this face has.
    fn remove(&mut self, range: Range<usize>) -> bool {
        if range.start >= range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return false;
        }
        self.text.drain(range);
        self.snap_caret_after_cluster();
        true
    }
}

/// Keep raw values intact while preventing controls from reaching the terminal.
/// Common whitespace gets a compact marker; other controls show their codepoint.
fn visible_text(text: &str) -> Cow<'_, str> {
    if !text.chars().any(char::is_control) {
        return Cow::Borrowed(text);
    }
    let mut visible = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '\n' => visible.push('␊'),
            '\r' => visible.push('␍'),
            '\t' => visible.push('␉'),
            control if control.is_control() => {
                visible.extend(control.escape_unicode());
            },
            printable => visible.push(printable),
        }
    }
    Cow::Owned(visible)
}

/// The scroll a box of `width` cells needs for `caret` to be inside it.
///
/// Zero until the caret would fall off the right edge, and then just enough to
/// keep it in the last cell. The caret's own cell counts: a caret past the last
/// glyph of a full field must still be visible, or typing into a field that has
/// filled its box gives no feedback at all.
pub(super) const fn scroll_for(caret: usize, width: usize) -> usize {
    if width == 0 || caret < width {
        return 0;
    }
    caret + 1 - width
}

#[cfg(test)]
mod tests;
