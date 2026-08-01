//! The cell grid: what one cell holds, and the buffer that holds them all.
//!
//! # The invariants
//!
//! A [`CellBuffer`] keeps three things true at every observable moment, and
//! every method here exists to keep them true rather than to be convenient:
//!
//! 1. A double-width glyph occupies exactly two adjacent cells of one row: the
//!    grapheme itself, then a [`CellContent::Continuation`].
//! 2. A continuation cell always has a double-width glyph immediately to its
//!    left, and carries that glyph's style.
//! 3. Overwriting either half of a double-width glyph destroys the whole
//!    glyph. Anything else would leave half a glyph on screen, which no
//!    terminal can render and no repaint can repair.
//!
//! There is deliberately no way to obtain `&mut Cell`: a caller with one could
//! break all three.

use unicode_segmentation::UnicodeSegmentation as _;

use super::grapheme::Grapheme;
use super::style::Style;

/// The largest width or height a buffer will allocate.
///
/// Every terminal size protocol — `TIOCGWINSZ`, the `CSI 18 t` reply, the
/// Windows console API — carries dimensions as 16-bit values, so no real
/// terminal can ask for more. Clamping here keeps the cell-count arithmetic
/// total on 32-bit targets as well as 64-bit ones.
pub const MAX_DIMENSION: usize = u16::MAX as usize;

/// What a cell holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CellContent {
    /// A grapheme cluster whose leftmost column is this cell.
    Grapheme(Grapheme),
    /// The right half of a double-width glyph.
    ///
    /// It holds no character of its own and must never be written to the
    /// terminal independently: writing the glyph in the cell to its left
    /// paints both columns.
    Continuation,
}

/// One cell of the grid: what it holds, and how it is painted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    content: CellContent,
    style: Style,
}

impl Cell {
    /// A space in the terminal's own colours.
    pub const BLANK: Self = Self {
        content: CellContent::Grapheme(Grapheme::SPACE),
        style: Style::DEFAULT,
    };

    /// A space in the given style.
    ///
    /// The style matters even though the content does not: a blank cell still
    /// paints its background, and that is how selection highlighting and the
    /// gutter get their colour.
    pub const fn blank(style: Style) -> Self {
        Self {
            content: CellContent::Grapheme(Grapheme::SPACE),
            style,
        }
    }

    /// A cell holding the given grapheme.
    pub const fn new(grapheme: Grapheme, style: Style) -> Self {
        Self {
            content: CellContent::Grapheme(grapheme),
            style,
        }
    }

    /// The right half of a double-width glyph, in the glyph's style.
    pub const fn continuation(style: Style) -> Self {
        Self {
            content: CellContent::Continuation,
            style,
        }
    }

    /// What this cell holds.
    pub const fn content(&self) -> &CellContent {
        &self.content
    }

    /// How this cell is painted.
    pub const fn style(&self) -> Style {
        self.style
    }

    /// Whether this cell is the right half of a double-width glyph.
    pub const fn is_continuation(&self) -> bool {
        matches!(self.content, CellContent::Continuation)
    }

    /// The number of columns the glyph starting at this cell occupies.
    ///
    /// A continuation cell starts no glyph and so occupies none of its own:
    /// its column is already accounted for by the glyph to its left.
    pub fn columns(&self) -> usize {
        match &self.content {
            CellContent::Grapheme(grapheme) => grapheme.width(),
            CellContent::Continuation => 0,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::BLANK
    }
}

/// What happened when a grapheme was written into a buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use = "a write that did not fit changes where the next one goes"]
pub enum WriteOutcome {
    /// The grapheme was written, consuming this many columns: one or two.
    Written(usize),
    /// A double-width glyph was asked to start in the last column of a row,
    /// where it cannot fit.
    ///
    /// A space in the requested style was written there instead, consuming one
    /// column. The glyph itself is dropped: terminals disagree about whether
    /// they wrap such a glyph to the next line, truncate it or overwrite the
    /// previous cell, so emitting it would make the buffer's model of the
    /// screen wrong on some terminals. Callers that must show the glyph should
    /// have laid it out one column earlier.
    Truncated,
    /// The position was outside the buffer. Nothing was written.
    OutOfBounds,
}

impl WriteOutcome {
    /// The number of columns consumed by the write.
    pub const fn columns(self) -> usize {
        match self {
            Self::Written(columns) => columns,
            Self::Truncated => 1,
            Self::OutOfBounds => 0,
        }
    }
}

/// A grid of cells, in row-major order.
///
/// A buffer may legitimately be empty in either dimension: a terminal can
/// report a zero size while it is being resized, and every operation on an
/// empty buffer is a well-defined no-op rather than a panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellBuffer {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
}

