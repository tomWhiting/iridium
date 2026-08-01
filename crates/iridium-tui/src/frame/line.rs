//! Where each grapheme cluster of one line lands, measured in cells.
//!
//! This is the only place in the face that converts between a document
//! *column* — which the kernel counts in `char`s — and a screen *cell*. The two
//! are not the same number and the mapping is not monotonic in `char` count:
//!
//! * a tab occupies however many cells reach the next tab stop;
//! * a CJK ideograph or an emoji occupies two cells;
//! * a grapheme cluster may be several `char`s in one cell — `e` plus a
//!   combining acute is two `char`s and one cell, a family emoji is seven
//!   `char`s and two cells;
//! * a zero-width or control character occupies no cell at all while still
//!   consuming `char`s and bytes.
//!
//! Every one of those makes `column == cell` wrong, which is why highlight
//! spans (byte ranges) and cursors (document positions) are resolved through
//! this type rather than by arithmetic at the call site.
//!
//! Nothing here is layout in the sense the kernel owns: which *lines* are
//! visible, and where they sit, is the kernel's fold-aware `Viewport`. This
//! measures *within* one line, which the kernel expresses in pixels against a
//! fixed character width — an assumption a terminal cannot make.

use unicode_segmentation::UnicodeSegmentation as _;

use crate::cell::Grapheme;

/// One grapheme cluster of a line, placed in cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedCluster {
    /// The cluster, when it can be painted.
    ///
    /// `None` for a tab — which is painted as spaces to the next tab stop —
    /// and for a cluster that occupies no cell at all.
    grapheme: Option<Grapheme>,
    /// Whether this cluster is a tab.
    is_tab: bool,
    /// The first display column this cluster occupies.
    column: usize,
    /// How many display columns it occupies: zero, one, two, or a tab's run.
    width: usize,
    /// The document column (in `char`s) of this cluster's first `char`.
    char_index: usize,
    /// How many `char`s the cluster spans.
    char_count: usize,
    /// The byte offset of this cluster from the start of the line.
    byte_index: usize,
    /// How many bytes the cluster spans.
    byte_count: usize,
}

impl PlacedCluster {
    /// The cluster, when it can be painted directly.
    pub const fn grapheme(&self) -> Option<&Grapheme> {
        self.grapheme.as_ref()
    }

    /// Whether this cluster is a tab, and so is painted as spaces.
    pub const fn is_tab(&self) -> bool {
        self.is_tab
    }

    /// The first display column this cluster occupies.
    pub const fn column(&self) -> usize {
        self.column
    }

    /// How many display columns this cluster occupies.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// The document column of this cluster's first `char`.
    pub const fn char_index(&self) -> usize {
        self.char_index
    }

    /// How many `char`s this cluster spans.
    pub const fn char_count(&self) -> usize {
        self.char_count
    }

    /// The byte offset of this cluster from the start of its line.
    pub const fn byte_index(&self) -> usize {
        self.byte_index
    }

    /// How many bytes this cluster spans.
    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }
}

/// Every cluster of one line, placed in cells.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineLayout {
    /// The clusters, in document order.
    clusters: Vec<PlacedCluster>,
    /// The total number of display columns the line occupies.
    width: usize,
    /// The number of `char`s in the line.
    char_count: usize,
    /// The number of bytes in the line.
    byte_count: usize,
}

impl LineLayout {
    /// Measures `text` — one line, without its line ending — into cells.
    ///
    /// `tab_width` is floored at one, matching the kernel's own rule for a
    /// zero width from a corrupt configuration (see the editing behaviours in
    /// `iridium_editor::input::keyboard`): a tab must always advance, or a
    /// caret could never move past one.
    pub fn new(text: &str, tab_width: usize) -> Self {
        let tab_width = tab_width.max(1);
        let mut clusters = Vec::new();
        let mut column = 0;
        let mut char_index = 0;

        for (byte_index, cluster) in text.grapheme_indices(true) {
            let char_count = cluster.chars().count();
            let grapheme = Grapheme::new(cluster);
            let is_tab = cluster == "\t";
            let width = if is_tab {
                tab_width - (column % tab_width)
            } else {
                grapheme.as_ref().map_or(0, Grapheme::width)
            };
            clusters.push(PlacedCluster {
                grapheme,
                is_tab,
                column,
                width,
                char_index,
                char_count,
                byte_index,
                byte_count: cluster.len(),
            });
            column += width;
            char_index += char_count;
        }

        Self {
            clusters,
            width: column,
            char_count: char_index,
            byte_count: text.len(),
        }
    }

