//! One editable line of text with a caret in it, and nothing else.
//!
//! # Why this is shared rather than each panel's own
//!
//! ⭐ **Four surfaces already type into one of these** — the command palette's
//! query, the search bar's two fields, the file explorer's filter, and the name
//! being typed while its rows are edited — and a fifth is owed to the terminal
//! face. A field that behaved differently in one of them would show up as "the
//! arrow key skips half an emoji in the sidebar but not in the palette", which
//! is precisely the kind of divergence a single implementation makes
//! impossible.
//!
//! # This is not a second editing model
//!
//! [`Entry`] is a line of text with a caret, not a document: no history, no
//! selections, no commands, and nothing in it reachable from document text.
//! The kernel's editing verbs act on a `Document` through reversible commands,
//! and a file name being typed is a `&str` argument.
//!
//! # The two units, and why they differ
//!
//! The caret is a **byte offset that always sits on a grapheme-cluster
//! boundary**. Every motion and deletion moves whole clusters — removing one
//! `char` of a joined emoji would leave a dangling zero-width joiner in a file
//! name.
//!
//! The caret's **column**, by contrast, is counted in `char`s, because that is
//! the grid the GPU face draws in: the compositor places the document caret at
//! `column × char_width`, and a field's caret follows the same convention
//! rather than inventing a second one. ⚠️ A terminal face measuring in cells
//! must convert rather than assume — a cell is not a `char` for wide glyphs —
//! and that conversion is the face's, exactly as the pixel one is.

use std::ops::Range;

use iridium_editor::KeyCode;
use unicode_segmentation::UnicodeSegmentation as _;

/// One editable line of text with a caret in it.
///
/// The caret is a byte offset on a grapheme-cluster boundary; see this
/// module's documentation for why it is a cluster boundary and why its
/// column is counted in `char`s.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// The text, which never contains a control character.
    text: String,
    /// The caret, as a byte offset that is always on a cluster boundary.
    caret: usize,
}

impl Entry {
    /// An empty field.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A field already holding `text`, with the caret after it.
    ///
    /// Built by [`insert`](Self::insert)ing one character at a time rather than
    /// by assigning the string, so the field's one invariant — **no control
    /// character ever enters a file name** — holds by construction here as it
    /// does for typing. A second way in that assigned the string directly would
    /// be a second way for a newline to reach a path, and the caller's text is
    /// exactly the kind that could carry one: it comes from an
    /// [`OsStr`](std::ffi::OsStr), which permits bytes a keyboard cannot send.
    #[must_use]
    pub fn with_text(text: &str) -> Self {
        let mut entry = Self::new();
        for character in text.chars() {
            let _ = entry.insert(character);
        }
        entry
    }

    /// The field's text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Applies a motion or a deletion named by a key.
    ///
    /// Keys this does not name leave the field alone; the caller has already
    /// consumed them. The overlays call the named operations directly because
    /// they need to know whether the text changed; the prompt does not.
    pub fn edit(&mut self, key: KeyCode) {
        let _ = match key {
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.move_home(),
            KeyCode::End => self.move_end(),
            _ => false,
        };
    }