impl CellBuffer {
    /// A buffer of blank cells.
    ///
    /// Both dimensions are clamped to [`MAX_DIMENSION`].
    pub fn new(width: usize, height: usize) -> Self {
        let width = width.min(MAX_DIMENSION);
        let height = height.min(MAX_DIMENSION);
        Self {
            width,
            height,
            cells: vec![Cell::BLANK; width * height],
        }
    }

    /// The number of columns.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// The number of rows.
    pub const fn height(&self) -> usize {
        self.height
    }

    /// The cell at a position, or `None` if the position is outside the grid.
    pub fn get(&self, column: usize, row: usize) -> Option<&Cell> {
        self.cells.get(self.index(column, row)?)
    }

    /// A whole row, or `None` if there is no such row.
    pub fn row(&self, row: usize) -> Option<&[Cell]> {
        if row >= self.height {
            return None;
        }
        let start = row * self.width;
        self.cells.get(start..start + self.width)
    }

    /// Replaces every cell with a blank in the given style.
    pub fn fill(&mut self, style: Style) {
        let blank = Cell::blank(style);
        for cell in &mut self.cells {
            cell.clone_from(&blank);
        }
    }

    /// Replaces every cell with a blank in the terminal's own colours.
    pub fn clear(&mut self) {
        self.fill(Style::DEFAULT);
    }

    /// Writes one grapheme at a position.
    ///
    /// Whatever was there is destroyed, including the other half of a
    /// double-width glyph that this write lands on.
    pub fn set_grapheme(
        &mut self,
        column: usize,
        row: usize,
        grapheme: &Grapheme,
        style: Style,
    ) -> WriteOutcome {
        let Some(index) = self.index(column, row) else {
            return WriteOutcome::OutOfBounds;
        };

        if grapheme.is_double_width() {
            if column + 1 >= self.width {
                self.detach(column, row);
                self.write(index, Cell::blank(style));
                return WriteOutcome::Truncated;
            }
            self.detach(column, row);
            self.detach(column + 1, row);
            self.write(index, Cell::new(grapheme.clone(), style));
            self.write(index + 1, Cell::continuation(style));
            return WriteOutcome::Written(2);
        }

        self.detach(column, row);
        self.write(index, Cell::new(grapheme.clone(), style));
        WriteOutcome::Written(1)
    }

    /// Writes text from a position rightwards, one grapheme cluster per cell.
    ///
    /// Returns the column immediately after the last one written, which is
    /// where the next write should start. Text that would run past the end of
    /// the row is dropped: this buffer does not wrap, because wrapping is a
    /// layout decision and the kernel already owns layout.
    ///
    /// Clusters that cannot be painted — control characters, zero-width
    /// characters — consume no column and are skipped; see [`Grapheme::new`].
    pub fn set_str(&mut self, column: usize, row: usize, text: &str, style: Style) -> usize {
        let mut column = column;
        if row >= self.height {
            return column.min(self.width);
        }
        for cluster in text.graphemes(true) {
            if column >= self.width {
                break;
            }
            let Some(grapheme) = Grapheme::new(cluster) else {
                continue;
            };
            match self.set_grapheme(column, row, &grapheme, style) {
                WriteOutcome::Written(columns) => column += columns,
                WriteOutcome::Truncated => column += 1,
                WriteOutcome::OutOfBounds => break,
            }
        }
        column.min(self.width)
    }

