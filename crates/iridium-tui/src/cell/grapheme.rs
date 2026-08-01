//! Grapheme clusters, and how many terminal cells one of them occupies.
//!
//! A cell holds a grapheme cluster, not a `char`. `e` followed by a combining
//! acute accent is one cluster, one cell and two `char`s; a family emoji is one
//! cluster, *two* cells and seven `char`s. Storing a `char` per cell would
//! silently drop the accent and shred the emoji.

use core::fmt;
use core::fmt::Write as _;

use unicode_width::UnicodeWidthStr;

/// A grapheme cluster that can be painted, together with the number of cells
/// it occupies.
///
/// Every value of this type is renderable by construction: it is non-empty, it
/// contains no control characters, and it occupies one or two cells. That is
/// what lets the cell buffer treat the width as settled rather than
/// recomputing it, and what keeps control characters — which would move the
/// terminal's cursor if they reached it — out of the damage output entirely.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Grapheme {
    store: Store,
    width: u8,
}

/// How the cluster's text is held.
///
/// The single-`char` case is the overwhelmingly common one and is kept inline;
/// only a genuine multi-`char` cluster allocates. [`Grapheme::new`] guarantees
/// that a one-`char` cluster is always [`Store::Single`], so two equal clusters
/// always compare equal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Store {
    /// A cluster of exactly one `char`.
    Single(char),
    /// A cluster of two or more `char`s.
    Cluster(Box<str>),
}

impl Grapheme {
    /// A single space: the content of a blank cell.
    pub const SPACE: Self = Self {
        store: Store::Single(' '),
        width: 1,
    };

    /// The grapheme cluster in `cluster`, if it can be painted.
    ///
    /// Returns `None` when the cluster cannot occupy a cell:
    ///
    /// * it is empty;
    /// * it contains a control character (which includes `"\r\n"`, a single
    ///   cluster, and `'\t'` — tab expansion is layout's job, and a tab that
    ///   reached the terminal would move the cursor);
    /// * it has zero display width, as a zero-width space or a stray combining
    ///   mark with no base character does. Such a cluster must not consume a
    ///   cell, and there is nothing here for it to attach to, so it is
    ///   dropped.
    ///
    /// A cluster wider than two cells is clamped to two: no terminal advances
    /// the cursor by more than two for one cluster, so treating it as wider
    /// would desynchronise every column after it.
    pub fn new(cluster: &str) -> Option<Self> {
        if cluster.is_empty() || cluster.chars().any(char::is_control) {
            return None;
        }
        let width = match UnicodeWidthStr::width(cluster) {
            0 => return None,
            1 => 1,
            _ => 2,
        };
        let mut chars = cluster.chars();
        let store = match (chars.next(), chars.next()) {
            (Some(single), None) => Store::Single(single),
            _ => Store::Cluster(Box::from(cluster)),
        };
        Some(Self { store, width })
    }

    /// The number of cells this cluster occupies: one or two.
    pub fn width(&self) -> usize {
        usize::from(self.width)
    }

    /// Whether this cluster occupies two cells.
    pub const fn is_double_width(&self) -> bool {
        self.width == 2
    }

    /// Appends this cluster's text to `out`.
    pub fn push_to(&self, out: &mut String) {
        match &self.store {
            Store::Single(single) => out.push(*single),
            Store::Cluster(cluster) => out.push_str(cluster),
        }
    }
}

impl fmt::Display for Grapheme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.store {
            Store::Single(single) => f.write_char(*single),
            Store::Cluster(cluster) => f.write_str(cluster),
        }
    }
}

#[cfg(test)]
mod tests {
    use unicode_segmentation::UnicodeSegmentation as _;

    use super::*;

    /// The clusters of `text`, in order, dropping the unrenderable ones.
    fn clusters(text: &str) -> Vec<Grapheme> {
        text.graphemes(true).filter_map(Grapheme::new).collect()
    }