    /// The number of display columns the whole line occupies.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// The number of `char`s in the line, which is its last valid column.
    pub const fn char_count(&self) -> usize {
        self.char_count
    }

    /// The number of bytes in the line.
    pub const fn byte_count(&self) -> usize {
        self.byte_count
    }

    /// The clusters, in document order.
    pub fn clusters(&self) -> &[PlacedCluster] {
        &self.clusters
    }

    /// The display column a document column lands on.
    ///
    /// A column inside a multi-`char` cluster snaps to that cluster's first
    /// cell: the cluster is painted as one unit and there is no cell between
    /// its `char`s to land on. A column at or past the end of the line lands
    /// one cell past the last one, which is where a caret at end of line goes.
    pub fn column_to_cell(&self, column: usize) -> usize {
        if column >= self.char_count {
            return self.width;
        }
        match self
            .clusters
            .binary_search_by(|cluster| cluster.char_index.cmp(&column))
        {
            Ok(index) => self.clusters.get(index).map_or(self.width, |c| c.column),
            // `column` falls inside the cluster before the insertion point.
            // There is always one: `column < char_count` and the first cluster
            // starts at zero.
            Err(index) => index
                .checked_sub(1)
                .and_then(|previous| self.clusters.get(previous))
                .map_or(0, |cluster| cluster.column),
        }
    }

    /// The document column at a display column.
    ///
    /// A cell inside a wider glyph — the continuation half of a double-width
    /// character, or any cell of a tab's run — resolves to that glyph's own
    /// column, because a caret cannot sit inside one. A cell at or past the
    /// end of the line resolves to the end of the line.
    pub fn cell_to_column(&self, cell: usize) -> usize {
        if cell >= self.width {
            return self.char_count;
        }
        // Zero-width clusters share a column with the cluster that follows
        // them, so the search walks forward to the first cluster whose run
        // covers the cell.
        for cluster in &self.clusters {
            if cluster.width > 0 && cell < cluster.column + cluster.width {
                return cluster.char_index;
            }
        }
        self.char_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The width of each cluster of `text`, in order.
    fn widths(text: &str, tab_width: usize) -> Vec<usize> {
        LineLayout::new(text, tab_width)
            .clusters()
            .iter()
            .map(PlacedCluster::width)
            .collect()
    }

    #[test]
    fn ascii_is_one_cell_per_char() {
        let layout = LineLayout::new("hello", 4);
        assert_eq!(layout.width(), 5);
        assert_eq!(layout.char_count(), 5);
        assert_eq!(widths("hello", 4), vec![1, 1, 1, 1, 1]);
        for column in 0..=5 {
            assert_eq!(layout.column_to_cell(column), column);
        }
    }

    #[test]
    fn a_cjk_character_takes_two_cells_and_shifts_everything_after_it() {
        let layout = LineLayout::new("a漢b", 4);
        assert_eq!(layout.width(), 4);
        assert_eq!(layout.char_count(), 3);
        assert_eq!(layout.column_to_cell(0), 0);
        assert_eq!(layout.column_to_cell(1), 1);
        assert_eq!(layout.column_to_cell(2), 3, "`b` sits after both halves");
        assert_eq!(layout.column_to_cell(3), 4);
    }

    #[test]
    fn an_emoji_takes_two_cells() {
        let layout = LineLayout::new("a\u{1F600}b", 4);
        assert_eq!(layout.width(), 4);
        assert_eq!(layout.column_to_cell(2), 3);
    }

    #[test]
    fn a_zwj_sequence_is_one_cluster_of_two_cells_and_seven_chars() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        let layout = LineLayout::new(&format!("a{family}b"), 4);
        assert_eq!(layout.clusters().len(), 3);
        assert_eq!(layout.char_count(), 9);
        assert_eq!(layout.width(), 4);
        // The `b` is at column 8 — seven chars of family plus the leading `a`.
        assert_eq!(layout.column_to_cell(8), 3);
    }

