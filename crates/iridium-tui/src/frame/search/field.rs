//! One editable line of text belonging to the search overlay.
//!
//! # Why this is not a kernel verb
//!
//! Editing text is the kernel's job and this crate must not have a second copy
//! of it — but a search query is not a document. The kernel's editing verbs act
//! on a [`Document`](iridium_editor::Document) through reversible commands and
//! an undo tree; the query is a `&str` argument to
//! [`Editor::find`](iridium_editor::Editor::find) and the kernel offers no way
//! to edit one. So this is a field, not a buffer: no history, no selections, no
//! commands, and nothing here is reachable from document text.
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

use core::ops::Range;

use crate::cell::{CellBuffer, Style};
use crate::frame::line::{LineLayout, PlacedCluster};
use crate::frame::text::{self, TextArea};

/// One line of editable text with a caret in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Field {
    /// The text, which never contains a control character.
    text: String,
    /// The caret, as a byte offset that is always on a cluster boundary.
    caret: usize,
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
    /// Control characters are refused. Nothing in this crate can paint one —
    /// see [`Grapheme::new`](crate::cell::Grapheme::new) — and a tab or a
    /// newline reaching a query field would be text the user could see no trace
    /// of while it silently changed what the search matched.
    ///
    /// Returns whether anything was inserted.
    pub(super) fn insert(&mut self, character: char) -> bool {
        if character.is_control() {
            return false;
        }
        // The caret is on a cluster boundary by construction, so this holds;
        // recovering rather than trusting it is what keeps a mistake anywhere
        // else in this module from becoming a panic with the terminal in raw
        // mode.
        if !self.text.is_char_boundary(self.caret) {
            self.caret = self.text.len();
        }
        self.text.insert(self.caret, character);
        self.caret += character.len_utf8();
        true
    }

    /// Removes the whole grapheme cluster before the caret.
    ///
    /// Returns whether anything was removed.
    pub(super) fn backspace(&mut self) -> bool {
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
        let Some(range) = self.cluster_at() else {
            return false;
        };
        self.remove(range)
    }

    /// Moves the caret one cluster left. Returns whether it moved.
    pub(super) fn move_left(&mut self) -> bool {
        let Some(range) = self.cluster_before() else {
            return false;
        };
        self.caret = range.start;
        true
    }

    /// Moves the caret one cluster right. Returns whether it moved.
    pub(super) fn move_right(&mut self) -> bool {
        let Some(range) = self.cluster_at() else {
            return false;
        };
        self.caret = range.end;
        true
    }

    /// Moves the caret to the start. Returns whether it moved.
    pub(super) const fn move_home(&mut self) -> bool {
        let moved = self.caret != 0;
        self.caret = 0;
        moved
    }

    /// Moves the caret past the last cluster. Returns whether it moved.
    pub(super) fn move_end(&mut self) -> bool {
        let moved = self.caret != self.text.len();
        self.caret = self.text.len();
        moved
    }

    /// The display column the caret sits at, counted from the field's start.
    ///
    /// This is a cell count, not a `char` count: a caret after one CJK
    /// ideograph is at column two.
    pub(super) fn caret_cell(&self) -> usize {
        let layout = self.layout();
        if self.caret >= self.text.len() {
            return layout.width();
        }
        layout
            .clusters()
            .iter()
            .find(|cluster| cluster.byte_index() == self.caret)
            .map_or_else(|| layout.width(), PlacedCluster::column)
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
        text::paint_text(buffer, row, 0, &self.text, style, area);
    }

    /// The cluster layout of the field's text.
    ///
    /// A tab width of one because a field has no tab stops: nothing can insert
    /// a tab into it, and the width is still needed for the general call.
    fn layout(&self) -> LineLayout {
        LineLayout::new(&self.text, 1)
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
        true
    }
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
mod tests {
    use super::*;
    use crate::cell::CellContent;

    /// A family emoji: seven `char`s, one cluster, two cells.
    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

    /// A field holding `text` with the caret at its end.
    fn ending_at(text: &str) -> Field {
        let mut field = Field::default();
        for character in text.chars() {
            assert!(field.insert(character), "{character:?} must be insertable");
        }
        field
    }

    /// The text of a row, with continuation cells contributing nothing.
    fn row_text(buffer: &CellBuffer, row: usize) -> String {
        let mut out = String::new();
        let Some(cells) = buffer.row(row) else {
            return out;
        };
        for cell in cells {
            match cell.content() {
                CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
                CellContent::Continuation => {},
            }
        }
        out
    }

    #[test]
    fn typing_ascii_fills_the_field_and_moves_the_caret() {
        let field = ending_at("foo");
        assert_eq!(field.text(), "foo");
        assert_eq!(field.caret_cell(), 3);
    }

    #[test]
    fn the_caret_counts_cells_not_chars_after_a_double_width_glyph() {
        let field = ending_at("漢字");
        assert_eq!(field.text().chars().count(), 2);
        assert_eq!(
            field.caret_cell(),
            4,
            "two ideographs are four cells, not two"
        );
    }