    /// Inserts a character at the caret, which then sits after it.
    ///
    /// Control characters are refused: nothing in the face can paint one, and
    /// a newline in a file name would be a character the user could see no
    /// trace of while it silently changed which file was written.
    ///
    /// Returns whether anything was inserted.
    pub fn insert(&mut self, character: char) -> bool {
        if character.is_control() {
            return false;
        }
        // The caret is on a cluster boundary by construction. Recovering
        // rather than trusting it is what keeps a mistake anywhere else in
        // this module from becoming a panic mid-frame.
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
    pub fn backspace(&mut self) -> bool {
        let Some(range) = self.cluster_before() else {
            return false;
        };
        self.caret = range.start;
        self.remove(range)
    }

    /// Removes the whole grapheme cluster at the caret, leaving it in place.
    ///
    /// Returns whether anything was removed.
    pub fn delete(&mut self) -> bool {
        let Some(range) = self.cluster_at() else {
            return false;
        };
        self.remove(range)
    }

    /// Moves the caret one cluster left. Returns whether it moved.
    pub fn move_left(&mut self) -> bool {
        let Some(range) = self.cluster_before() else {
            return false;
        };
        self.caret = range.start;
        true
    }

    /// Moves the caret one cluster right. Returns whether it moved.
    pub fn move_right(&mut self) -> bool {
        let Some(range) = self.cluster_at() else {
            return false;
        };
        self.caret = range.end;
        true
    }

    /// Moves the caret to the start. Returns whether it moved.
    pub const fn move_home(&mut self) -> bool {
        let moved = self.caret != 0;
        self.caret = 0;
        moved
    }

    /// Moves the caret past the last cluster. Returns whether it moved.
    pub fn move_end(&mut self) -> bool {
        let moved = self.caret != self.text.len();
        self.caret = self.text.len();
        moved
    }

    /// The caret's column, counted in `char`s from the field's start.
    ///
    /// `char`s rather than cells because the GPU face draws on a
    /// `char × char_width` grid — the same convention the compositor places
    /// the document caret with.
    #[must_use]
    pub fn caret_column(&self) -> usize {
        self.text.get(..self.caret).map_or_else(
            || self.text.chars().count(),
            |prefix| prefix.chars().count(),
        )
    }

    /// The byte range of the cluster ending at the caret, if there is one.
    fn cluster_before(&self) -> Option<Range<usize>> {
        let prefix = self.text.get(..self.caret)?;
        let (start, cluster) = prefix.grapheme_indices(true).next_back()?;
        Some(start..start + cluster.len())
    }

    /// The byte range of the cluster starting at the caret, if there is one.
    fn cluster_at(&self) -> Option<Range<usize>> {
        let suffix = self.text.get(self.caret..)?;
        let cluster = suffix.graphemes(true).next()?;
        Some(self.caret..self.caret + cluster.len())
    }

    /// Removes a byte range, refusing one that is not a valid slice.
    ///
    /// Every range this module produces comes from a cluster and is valid.
    /// The check is here because `String::drain` panics on one that is not.
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

#[cfg(test)]
mod tests {
    use super::Entry;
    use iridium_editor::KeyCode;

    /// The invariant the type exists to hold: nothing that cannot be painted,
    /// and nothing that could silently change which file is written.
    #[test]
    fn a_control_character_never_enters_the_field() {
        let mut entry = Entry::new();
        assert!(!entry.insert('\n'), "a newline is refused");
        assert!(!entry.insert('\t'), "a tab is refused");
        assert!(entry.insert('a'), "an ordinary character is not");
        assert_eq!(entry.text(), "a");
    }

    /// ⚠️ **The second way in has to hold the same invariant as typing.**
    /// `with_text` takes a caller's string — which can come from an `OsStr`,
    /// and so can carry bytes a keyboard cannot send.
    #[test]
    fn prefilled_text_is_filtered_the_same_way_typing_is() {
        let entry = Entry::with_text("na\nme");
        assert_eq!(entry.text(), "name", "the newline never landed");
        assert_eq!(entry.caret_column(), 4, "the caret is after what remains");
    }

    /// The whole reason the caret is measured in clusters: a family emoji is
    /// several `char`s joined by zero-width joiners, and deleting one of them
    /// leaves a different emoji behind.
    #[test]
    fn backspace_removes_a_whole_cluster_not_a_char() {
        let mut entry = Entry::with_text("a👨‍👩‍👧");
        assert!(entry.backspace());
        assert_eq!(entry.text(), "a", "the joined cluster went as one");
        assert!(entry.backspace());
        assert_eq!(entry.text(), "");
        assert!(!entry.backspace(), "an empty field has nothing to remove");
    }

    /// The same rule one direction across, and with the caret left where it was.
    #[test]
    fn delete_removes_the_cluster_at_the_caret_and_leaves_it_in_place() {
        let mut entry = Entry::with_text("👨‍👩‍👧z");
        entry.move_home();
        assert!(entry.delete());
        assert_eq!(entry.text(), "z");
        assert_eq!(entry.caret_column(), 0, "the caret did not follow");
    }

    /// Motion is in clusters too, in both directions, and reports whether it
    /// actually moved — which is what lets a panel repaint only when something
    /// changed.
    #[test]
    fn motion_steps_whole_clusters_and_says_whether_it_moved() {
        let mut entry = Entry::with_text("é👍b");
        assert!(entry.move_left(), "off the end, onto the cluster before it");
        assert!(entry.move_left());
        assert_eq!(entry.caret_column(), 1, "past `é`, before the thumb");
        assert!(entry.move_right());
        assert!(!entry.move_right() || entry.caret_column() > 0);
        assert!(entry.move_home());
        assert!(!entry.move_home(), "already there is not a move");
        assert!(entry.move_end());
        assert!(!entry.move_end(), "and neither is this");
    }

    /// ⚠️ **The column is `char`s, not clusters and not bytes.** A face that
    /// read it as either would place the caret in the wrong cell.
    #[test]
    fn the_column_counts_chars_rather_than_clusters_or_bytes() {
        let entry = Entry::with_text("é");
        assert_eq!(entry.text().len(), 2, "two bytes");
        assert_eq!(entry.caret_column(), 1, "but one char");
    }

    /// Insertion lands at the caret rather than at the end — the thing that
    /// makes the field editable rather than append-only.
    #[test]
    fn insertion_lands_at_the_caret() {
        let mut entry = Entry::with_text("ac");
        assert!(entry.move_left());
        assert!(entry.insert('b'));
        assert_eq!(entry.text(), "abc");
        assert_eq!(entry.caret_column(), 2, "and the caret is after it");
    }

    /// The key-named form the prompt drives the field with, kept in step with
    /// the verbs the panels call directly — one table, not two.
    #[test]
    fn edit_names_the_same_verbs_the_panels_call_by_hand() {
        let mut entry = Entry::with_text("ab");
        entry.edit(KeyCode::Left);
        assert_eq!(entry.caret_column(), 1);
        entry.edit(KeyCode::Backspace);
        assert_eq!(entry.text(), "b");
        entry.edit(KeyCode::End);
        entry.edit(KeyCode::Home);
        assert_eq!(entry.caret_column(), 0);
        entry.edit(KeyCode::Delete);
        assert_eq!(entry.text(), "");
        // A key it does not name leaves the field exactly as it was.
        entry.edit(KeyCode::Enter);
        assert_eq!(entry.text(), "");
    }
}
