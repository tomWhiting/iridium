//! The damage diff: what must be written to turn one buffer into another.
//!
//! Damage is **computed, never declared**. Nothing outside this module may say
//! that a region is dirty, because a caller that forgets to say so leaves a
//! stale cell on screen and a caller that says so too eagerly makes the screen
//! flicker. The only declaration this module admits is [`Damage::full`], which
//! cannot be wrong about a *region* because it names the whole screen; it
//! exists for the moments when the terminal's contents changed underneath us
//! and nothing about the previous frame can be trusted.
//!
//! The output is data, not escape sequences. Turning a run into bytes is the
//! driver's job; keeping the two apart is what makes every rule below testable
//! without a pty.
//!
//! # The coalescing rule
//!
//! One write per changed cell would be absurd: the cursor move between two
//! nearly-adjacent changes costs more than simply writing over the unchanged
//! cells between them. So two damaged spans in the same row are merged into a
//! single run when **both** of these hold:
//!
//! * the gap of unchanged cells between them is at most
//!   [`MAX_COALESCE_GAP`] columns, and
//! * every cell in that gap already carries the same style as the cell
//!   immediately to the gap's left.
//!
//! The first condition bounds the cost of re-writing the gap at four
//! characters, against the four to ten bytes of the cursor move it replaces
//! (`ESC [ n C` is four or five, `ESC [ r ; c H` is seven to ten). The second
//! condition is what keeps that accounting honest: crossing a gap whose style
//! differs would force two extra SGR sequences, ten to forty bytes, and turn
//! the saving into a loss. Any other gap ends the run.
//!
//! Runs never span rows, because moving to the next row is a cursor move
//! whatever else happens.
//!
//! # What is deliberately not modelled
//!
//! Erasing to the end of a line with `EL` is cheaper than writing a row of
//! trailing blanks, but only when the terminal's background-colour-erase
//! behaviour is known. That is a capability question, so it belongs to the
//! driver in step 3, not here.

use super::buffer::{Cell, CellBuffer, CellContent};
use super::color::ColorDepth;
use super::style::Style;

/// The widest run of unchanged cells that is cheaper to overwrite than to skip.
///
/// See the module documentation for the byte accounting behind the number.
pub const MAX_COALESCE_GAP: usize = 4;

/// A stretch of text within a run that shares one style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSegment {
    style: Style,
    text: String,
    columns: usize,
}

impl StyledSegment {
    /// The style every character in this segment is painted with.
    pub const fn style(&self) -> Style {
        self.style
    }

    /// The text to write, already free of control characters.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The number of terminal columns this segment covers.
    ///
    /// This is not the length of the text: a double-width glyph is one cluster
    /// and two columns.
    pub const fn columns(&self) -> usize {
        self.columns
    }
}

/// One cursor move and everything to write from there.
///
/// The segments are contiguous and in left-to-right order, so a driver moves
/// the cursor once, then writes each segment's text after setting its style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DamageRun {
    row: usize,
    column: usize,
    segments: Vec<StyledSegment>,
}

impl DamageRun {
    /// The row this run starts on, counted from the top.
    pub const fn row(&self) -> usize {
        self.row
    }

    /// The column this run starts at, counted from the left.
    ///
    /// This is never the right half of a double-width glyph: starting there
    /// would write half a glyph.
    pub const fn column(&self) -> usize {
        self.column
    }

    /// The styled text to write, left to right.
    pub fn segments(&self) -> &[StyledSegment] {
        &self.segments
    }

    /// The number of columns this run covers in total.
    pub fn columns(&self) -> usize {
        self.segments.iter().map(StyledSegment::columns).sum()
    }

    /// The run's text with its styles dropped. Useful for assertions.
    pub fn text(&self) -> String {
        self.segments.iter().map(StyledSegment::text).collect()
    }
}

/// Everything that must be written to bring the screen up to date.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Damage {
    runs: Vec<DamageRun>,
    requires_clear: bool,
}

impl Damage {
    /// The writes, in top-to-bottom, left-to-right order.
    pub fn runs(&self) -> &[DamageRun] {
        &self.runs
    }