    /// Restyles the glyph at a position, leaving its content alone.
    ///
    /// Both halves of a double-width glyph are restyled together, whichever
    /// half the position names, because a glyph is painted with one style.
    /// Returns `false` if the position is outside the grid.
    pub fn set_style(&mut self, column: usize, row: usize, style: Style) -> bool {
        let Some(index) = self.index(column, row) else {
            return false;
        };
        let is_continuation = self.cells.get(index).is_some_and(Cell::is_continuation);
        if is_continuation && column > 0 {
            if let Some(cell) = self.cells.get_mut(index - 1) {
                cell.style = style;
            }
        }
        let is_wide = self
            .cells
            .get(index)
            .is_some_and(|cell| cell.columns() == 2);
        if is_wide && column + 1 < self.width {
            if let Some(cell) = self.cells.get_mut(index + 1) {
                cell.style = style;
            }
        }
        if let Some(cell) = self.cells.get_mut(index) {
            cell.style = style;
        }
        true
    }

    /// Changes the size of the grid, keeping the content that still fits.
    ///
    /// New cells are blank. A double-width glyph whose second half falls
    /// outside the new width is replaced by a blank in its own style: leaving
    /// its leading half behind would be exactly the half-glyph the invariants
    /// exist to prevent.
    ///
    /// Both dimensions are clamped to [`MAX_DIMENSION`].
    pub fn resize(&mut self, width: usize, height: usize) {
        let width = width.min(MAX_DIMENSION);
        let height = height.min(MAX_DIMENSION);
        if width == self.width && height == self.height {
            return;
        }

        let mut cells = vec![Cell::BLANK; width * height];
        if width > 0 && self.width > 0 {
            let columns = width.min(self.width);
            let rows = height.min(self.height);
            let destination = cells.chunks_mut(width);
            let source = self.cells.chunks(self.width);
            for (destination_row, source_row) in destination.zip(source).take(rows) {
                for (destination_cell, source_cell) in destination_row
                    .iter_mut()
                    .zip(source_row.iter())
                    .take(columns)
                {
                    destination_cell.clone_from(source_cell);
                }
            }
        }

        self.cells = cells;
        self.width = width;
        self.height = height;
        self.repair();
    }

    /// The index of a position, or `None` if it is outside the grid.
    const fn index(&self, column: usize, row: usize) -> Option<usize> {
        if column >= self.width || row >= self.height {
            return None;
        }
        Some(row * self.width + column)
    }

    /// Puts a cell at an index that is already known to be inside the grid.
    fn write(&mut self, index: usize, cell: Cell) {
        if let Some(slot) = self.cells.get_mut(index) {
            *slot = cell;
        }
    }

    /// Destroys any double-width glyph that covers a position, so that the
    /// position can be written to.
    ///
    /// The surviving half becomes a blank in the destroyed glyph's own style,
    /// which keeps the background — a selection, the current-line highlight —
    /// looking the way it did.
    fn detach(&mut self, column: usize, row: usize) {
        let Some(index) = self.index(column, row) else {
            return;
        };
        if self.cells.get(index).is_some_and(Cell::is_continuation) && column > 0 {
            if let Some(lead) = self.cells.get_mut(index - 1) {
                let style = lead.style;
                *lead = Cell::blank(style);
            }
        }
        if self
            .cells
            .get(index)
            .is_some_and(|cell| cell.columns() == 2)
            && column + 1 < self.width
        {
            if let Some(trail) = self.cells.get_mut(index + 1) {
                let style = trail.style;
                *trail = Cell::blank(style);
            }
        }
    }