    #[test]
    fn ascii_is_one_cell_and_one_char() {
        let grapheme = Grapheme::new("a").expect("`a` is renderable");
        assert_eq!(grapheme.width(), 1);
        assert!(!grapheme.is_double_width());
        assert_eq!(grapheme.to_string(), "a");
        assert_eq!(grapheme.store, Store::Single('a'));
    }

    #[test]
    fn cjk_is_two_cells() {
        for text in ["漢", "字", "한", "あ"] {
            let grapheme = Grapheme::new(text).expect("CJK is renderable");
            assert_eq!(grapheme.width(), 2, "{text} should be two cells");
            assert!(grapheme.is_double_width());
        }
    }

    #[test]
    fn combining_marks_stay_in_one_cell() {
        // `e` + U+0301 COMBINING ACUTE ACCENT: two chars, one cluster, one cell.
        let grapheme = Grapheme::new("e\u{301}").expect("a combining sequence is renderable");
        assert_eq!(grapheme.width(), 1);
        assert_eq!(grapheme.to_string(), "e\u{301}");
        assert!(matches!(grapheme.store, Store::Cluster(_)));
    }

    #[test]
    fn a_long_combining_sequence_is_still_one_cell() {
        let text = format!("a{}", "\u{301}".repeat(40));
        let grapheme = Grapheme::new(&text).expect("a long combining sequence is renderable");
        assert_eq!(grapheme.width(), 1);
        assert_eq!(grapheme.to_string(), text);
    }

    #[test]
    fn a_zwj_sequence_is_one_grapheme_of_at_most_two_cells() {
        // U+1F468 U+200D U+1F469 U+200D U+1F467 U+200D U+1F466: seven chars.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        assert_eq!(family.graphemes(true).count(), 1, "one cluster");
        assert_eq!(family.chars().count(), 7, "seven chars");
        let grapheme = Grapheme::new(family).expect("a ZWJ sequence is renderable");
        assert_eq!(grapheme.width(), 2);
        assert_eq!(grapheme.to_string(), family);
    }

    #[test]
    fn zero_width_clusters_are_rejected() {
        // A zero-width space, and a combining mark with no base character.
        assert_eq!(Grapheme::new("\u{200B}"), None);
        assert_eq!(Grapheme::new("\u{301}"), None);
    }

    #[test]
    fn control_characters_are_rejected() {
        for text in ["\t", "\n", "\r", "\r\n", "\u{1B}", "\u{7F}", "\u{0}"] {
            assert_eq!(Grapheme::new(text), None, "{text:?} must not reach a cell");
        }
    }

    #[test]
    fn the_empty_string_is_rejected() {
        assert_eq!(Grapheme::new(""), None);
    }

    #[test]
    fn one_char_clusters_never_allocate() {
        // Two clusters with the same text must compare equal, which they only
        // do if the single-char case is always stored the same way.
        let from_str = Grapheme::new("x").expect("renderable");
        assert_eq!(from_str.store, Store::Single('x'));
        assert_eq!(from_str, Grapheme::new("x").expect("renderable"));
    }

    #[test]
    fn a_string_segments_into_the_expected_cells() {
        let cells = clusters("aé漢\u{200B}b");
        let widths: Vec<usize> = cells.iter().map(Grapheme::width).collect();
        assert_eq!(widths, vec![1, 1, 2, 1]);
        let text: String = cells.iter().map(ToString::to_string).collect();
        assert_eq!(text, "aé漢b");
    }

    #[test]
    fn push_to_appends_the_whole_cluster() {
        let mut out = String::new();
        Grapheme::new("a").expect("renderable").push_to(&mut out);
        Grapheme::new("e\u{301}")
            .expect("renderable")
            .push_to(&mut out);
        assert_eq!(out, "ae\u{301}");
    }

    #[test]
    fn space_is_a_single_cell() {
        assert_eq!(Grapheme::SPACE.width(), 1);
        assert_eq!(Grapheme::SPACE.to_string(), " ");
        assert_eq!(Grapheme::new(" "), Some(Grapheme::SPACE));
    }
}