    /// Whether the screen must be cleared before the runs are applied.
    ///
    /// This is set when the previous frame's contents cannot be trusted at
    /// all — after a resize, or after an explicit invalidation — because the
    /// terminal may be showing cells that the new frame does not cover.
    pub const fn requires_clear(&self) -> bool {
        self.requires_clear
    }

    /// Whether there is nothing at all to do.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty() && !self.requires_clear
    }

    /// The writes that turn `front` into `back`.
    ///
    /// `front` is what the terminal is currently showing and `back` is what it
    /// should show. `depth` is the colour depth the terminal can actually
    /// display, and both buffers are compared *after* degrading to it: two
    /// truecolours that collapse onto the same 16-colour entry are not a
    /// visible difference, and repainting them would be waste.
    ///
    /// If the two buffers have different sizes there is no correspondence
    /// between their cells, so the result is a full repaint of `back`.
    pub fn compute(front: &CellBuffer, back: &CellBuffer, depth: ColorDepth) -> Self {
        if front.width() != back.width() || front.height() != back.height() {
            return Self::full(back, depth);
        }
        let mut runs = Vec::new();
        for row in 0..back.height() {
            let (Some(front_row), Some(back_row)) = (front.row(row), back.row(row)) else {
                continue;
            };
            append_row_runs(row, front_row, back_row, depth, &mut runs);
        }
        Self {
            runs,
            requires_clear: false,
        }
    }

    /// A repaint of every cell of `buffer`, preceded by a clear.
    ///
    /// This is the one form of declared damage the module allows, and it can
    /// only declare all of it. Use it when the terminal's contents changed
    /// without going through the buffer — a resize, a resume from suspend,
    /// another process writing to the same terminal — and never as a shortcut
    /// for working out what actually changed.
    pub fn full(buffer: &CellBuffer, depth: ColorDepth) -> Self {
        let mut runs = Vec::new();
        for row in 0..buffer.height() {
            let Some(cells) = buffer.row(row) else {
                continue;
            };
            if let Some(run) = build_run(row, 0, cells.len(), cells, depth) {
                runs.push(run);
            }
        }
        Self {
            runs,
            requires_clear: true,
        }
    }
}

/// Whether two cells would paint differently on a terminal of this depth.
fn cells_differ(front: &Cell, back: &Cell, depth: ColorDepth) -> bool {
    front.content() != back.content() || front.style().degrade(depth) != back.style().degrade(depth)
}

/// Appends the runs for one row.
fn append_row_runs(
    row: usize,
    front: &[Cell],
    back: &[Cell],
    depth: ColorDepth,
    out: &mut Vec<DamageRun>,
) {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for (column, (front_cell, back_cell)) in front.iter().zip(back.iter()).enumerate() {
        if cells_differ(front_cell, back_cell, depth) {
            match &mut current {
                Some((_, end)) => *end = column + 1,
                None => current = Some((column, column + 1)),
            }
        } else if let Some(span) = current.take() {
            spans.push(span);
        }
    }
    if let Some(span) = current.take() {
        spans.push(span);
    }

    // A span must cover whole glyphs. Extending the end past a continuation is
    // load-bearing: a span that stopped between the halves would measure the
    // gap after it from the wrong column and split a run that should not be
    // split. Extending the start backwards is defence in depth — while the
    // buffer's invariants hold, a continuation cannot differ unless the glyph
    // to its left differs too, so a span cannot begin on one.
    for span in &mut spans {
        while span.0 > 0 && back.get(span.0).is_some_and(Cell::is_continuation) {
            span.0 -= 1;
        }
        while span.1 < back.len() && back.get(span.1).is_some_and(Cell::is_continuation) {
            span.1 += 1;
        }
    }

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for span in spans {
        match merged.last_mut() {
            Some(previous) if can_coalesce(back, previous.1, span.0, depth) => {
                previous.1 = previous.1.max(span.1);
            },
            _ => merged.push(span),
        }
    }

    for (start, end) in merged {
        if let Some(run) = build_run(row, start, end, back, depth) {
            out.push(run);
        }
    }
}