    /// Restores the double-width invariants across the whole grid.
    ///
    /// Only [`CellBuffer::resize`] can break them, and only at the right-hand
    /// edge, but the sweep is a whole row either way and being exhaustive
    /// costs nothing.
    fn repair(&mut self) {
        if self.width == 0 {
            return;
        }
        let width = self.width;
        for row in self.cells.chunks_mut(width) {
            let mut column = 0;
            while column < width {
                match row.get(column).map_or(0, Cell::columns) {
                    // A continuation reached here rather than being consumed
                    // by the glyph before it, so it has no glyph: erase it.
                    0 => {
                        if let Some(cell) = row.get_mut(column) {
                            let style = cell.style;
                            *cell = Cell::blank(style);
                        }
                        column += 1;
                    },
                    2 => {
                        if row.get(column + 1).is_some_and(Cell::is_continuation) {
                            column += 2;
                        } else {
                            if let Some(cell) = row.get_mut(column) {
                                let style = cell.style;
                                *cell = Cell::blank(style);
                            }
                            column += 1;
                        }
                    },
                    _ => column += 1,
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::color::Color;
    use super::*;

    /// The text of a row, with a marker for each continuation cell.
    fn row_text(buffer: &CellBuffer, row: usize) -> String {
        let mut text = String::new();
        for cell in buffer.row(row).expect("the row exists") {
            match cell.content() {
                CellContent::Grapheme(grapheme) => grapheme.push_to(&mut text),
                CellContent::Continuation => text.push('~'),
            }
        }
        text
    }

    fn styled(color: Color) -> Style {
        Style::DEFAULT.with_foreground(color)
    }

    /// Every invariant the buffer promises, checked exhaustively.
    fn assert_invariants(buffer: &CellBuffer) {
        for row in 0..buffer.height() {
            let cells = buffer.row(row).expect("the row exists");
            assert_eq!(cells.len(), buffer.width());
            for (column, cell) in cells.iter().enumerate() {
                if cell.is_continuation() {
                    let lead = column
                        .checked_sub(1)
                        .and_then(|previous| cells.get(previous))
                        .expect("a continuation cell must have a cell to its left");
                    assert_eq!(
                        lead.columns(),
                        2,
                        "continuation at {column},{row} has no double-width glyph to its left"
                    );
                    assert_eq!(
                        lead.style(),
                        cell.style(),
                        "the halves of the glyph at {column},{row} disagree about style"
                    );
                }
                if cell.columns() == 2 {
                    let trail = cells
                        .get(column + 1)
                        .expect("a double-width glyph must have a cell to its right");
                    assert!(
                        trail.is_continuation(),
                        "double-width glyph at {column},{row} has no continuation"
                    );
                }
            }
        }
    }

    #[test]
    fn a_new_buffer_is_blank() {
        let buffer = CellBuffer::new(4, 2);
        assert_eq!(buffer.width(), 4);
        assert_eq!(buffer.height(), 2);
        assert_eq!(row_text(&buffer, 0), "    ");
        assert_eq!(row_text(&buffer, 1), "    ");
        assert_invariants(&buffer);
    }

    #[test]
    fn an_empty_buffer_is_usable() {
        let mut buffer = CellBuffer::new(0, 0);
        assert_eq!(buffer.row(0), None);
        assert_eq!(buffer.get(0, 0), None);
        assert_eq!(buffer.set_str(0, 0, "hello", Style::DEFAULT), 0);
        buffer.clear();
        buffer.resize(0, 4);
        assert_eq!(buffer.width(), 0);
        assert_eq!(buffer.height(), 4);
        assert_eq!(buffer.row(0), Some(&[][..]));
    }

    #[test]
    fn writing_ascii_fills_one_cell_each() {
        let mut buffer = CellBuffer::new(6, 1);
        assert_eq!(buffer.set_str(0, 0, "abc", Style::DEFAULT), 3);
        assert_eq!(row_text(&buffer, 0), "abc   ");
        assert_invariants(&buffer);
    }

    #[test]
    fn writing_past_the_end_of_a_row_drops_the_rest() {
        let mut buffer = CellBuffer::new(3, 1);
        assert_eq!(buffer.set_str(0, 0, "abcdef", Style::DEFAULT), 3);
        assert_eq!(row_text(&buffer, 0), "abc");
        assert_invariants(&buffer);
    }

    #[test]
    fn writing_outside_the_grid_does_nothing() {
        let mut buffer = CellBuffer::new(2, 1);
        let glyph = Grapheme::new("a").unwrap();
        assert_eq!(
            buffer.set_grapheme(2, 0, &glyph, Style::DEFAULT),
            WriteOutcome::OutOfBounds
        );
        assert_eq!(
            buffer.set_grapheme(0, 1, &glyph, Style::DEFAULT),
            WriteOutcome::OutOfBounds
        );
        assert_eq!(row_text(&buffer, 0), "  ");
    }

    #[test]
    fn a_double_width_glyph_takes_two_cells() {
        let mut buffer = CellBuffer::new(4, 1);
        assert_eq!(buffer.set_str(0, 0, "漢", Style::DEFAULT), 2);
        assert_eq!(row_text(&buffer, 0), "漢~  ");
        assert!(buffer.get(1, 0).expect("cell 1").is_continuation());
        assert_eq!(buffer.get(0, 0).expect("cell 0").columns(), 2);
        assert_eq!(buffer.get(1, 0).expect("cell 1").columns(), 0);
        assert_invariants(&buffer);
    }

    #[test]
    fn the_halves_of_a_double_width_glyph_share_a_style() {
        let mut buffer = CellBuffer::new(4, 1);
        let style = styled(Color::Indexed(3));
        buffer.set_str(0, 0, "漢", style);
        assert_eq!(buffer.get(0, 0).expect("cell 0").style(), style);
        assert_eq!(buffer.get(1, 0).expect("cell 1").style(), style);
    }

    #[test]
    fn overwriting_the_leading_half_clears_the_continuation() {
        let mut buffer = CellBuffer::new(4, 1);
        buffer.set_str(0, 0, "漢", Style::DEFAULT);
        buffer.set_str(0, 0, "x", Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), "x   ");
        assert_invariants(&buffer);
    }

    #[test]
    fn overwriting_the_continuation_clears_the_leading_half() {
        let mut buffer = CellBuffer::new(4, 1);
        buffer.set_str(0, 0, "漢", Style::DEFAULT);
        buffer.set_str(1, 0, "x", Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), " x  ");
        assert_invariants(&buffer);
    }

    #[test]
    fn a_destroyed_half_keeps_its_background() {
        let mut buffer = CellBuffer::new(4, 1);
        let highlighted = Style::DEFAULT.with_background(Color::Indexed(4));
        buffer.set_str(0, 0, "漢", highlighted);
        buffer.set_str(1, 0, "x", Style::DEFAULT);
        assert_eq!(
            buffer.get(0, 0).expect("cell 0").style(),
            highlighted,
            "the surviving blank must keep the destroyed glyph's background"
        );
    }

    #[test]
    fn a_double_width_glyph_overwriting_another_clears_both_neighbours() {
        let mut buffer = CellBuffer::new(6, 1);
        buffer.set_str(0, 0, "漢字", Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), "漢~字~  ");
        // Landing on the continuation of the first and the lead of the second:
        // both glyphs must go, all four cells accounted for.
        buffer.set_str(1, 0, "本", Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), " 本~   ");
        assert_invariants(&buffer);
    }

