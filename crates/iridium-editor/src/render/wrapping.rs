//! Source-preserving soft-wrap boundaries, independent of the drawing backend.
use std::ops::Range;

/// A measured grapheme's end coordinate and whether a word may break after it.
#[derive(Clone, Copy, Debug)]
pub struct Unit {
    /// End coordinate in host layout units, measured from the line start.
    pub end: usize,
    /// Whitespace offers a preferred break without removing any source text.
    pub break_after: bool,
}

/// Partition measured graphemes into source-preserving rows.
///
/// Whitespace is retained, wide
/// graphemes are never split, and overlong words break at grapheme boundaries.
/// A glyph wider than the viewport occupies a row by itself. The final row
/// includes a cell for the end-of-line caret when needed.
#[must_use]
pub fn rows(units: &[Unit], width: usize) -> Vec<Range<usize>> {
    let width = width.max(1);
    let mut result = Vec::new();
    let mut first = 0;
    let mut start = 0;
    while first < units.len() {
        let mut index = first;
        let mut word_end = None;
        while index < units.len() && units[index].end.saturating_sub(start) <= width {
            if units[index].break_after {
                word_end = Some(index + 1);
            }
            index += 1;
        }
        if index == units.len() {
            break;
        }
        let next = word_end
            .filter(|end| *end > first)
            .unwrap_or_else(|| index.max(first + 1));
        let end = units[next - 1].end;
        result.push(start..end);
        start = end;
        first = next;
    }
    let end = units.last().map_or(0, |unit| unit.end);
    if end.saturating_sub(start) >= width {
        result.push(start..end);
        start = end;
    }
    result.push(start..end.saturating_add(1));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn units(text: &str) -> Vec<Unit> {
        text.chars()
            .enumerate()
            .map(|(index, c)| Unit {
                end: index + 1,
                break_after: c.is_whitespace(),
            })
            .collect()
    }
    #[test]
    fn prefers_words_retains_spaces_and_covers_source_once() {
        assert_eq!(
            rows(&units("hello wonderful world"), 8),
            vec![0..6, 6..14, 14..22]
        );
    }
    #[test]
    fn empty_and_exact_width_have_a_caret_cell() {
        assert_eq!(rows(&[], 8), vec![0..1]);
        assert_eq!(rows(&units("abcdefgh"), 8), vec![0..8, 8..9]);
    }
    #[test]
    fn wide_graphemes_never_straddle_a_row_boundary() {
        let units = [
            Unit {
                end: 2,
                break_after: false,
            },
            Unit {
                end: 3,
                break_after: false,
            },
            Unit {
                end: 5,
                break_after: false,
            },
            Unit {
                end: 6,
                break_after: false,
            },
        ];
        assert_eq!(rows(&units, 4), vec![0..3, 3..7]);
        assert_eq!(rows(&units, 1), vec![0..2, 2..3, 3..5, 5..6, 6..7]);
    }
}