/// Whether the gap between two spans is cheaper to write over than to skip.
///
/// See the module documentation for the rule and the reasoning behind it.
fn can_coalesce(back: &[Cell], previous_end: usize, next_start: usize, depth: ColorDepth) -> bool {
    if next_start <= previous_end {
        // The two spans touch or overlap after continuation expansion; there
        // is no gap to pay for.
        return true;
    }
    if next_start - previous_end > MAX_COALESCE_GAP {
        return false;
    }
    let Some(reference) = previous_end
        .checked_sub(1)
        .and_then(|column| back.get(column))
        .map(|cell| cell.style().degrade(depth))
    else {
        return false;
    };
    back.get(previous_end..next_start).is_some_and(|gap| {
        gap.iter()
            .all(|cell| cell.style().degrade(depth) == reference)
    })
}

/// Builds the run covering `start..end` of one row, if there is anything in it.
fn build_run(
    row: usize,
    start: usize,
    end: usize,
    cells: &[Cell],
    depth: ColorDepth,
) -> Option<DamageRun> {
    let span = cells.get(start..end)?;
    let mut segments: Vec<StyledSegment> = Vec::new();
    for cell in span {
        let style = cell.style().degrade(depth);
        match cell.content() {
            CellContent::Continuation => {
                if segments.is_empty() {
                    // Unreachable while the buffer's invariants hold: a
                    // continuation always has its glyph to its left, and the
                    // span was expanded to include it. If a buffer were ever
                    // corrupted, erasing the orphan is the one repair that
                    // cannot leave half a glyph on screen.
                    segments.push(StyledSegment {
                        style,
                        text: String::from(" "),
                        columns: 1,
                    });
                }
                // Otherwise the glyph to the left already covers this column.
            },
            CellContent::Grapheme(grapheme) => {
                if segments.last().is_none_or(|last| last.style != style) {
                    segments.push(StyledSegment {
                        style,
                        text: String::new(),
                        columns: 0,
                    });
                }
                if let Some(last) = segments.last_mut() {
                    grapheme.push_to(&mut last.text);
                    last.columns += grapheme.width();
                }
            },
        }
    }
    if segments.is_empty() {
        return None;
    }
    Some(DamageRun {
        row,
        column: start,
        segments,
    })
}

#[cfg(test)]
mod tests {
    use super::super::color::Color;
    use super::*;

    fn buffer(width: usize, height: usize, text: &str) -> CellBuffer {
        let mut buffer = CellBuffer::new(width, height);
        buffer.set_str(0, 0, text, Style::DEFAULT);
        buffer
    }

    fn styled(index: u8) -> Style {
        Style::DEFAULT.with_foreground(Color::Indexed(index))
    }

    /// Each run as `(row, column, text)`.
    fn summary(damage: &Damage) -> Vec<(usize, usize, String)> {
        damage
            .runs()
            .iter()
            .map(|run| (run.row(), run.column(), run.text()))
            .collect()
    }

    /// Applies the damage to a copy of `front` and checks it becomes `back`.
    ///
    /// This is the property that matters: whatever the runs are, replaying
    /// them must land exactly on the intended screen.
    fn assert_repaints(front: &CellBuffer, back: &CellBuffer, depth: ColorDepth) {
        let damage = Damage::compute(front, back, depth);
        let mut screen = if damage.requires_clear() {
            CellBuffer::new(back.width(), back.height())
        } else {
            front.clone()
        };
        for run in damage.runs() {
            let mut column = run.column();
            for segment in run.segments() {
                column = screen.set_str(column, run.row(), segment.text(), segment.style());
            }
        }
        for row in 0..back.height() {
            let painted = screen.row(row).expect("the row exists");
            let intended = back.row(row).expect("the row exists");
            for (column, (left, right)) in painted.iter().zip(intended.iter()).enumerate() {
                assert!(
                    !cells_differ(left, right, depth),
                    "cell {column},{row} was left as {left:?} but should be {right:?}"
                );
            }
        }
    }

    #[test]
    fn identical_buffers_produce_no_writes() {
        let front = buffer(10, 3, "hello");
        let back = front.clone();
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert!(damage.is_empty());
        assert!(damage.runs().is_empty());
        assert!(!damage.requires_clear());
    }