    #[test]
    fn a_double_width_glyph_in_the_last_column_is_refused() {
        let mut buffer = CellBuffer::new(3, 1);
        let style = styled(Color::Indexed(5));
        assert_eq!(buffer.set_str(0, 0, "aa漢", style), 3);
        assert_eq!(row_text(&buffer, 0), "aa ");
        assert_eq!(
            buffer.get(2, 0).expect("cell 2").style(),
            style,
            "the substituted blank takes the style that was asked for"
        );
        assert_invariants(&buffer);
    }

    #[test]
    fn a_refused_glyph_reports_truncation() {
        let mut buffer = CellBuffer::new(2, 1);
        let glyph = Grapheme::new("漢").unwrap();
        assert_eq!(
            buffer.set_grapheme(1, 0, &glyph, Style::DEFAULT),
            WriteOutcome::Truncated
        );
        assert_eq!(WriteOutcome::Truncated.columns(), 1);
        assert_eq!(WriteOutcome::Written(2).columns(), 2);
        assert_eq!(WriteOutcome::OutOfBounds.columns(), 0);
    }

    #[test]
    fn refusing_a_glyph_still_destroys_what_it_lands_on() {
        let mut buffer = CellBuffer::new(3, 1);
        buffer.set_str(1, 0, "漢", Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), " 漢~");
        // The refused glyph lands on the continuation of the one already there.
        let glyph = Grapheme::new("字").unwrap();
        assert_eq!(
            buffer.set_grapheme(2, 0, &glyph, Style::DEFAULT),
            WriteOutcome::Truncated
        );
        assert_eq!(row_text(&buffer, 0), "   ");
        assert_invariants(&buffer);
    }

    #[test]
    fn combining_marks_stay_with_their_base_character() {
        let mut buffer = CellBuffer::new(4, 1);
        assert_eq!(buffer.set_str(0, 0, "e\u{301}x", Style::DEFAULT), 2);
        assert_eq!(row_text(&buffer, 0), "e\u{301}x  ");
        assert_eq!(buffer.get(0, 0).expect("cell 0").columns(), 1);
        assert_invariants(&buffer);
    }

    #[test]
    fn a_zwj_sequence_occupies_two_cells() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        let mut buffer = CellBuffer::new(4, 1);
        assert_eq!(buffer.set_str(0, 0, family, Style::DEFAULT), 2);
        assert_eq!(row_text(&buffer, 0), format!("{family}~  "));
        assert_invariants(&buffer);
    }

    #[test]
    fn zero_width_and_control_characters_consume_no_cell() {
        let mut buffer = CellBuffer::new(4, 1);
        assert_eq!(buffer.set_str(0, 0, "a\u{200B}\tb\nc", Style::DEFAULT), 3);
        assert_eq!(row_text(&buffer, 0), "abc ");
        assert_invariants(&buffer);
    }

    #[test]
    fn filling_replaces_everything_with_styled_blanks() {
        let mut buffer = CellBuffer::new(3, 2);
        buffer.set_str(0, 0, "漢a", Style::DEFAULT);
        let style = styled(Color::Indexed(7));
        buffer.fill(style);
        assert_eq!(row_text(&buffer, 0), "   ");
        assert_eq!(buffer.get(0, 0).expect("cell 0").style(), style);
        buffer.clear();
        assert_eq!(buffer.get(0, 0).expect("cell 0").style(), Style::DEFAULT);
        assert_invariants(&buffer);
    }

    #[test]
    fn restyling_covers_both_halves_of_a_glyph() {
        let mut buffer = CellBuffer::new(4, 1);
        buffer.set_str(0, 0, "漢", Style::DEFAULT);
        let style = styled(Color::Indexed(2));

        assert!(buffer.set_style(1, 0, style), "the continuation half");
        assert_eq!(buffer.get(0, 0).expect("cell 0").style(), style);
        assert_eq!(buffer.get(1, 0).expect("cell 1").style(), style);

        let other = styled(Color::Indexed(6));
        assert!(buffer.set_style(0, 0, other), "the leading half");
        assert_eq!(buffer.get(0, 0).expect("cell 0").style(), other);
        assert_eq!(buffer.get(1, 0).expect("cell 1").style(), other);

        assert!(!buffer.set_style(9, 0, other));
        assert_eq!(row_text(&buffer, 0), "漢~  ");
        assert_invariants(&buffer);
    }

    #[test]
    fn growing_keeps_the_content_and_blanks_the_rest() {
        let mut buffer = CellBuffer::new(3, 1);
        buffer.set_str(0, 0, "ab", Style::DEFAULT);
        buffer.resize(5, 3);
        assert_eq!(buffer.width(), 5);
        assert_eq!(buffer.height(), 3);
        assert_eq!(row_text(&buffer, 0), "ab   ");
        assert_eq!(row_text(&buffer, 1), "     ");
        assert_eq!(row_text(&buffer, 2), "     ");
        assert_invariants(&buffer);
    }

    #[test]
    fn shrinking_keeps_what_still_fits() {
        let mut buffer = CellBuffer::new(6, 3);
        buffer.set_str(0, 0, "abcdef", Style::DEFAULT);
        buffer.set_str(0, 1, "ghijkl", Style::DEFAULT);
        buffer.set_str(0, 2, "mnopqr", Style::DEFAULT);
        buffer.resize(3, 2);
        assert_eq!(buffer.height(), 2);
        assert_eq!(row_text(&buffer, 0), "abc");
        assert_eq!(row_text(&buffer, 1), "ghi");
        assert_eq!(buffer.row(2), None);
        assert_invariants(&buffer);
    }

    #[test]
    fn shrinking_through_a_double_width_glyph_removes_the_whole_glyph() {
        let mut buffer = CellBuffer::new(6, 1);
        let style = styled(Color::Indexed(1));
        buffer.set_str(0, 0, "a漢b", style);
        assert_eq!(row_text(&buffer, 0), "a漢~b  ");
        // Column 2 is the continuation; cutting there orphans the leading half.
        buffer.resize(2, 1);
        assert_eq!(row_text(&buffer, 0), "a ");
        assert_eq!(
            buffer.get(1, 0).expect("cell 1").style(),
            style,
            "the blank keeps the destroyed glyph's style"
        );
        assert_invariants(&buffer);
    }

    #[test]
    fn shrinking_that_keeps_a_whole_glyph_keeps_it() {
        let mut buffer = CellBuffer::new(6, 1);
        buffer.set_str(0, 0, "a漢b", Style::DEFAULT);
        buffer.resize(3, 1);
        assert_eq!(row_text(&buffer, 0), "a漢~");
        assert_invariants(&buffer);
    }

    #[test]
    fn resizing_to_the_same_size_changes_nothing() {
        let mut buffer = CellBuffer::new(4, 2);
        buffer.set_str(0, 0, "漢b", Style::DEFAULT);
        let before = buffer.clone();
        buffer.resize(4, 2);
        assert_eq!(buffer, before);
    }

    #[test]
    fn resizing_to_zero_and_back_is_survivable() {
        let mut buffer = CellBuffer::new(4, 2);
        buffer.set_str(0, 0, "漢b", Style::DEFAULT);
        buffer.resize(0, 0);
        assert_eq!(buffer.width(), 0);
        assert_eq!(buffer.height(), 0);
        buffer.resize(4, 2);
        assert_eq!(row_text(&buffer, 0), "    ");
        assert_invariants(&buffer);
    }

    #[test]
    fn dimensions_are_clamped() {
        let buffer = CellBuffer::new(usize::MAX, 0);
        assert_eq!(buffer.width(), MAX_DIMENSION);
        assert_eq!(buffer.height(), 0);
    }

    #[test]
    fn writing_the_second_row_does_not_touch_the_first() {
        let mut buffer = CellBuffer::new(4, 2);
        buffer.set_str(0, 0, "abcd", Style::DEFAULT);
        buffer.set_str(0, 1, "wxyz", Style::DEFAULT);
        assert_eq!(row_text(&buffer, 0), "abcd");
        assert_eq!(row_text(&buffer, 1), "wxyz");
    }

    #[test]
    fn a_glyph_never_bleeds_into_the_next_row() {
        // The last cell of row 0 and the first of row 1 are adjacent in
        // memory; a double-width write must not treat them as adjacent on
        // screen.
        let mut buffer = CellBuffer::new(2, 2);
        assert_eq!(buffer.set_str(1, 0, "漢", Style::DEFAULT), 2);
        assert_eq!(row_text(&buffer, 0), "  ");
        assert_eq!(row_text(&buffer, 1), "  ");
        assert_invariants(&buffer);
    }
}