    #[test]
    fn a_combining_sequence_is_one_cell_and_two_chars() {
        let layout = LineLayout::new("e\u{301}x", 4);
        assert_eq!(layout.width(), 2);
        assert_eq!(layout.char_count(), 3);
        assert_eq!(layout.column_to_cell(0), 0);
        // A column inside the cluster snaps to the cluster's own cell.
        assert_eq!(layout.column_to_cell(1), 0);
        assert_eq!(layout.column_to_cell(2), 1);
    }

    #[test]
    fn tabs_advance_to_the_next_tab_stop() {
        assert_eq!(widths("\t", 4), vec![4]);
        assert_eq!(widths("a\t", 4), vec![1, 3]);
        assert_eq!(widths("abc\t", 4), vec![1, 1, 1, 1]);
        assert_eq!(widths("abcd\t", 4), vec![1, 1, 1, 1, 4]);
        assert_eq!(widths("\t\t", 8), vec![8, 8]);
    }

    #[test]
    fn a_tab_after_a_double_width_glyph_measures_from_the_cell_not_the_char() {
        // `漢` is one char but two cells, so the tab stop is reached a column
        // earlier than a char-counting layout would say.
        assert_eq!(widths("漢\t", 4), vec![2, 2]);
    }

    #[test]
    fn a_zero_tab_width_still_advances() {
        assert_eq!(widths("\t", 0), vec![1]);
    }

    #[test]
    fn zero_width_characters_consume_columns_but_no_cells() {
        let layout = LineLayout::new("a\u{200B}b", 4);
        assert_eq!(layout.width(), 2);
        assert_eq!(layout.char_count(), 3);
        assert_eq!(layout.column_to_cell(1), 1, "the zero-width cluster");
        assert_eq!(layout.column_to_cell(2), 1, "`b` shares its cell");
    }

    #[test]
    fn a_column_past_the_end_lands_one_cell_past_the_line() {
        let layout = LineLayout::new("漢", 4);
        assert_eq!(layout.column_to_cell(1), 2);
        assert_eq!(layout.column_to_cell(99), 2);
    }

    #[test]
    fn the_empty_line_is_zero_wide() {
        let layout = LineLayout::new("", 4);
        assert_eq!(layout.width(), 0);
        assert_eq!(layout.char_count(), 0);
        assert_eq!(layout.column_to_cell(0), 0);
        assert_eq!(layout.cell_to_column(0), 0);
    }

    #[test]
    fn cells_map_back_to_columns() {
        let layout = LineLayout::new("a漢b", 4);
        assert_eq!(layout.cell_to_column(0), 0);
        assert_eq!(layout.cell_to_column(1), 1, "the glyph's leading half");
        assert_eq!(layout.cell_to_column(2), 1, "its continuation half");
        assert_eq!(layout.cell_to_column(3), 2);
        assert_eq!(layout.cell_to_column(4), 3);
        assert_eq!(layout.cell_to_column(99), 3);
    }

    #[test]
    fn every_cell_of_a_tab_maps_to_the_tab() {
        let layout = LineLayout::new("\tx", 4);
        for cell in 0..4 {
            assert_eq!(layout.cell_to_column(cell), 0, "cell {cell} is the tab");
        }
        assert_eq!(layout.cell_to_column(4), 1);
    }

    #[test]
    fn byte_offsets_follow_the_clusters() {
        let layout = LineLayout::new("a漢b", 4);
        let offsets: Vec<usize> = layout
            .clusters()
            .iter()
            .map(PlacedCluster::byte_index)
            .collect();
        assert_eq!(offsets, vec![0, 1, 4]);
        assert_eq!(layout.byte_count(), 5);
    }

    #[test]
    fn a_control_character_occupies_no_cell() {
        let layout = LineLayout::new("a\u{7}b", 4);
        assert_eq!(layout.width(), 2);
        assert!(layout.clusters().get(1).is_some_and(|c| c.width() == 0));
        assert!(
            layout
                .clusters()
                .get(1)
                .is_some_and(|c| c.grapheme().is_none())
        );
    }
}