    #[test]
    fn empty_buffers_produce_no_writes() {
        let front = CellBuffer::new(0, 0);
        let back = CellBuffer::new(0, 0);
        assert!(Damage::compute(&front, &back, ColorDepth::TrueColor).is_empty());
    }

    #[test]
    fn a_single_changed_cell_produces_one_write() {
        let front = buffer(10, 1, "hello");
        let mut back = front.clone();
        back.set_str(2, 0, "L", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 2, String::from("L"))]);
        assert_eq!(damage.runs()[0].columns(), 1);
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_style_change_alone_is_damage() {
        let front = buffer(4, 1, "abcd");
        let mut back = front.clone();
        back.set_style(1, 0, styled(9));
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 1, String::from("b"))]);
        assert_eq!(damage.runs()[0].segments()[0].style(), styled(9));
    }

    #[test]
    fn changes_on_different_rows_are_different_runs() {
        let mut front = CellBuffer::new(4, 3);
        front.set_str(0, 0, "aaaa", Style::DEFAULT);
        front.set_str(0, 1, "bbbb", Style::DEFAULT);
        front.set_str(0, 2, "cccc", Style::DEFAULT);
        let mut back = front.clone();
        back.set_str(1, 0, "X", Style::DEFAULT);
        back.set_str(1, 2, "Y", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(
            summary(&damage),
            vec![(0, 1, String::from("X")), (2, 1, String::from("Y"))]
        );
    }

    #[test]
    fn adjacent_changes_share_one_run() {
        let front = buffer(8, 1, "abcdefgh");
        let mut back = front.clone();
        back.set_str(2, 0, "XY", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 2, String::from("XY"))]);
    }

    #[test]
    fn a_small_gap_of_matching_style_is_written_over() {
        let front = buffer(12, 1, "abcdefghijkl");
        let mut back = front.clone();
        back.set_str(0, 0, "X", Style::DEFAULT);
        // Four unchanged cells, all in the same style as their left neighbour.
        back.set_str(5, 0, "Y", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 0, String::from("XbcdeY"))]);
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_gap_one_cell_too_wide_is_skipped() {
        let front = buffer(12, 1, "abcdefghijkl");
        let mut back = front.clone();
        back.set_str(0, 0, "X", Style::DEFAULT);
        // Five unchanged cells: one more than the rule allows.
        back.set_str(6, 0, "Y", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(
            summary(&damage),
            vec![(0, 0, String::from("X")), (0, 6, String::from("Y"))]
        );
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn the_gap_rule_is_exactly_max_coalesce_gap() {
        for gap in 0..=MAX_COALESCE_GAP + 1 {
            let front = buffer(16, 1, "abcdefghijklmnop");
            let mut back = front.clone();
            back.set_str(0, 0, "X", Style::DEFAULT);
            back.set_str(1 + gap, 0, "Y", Style::DEFAULT);
            let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
            let expected = if gap > MAX_COALESCE_GAP { 2 } else { 1 };
            assert_eq!(
                damage.runs().len(),
                expected,
                "a gap of {gap} produced {} runs",
                damage.runs().len()
            );
        }
    }

    #[test]
    fn a_gap_whose_style_differs_is_never_written_over() {
        let mut front = CellBuffer::new(12, 1);
        front.set_str(0, 0, "abcdefghijkl", Style::DEFAULT);
        // Give the two cells between the changes a style of their own.
        front.set_style(2, 0, styled(5));
        front.set_style(3, 0, styled(5));
        let mut back = front.clone();
        back.set_str(1, 0, "X", Style::DEFAULT);
        back.set_str(4, 0, "Y", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(
            summary(&damage),
            vec![(0, 1, String::from("X")), (0, 4, String::from("Y"))],
            "crossing a differently styled gap costs two SGR changes"
        );
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_run_splits_into_one_segment_per_style() {
        let mut front = CellBuffer::new(6, 1);
        front.set_str(0, 0, "aaaaaa", Style::DEFAULT);
        let mut back = front.clone();
        back.set_str(0, 0, "ab", styled(1));
        back.set_str(2, 0, "cd", styled(2));
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        let run = &damage.runs()[0];
        assert_eq!(run.column(), 0);
        assert_eq!(run.segments().len(), 2);
        assert_eq!(run.segments()[0].text(), "ab");
        assert_eq!(run.segments()[0].style(), styled(1));
        assert_eq!(run.segments()[1].text(), "cd");
        assert_eq!(run.segments()[1].style(), styled(2));
        assert_eq!(run.columns(), 4);
    }

    #[test]
    fn a_double_width_glyph_is_written_as_one_thing() {
        let front = buffer(6, 1, "abcdef");
        let mut back = front.clone();
        back.set_str(1, 0, "漢", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 1, String::from("漢"))]);
        assert_eq!(damage.runs()[0].columns(), 2, "one cluster, two columns");
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_run_never_starts_on_a_continuation_cell() {
        // Only the right-hand half of the glyph differs in the two buffers,
        // which is only possible if the glyph itself differs; the run must
        // still begin at the glyph.
        let mut front = CellBuffer::new(6, 1);
        front.set_str(0, 0, "a漢bc", Style::DEFAULT);
        let mut back = front.clone();
        back.set_str(1, 0, "字", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 1, String::from("字"))]);
        for run in damage.runs() {
            let cell = back.get(run.column(), run.row()).expect("the cell exists");
            assert!(!cell.is_continuation(), "run starts on half a glyph");
        }
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_gap_after_a_glyph_is_measured_from_the_glyph_s_far_side() {
        // The changed glyph covers columns 1 and 2, so the four unchanged
        // cells at 3..7 are a four-column gap, not five. Measuring from the
        // glyph's leading half instead would make the run split in two.
        let mut front = CellBuffer::new(12, 1);
        front.set_str(0, 0, "a漢bcdefghi", Style::DEFAULT);
        let mut back = front.clone();
        back.set_str(1, 0, "字", Style::DEFAULT);
        back.set_str(7, 0, "X", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 1, String::from("字bcdeX"))]);
        assert_eq!(damage.runs()[0].columns(), 7);
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn replacing_a_double_width_glyph_repaints_both_columns() {
        let mut front = CellBuffer::new(6, 1);
        front.set_str(0, 0, "a漢bc", Style::DEFAULT);
        let mut back = front.clone();
        back.set_str(1, 0, "xy", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 1, String::from("xy"))]);
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn overwriting_half_a_glyph_repaints_the_blank_it_leaves_behind() {
        let mut front = CellBuffer::new(6, 1);
        front.set_str(0, 0, "漢bcd", Style::DEFAULT);
        let mut back = front.clone();
        // Writing over the continuation destroys the glyph and blanks its
        // leading half, so column 0 is damaged even though nothing was written
        // to it.
        back.set_str(1, 0, "z", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 0, String::from(" z"))]);
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_run_carries_no_control_characters() {
        let front = CellBuffer::new(8, 1);
        let mut back = CellBuffer::new(8, 1);
        back.set_str(0, 0, "a\tb\nc", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 0, String::from("abc"))]);
        for run in damage.runs() {
            for segment in run.segments() {
                assert!(
                    !segment.text().chars().any(char::is_control),
                    "a control character in the damage output would move the cursor"
                );
            }
        }
    }

    #[test]
    fn combining_marks_travel_with_their_base_character() {
        let front = buffer(6, 1, "abcdef");
        let mut back = front.clone();
        back.set_str(1, 0, "e\u{301}", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 1, String::from("e\u{301}"))]);
        assert_eq!(damage.runs()[0].columns(), 1);
    }

    #[test]
    fn a_zwj_sequence_is_two_columns_of_one_cluster() {
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        let front = buffer(6, 1, "abcdef");
        let mut back = front.clone();
        back.set_str(0, 0, family, Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(summary(&damage), vec![(0, 0, String::from(family))]);
        assert_eq!(damage.runs()[0].columns(), 2);
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_size_change_forces_a_full_repaint() {
        let front = buffer(4, 1, "abcd");
        let mut back = front.clone();
        back.resize(6, 2);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert!(damage.requires_clear());
        assert!(!damage.is_empty());
        assert_eq!(
            summary(&damage),
            vec![
                (0, 0, String::from("abcd  ")),
                (1, 0, String::from("      "))
            ]
        );
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }

    #[test]
    fn a_full_repaint_covers_every_cell() {
        let back = buffer(3, 2, "a漢");
        let damage = Damage::full(&back, ColorDepth::TrueColor);
        assert!(damage.requires_clear());
        assert_eq!(damage.runs().len(), 2);
        for run in damage.runs() {
            assert_eq!(run.column(), 0);
            assert_eq!(run.columns(), 3);
        }
    }

    #[test]
    fn a_colour_change_the_terminal_cannot_show_is_not_damage() {
        let mut front = CellBuffer::new(4, 1);
        front.set_str(
            0,
            0,
            "abcd",
            Style::DEFAULT.with_foreground(Color::rgb(255, 0, 0)),
        );
        let mut back = CellBuffer::new(4, 1);
        // Near enough to red that 16 colours cannot tell them apart.
        back.set_str(
            0,
            0,
            "abcd",
            Style::DEFAULT.with_foreground(Color::rgb(250, 4, 4)),
        );

        assert!(
            !Damage::compute(&front, &back, ColorDepth::TrueColor).is_empty(),
            "a truecolor terminal shows the difference"
        );
        assert!(
            Damage::compute(&front, &back, ColorDepth::Ansi16).is_empty(),
            "a 16-colour terminal cannot, so repainting would be waste"
        );
    }

    #[test]
    fn a_colour_change_the_terminal_can_show_is_damage() {
        let mut front = CellBuffer::new(4, 1);
        front.set_str(
            0,
            0,
            "abcd",
            Style::DEFAULT.with_foreground(Color::rgb(255, 0, 0)),
        );
        let mut back = CellBuffer::new(4, 1);
        back.set_str(
            0,
            0,
            "abcd",
            Style::DEFAULT.with_foreground(Color::rgb(0, 0, 255)),
        );
        let damage = Damage::compute(&front, &back, ColorDepth::Ansi16);
        assert_eq!(summary(&damage), vec![(0, 0, String::from("abcd"))]);
    }

    #[test]
    fn runs_carry_already_degraded_styles() {
        let front = CellBuffer::new(4, 1);
        let mut back = CellBuffer::new(4, 1);
        back.set_str(
            0,
            0,
            "ab",
            Style::DEFAULT.with_foreground(Color::rgb(255, 0, 0)),
        );
        let damage = Damage::compute(&front, &back, ColorDepth::Ansi256);
        assert_eq!(
            damage.runs()[0].segments()[0].style().foreground,
            Color::Indexed(196),
            "the driver must not have to degrade a second time"
        );
    }

    #[test]
    fn degradation_can_merge_two_segments_into_one() {
        let mut front = CellBuffer::new(4, 1);
        front.set_str(0, 0, "aaaa", Style::DEFAULT);
        let mut back = CellBuffer::new(4, 1);
        back.set_str(
            0,
            0,
            "bb",
            Style::DEFAULT.with_foreground(Color::rgb(255, 0, 0)),
        );
        back.set_str(
            2,
            0,
            "cc",
            Style::DEFAULT.with_foreground(Color::rgb(250, 4, 4)),
        );

        let true_color = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(true_color.runs()[0].segments().len(), 2);

        let degraded = Damage::compute(&front, &back, ColorDepth::Ansi16);
        assert_eq!(
            degraded.runs()[0].segments().len(),
            1,
            "one SGR change is enough once the colours collapse"
        );
    }

    #[test]
    fn a_whole_row_of_changes_is_a_single_run() {
        // The spaces inside the sentence are unchanged, but they are one-cell
        // gaps in the same style, so the whole sentence is one run.
        let front = CellBuffer::new(20, 1);
        let mut back = CellBuffer::new(20, 1);
        back.set_str(0, 0, "the quick brown fox", Style::DEFAULT);
        let damage = Damage::compute(&front, &back, ColorDepth::TrueColor);
        assert_eq!(
            summary(&damage),
            vec![(0, 0, String::from("the quick brown fox"))]
        );
        assert_eq!(damage.runs()[0].columns(), 19);
        assert_repaints(&front, &back, ColorDepth::TrueColor);
    }
}