    #[test]
    fn the_caret_counts_one_cell_for_a_combining_sequence() {
        let field = ending_at("e\u{301}");
        assert_eq!(field.text().chars().count(), 2);
        assert_eq!(field.caret_cell(), 1);
    }

    #[test]
    fn backspace_removes_a_whole_cluster_not_a_char() {
        // Removing one `char` would leave a zero-width joiner dangling and the
        // query would search for text no document contains.
        let mut field = ending_at(FAMILY);
        assert_eq!(field.text().chars().count(), 7);
        assert!(field.backspace());
        assert_eq!(field.text(), "", "one press must remove the whole cluster");
        assert!(!field.backspace());
    }

    #[test]
    fn backspace_removes_a_whole_combining_sequence() {
        let mut field = ending_at("ae\u{301}");
        assert!(field.backspace());
        assert_eq!(field.text(), "a");
    }

    #[test]
    fn moving_left_and_right_steps_over_whole_clusters() {
        let mut field = ending_at(&format!("a{FAMILY}b"));
        assert_eq!(field.caret_cell(), 4);
        assert!(field.move_left());
        assert_eq!(field.caret_cell(), 3, "before the `b`");
        assert!(field.move_left());
        assert_eq!(field.caret_cell(), 1, "before the family, in one step");
        assert!(field.move_left());
        assert_eq!(field.caret_cell(), 0);
        assert!(!field.move_left());

        assert!(field.move_right());
        assert_eq!(field.caret_cell(), 1);
        assert!(field.move_right());
        assert_eq!(field.caret_cell(), 3, "past the family, in one step");
    }

    #[test]
    fn deleting_forwards_removes_the_cluster_at_the_caret() {
        let mut field = ending_at("a漢b");
        field.move_home();
        assert!(field.move_right());
        assert!(field.delete());
        assert_eq!(field.text(), "ab");
        assert_eq!(field.caret_cell(), 1, "the caret does not move");
    }

    #[test]
    fn deleting_at_the_end_does_nothing() {
        let mut field = ending_at("ab");
        assert!(!field.delete());
        assert_eq!(field.text(), "ab");
    }

    #[test]
    fn insertion_lands_at_the_caret_rather_than_the_end() {
        let mut field = ending_at("ac");
        assert!(field.move_left());
        assert!(field.insert('b'));
        assert_eq!(field.text(), "abc");
        assert_eq!(field.caret_cell(), 2);
    }

    #[test]
    fn home_and_end_report_whether_they_moved() {
        let mut field = ending_at("abc");
        assert!(!field.move_end());
        assert!(field.move_home());
        assert!(!field.move_home());
        assert!(field.move_end());
        assert_eq!(field.caret_cell(), 3);
    }

    #[test]
    fn control_characters_are_refused() {
        let mut field = Field::default();
        for character in ['\n', '\t', '\r', '\u{7}', '\u{1b}'] {
            assert!(
                !field.insert(character),
                "{character:?} must not enter a field"
            );
        }
        assert!(field.is_empty());
    }

    #[test]
    fn the_empty_field_has_no_motion_and_no_width() {
        let mut field = Field::default();
        assert!(field.is_empty());
        assert_eq!(field.caret_cell(), 0);
        assert!(!field.backspace());
        assert!(!field.delete());
        assert!(!field.move_left());
        assert!(!field.move_right());
    }

    #[test]
    fn the_scroll_keeps_the_caret_inside_the_box() {
        assert_eq!(scroll_for(0, 10), 0);
        assert_eq!(scroll_for(9, 10), 0);
        assert_eq!(scroll_for(10, 10), 1, "the caret's own cell must fit");
        assert_eq!(scroll_for(25, 10), 16);
        assert_eq!(scroll_for(3, 0), 0, "a box with no cells cannot scroll");
    }

    #[test]
    fn painting_blanks_the_part_of_the_box_the_text_does_not_fill() {
        let field = ending_at("ab");
        let mut buffer = CellBuffer::new(8, 1);
        let area = TextArea {
            origin: 2,
            width: 4,
            scroll: 0,
        };
        field.paint(&mut buffer, 0, area, Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), "  ab    ");
    }

    #[test]
    fn a_field_wider_than_its_box_scrolls_to_the_caret() {
        let field = ending_at("abcdefgh");
        let mut buffer = CellBuffer::new(4, 1);
        let area = TextArea {
            origin: 0,
            width: 4,
            scroll: scroll_for(field.caret_cell(), 4),
        };
        field.paint(&mut buffer, 0, area, Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), "fgh ");
    }

    #[test]
    fn a_glyph_the_scroll_cuts_through_is_painted_as_a_space() {
        // `漢` occupies cells 0 and 1; scrolling to 1 shows half of it, which
        // is a state no terminal can render.
        let field = ending_at("漢ab");
        let mut buffer = CellBuffer::new(3, 1);
        let area = TextArea {
            origin: 0,
            width: 3,
            scroll: 1,
        };
        field.paint(&mut buffer, 0, area, Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), " ab");
    }
}
