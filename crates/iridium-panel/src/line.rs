//! Assembling one panel row of coloured runs on the character grid.
//!
//! The overlay builders ([`crate::search`], [`crate::command_palette`],
//! [`crate::history_overlay`]) all compose rows the same way: runs pushed
//! left to right against a fixed character budget, sometimes padded out to a
//! column so a right-aligned segment lands on the panel's right edge. This is
//! that arithmetic, once. Everything is counted in `char`s because that is
//! the grid the GPU face draws on — the compositor places glyphs at
//! `char × char_width`, and the prompt module records why at length.

use iridium_editor::theme::{Color, Theme};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::row::Span;

/// One row being assembled against a character budget.
#[derive(Debug)]
pub struct LineBuilder {
    /// The characters the row may hold.
    columns: usize,
    /// The characters used so far.
    used: usize,
    /// The runs so far.
    spans: Vec<Span>,
}

impl LineBuilder {
    /// An empty row `columns` characters wide.
    pub const fn new(columns: usize) -> Self {
        Self {
            columns,
            used: 0,
            spans: Vec::new(),
        }
    }

    /// The characters used so far.
    pub const fn used(&self) -> usize {
        self.used
    }

    /// Pushes a run, truncated to what fits.
    pub fn push(&mut self, text: &str, color: Color) {
        if self.used >= self.columns {
            return;
        }
        let budget = self.columns - self.used;
        let taken: String = text.chars().take(budget).collect();
        if taken.is_empty() {
            return;
        }
        self.used += taken.chars().count();
        self.spans.push(Span::new(taken, color));
    }

    /// Pads with spaces up to `column`, so the next run starts there.
    ///
    /// A column already passed pads nothing; a column past the budget pads to
    /// the budget.
    pub fn pad_to(&mut self, column: usize, color: Color) {
        let target = column.min(self.columns);
        if target > self.used {
            let pad = " ".repeat(target - self.used);
            self.spans.push(Span::new(pad, color));
            self.used = target;
        }
    }

    /// The finished runs.
    pub fn finish(self) -> Vec<Span> {
        self.spans
    }
}

/// `text` split into runs, the matched grapheme clusters recoloured.
///
/// `positions` are the kernel's **character** positions into `text`. Each is
/// mapped to its byte offset and from there to the grapheme cluster that
/// contains it, so a combining sequence or a joined emoji recolours whole —
/// splitting a colour run mid-cluster would hand the shaper half a glyph.
///
/// A position past the end of `text` is dropped rather than clamped, which is
/// what makes this safe to call on a *truncated* string: the caller cuts the
/// text to the width it has and the highlights that fell off the end go with
/// it.
///
/// Shared by every panel that underlines what the user typed. A second copy
/// would be a second answer to "which characters did the matcher mean", and
/// the two would drift the first time either matcher changed.
pub fn highlighted_spans(text: &str, positions: &[u32], base: Color, matched: Color) -> Vec<Span> {
    if positions.is_empty() {
        return vec![Span::new(text, base)];
    }
    let bytes: Vec<usize> = text.char_indices().map(|(byte, _)| byte).collect();
    let matched_bytes: Vec<usize> = positions
        .iter()
        .filter_map(|&position| bytes.get(position as usize).copied())
        .collect();

    let mut spans: Vec<Span> = Vec::new();
    for (start, cluster) in text.grapheme_indices(true) {
        let end = start + cluster.len();
        let is_match = matched_bytes
            .iter()
            .any(|&byte| byte >= start && byte < end);
        let color = if is_match { matched } else { base };
        match spans.last_mut() {
            Some(last) if last.color == color => last.text.push_str(cluster),
            _ => spans.push(Span::new(cluster, color)),
        }
    }
    spans
}

/// The colour a matched character is recoloured with: the theme's current
/// search-match colour at full strength, which is the same statement the
/// terminal face makes with its match background.
pub const fn match_color(theme: &Theme) -> Color {
    let color = theme.editor.search_match_current;
    Color::new(color.r, color.g, color.b, 1.0)
}

/// `text` with its first `count` characters removed — how a scrolled field
/// shows its tail.
pub fn skip_chars(text: &str, count: usize) -> &str {
    match text.char_indices().nth(count) {
        Some((byte, _)) => text.get(byte..).unwrap_or(""),
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::theme::Color;

    use super::{LineBuilder, skip_chars};

    const WHITE: Color = Color::new(1.0, 1.0, 1.0, 1.0);

    /// The builder's text with the colours dropped.
    fn text_of(builder: LineBuilder) -> String {
        builder
            .finish()
            .iter()
            .map(|span| span.text.as_str())
            .collect()
    }

    #[test]
    fn runs_accumulate_and_truncate_at_the_budget() {
        let mut line = LineBuilder::new(6);
        line.push("abcd", WHITE);
        line.push("efgh", WHITE);
        assert_eq!(line.used(), 6);
        assert_eq!(text_of(line), "abcdef");
    }

    #[test]
    fn padding_reaches_the_named_column_and_never_goes_backwards() {
        let mut line = LineBuilder::new(10);
        line.push("ab", WHITE);
        line.pad_to(5, WHITE);
        assert_eq!(line.used(), 5);
        line.pad_to(3, WHITE);
        assert_eq!(line.used(), 5, "a passed column pads nothing");
        line.pad_to(99, WHITE);
        assert_eq!(line.used(), 10, "padding stops at the budget");
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        let mut line = LineBuilder::new(2);
        line.push("héllo", WHITE);
        assert_eq!(text_of(line), "hé");
    }

    #[test]
    fn skipping_characters_is_how_a_field_scrolls() {
        assert_eq!(skip_chars("abcdef", 2), "cdef");
        assert_eq!(skip_chars("héllo", 1), "éllo");
        assert_eq!(skip_chars("ab", 5), "");
        assert_eq!(skip_chars("", 0), "");
    }
}
