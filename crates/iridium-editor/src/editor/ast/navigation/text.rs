//! Text granularity uses Unicode boundaries and source offsets, never glyph cells.
use crate::document::Document;
use crate::input::keyboard::motions;
use std::ops::Range;
use unicode_segmentation::UnicodeSegmentation;

pub(super) fn character(document: &Document, offset: usize) -> Option<Range<usize>> {
    let start = motions::snap_to_grapheme(document, document.offset_to_position(offset)?);
    let end = motions::char_right(document, start);
    let range = document.position_to_offset(start)?..document.position_to_offset(end)?;
    (!range.is_empty()).then_some(range)
}

pub(super) fn adjacent_character(
    document: &Document,
    range: &Range<usize>,
    forward: bool,
) -> Option<Range<usize>> {
    let offset = if forward {
        range.end
    } else {
        let start = document.offset_to_position(range.start)?;
        document.position_to_offset(motions::char_left(document, start))?
    };
    character(document, offset).filter(|next| next != range)
}

pub(super) fn line(document: &Document, offset: usize) -> Option<Range<usize>> {
    let position = document.offset_to_position(offset)?;
    let start = document.line_to_byte_offset(position.line)?;
    Some(start..start + document.line(position.line)?.len())
}

pub(super) fn adjacent_line(
    document: &Document,
    range: &Range<usize>,
    forward: bool,
) -> Option<Range<usize>> {
    let current = document.offset_to_position(range.start)?.line;
    let next = if forward {
        current.checked_add(1)?
    } else {
        current.checked_sub(1)?
    };
    line(document, document.line_to_byte_offset(next)?)
}

pub(super) fn first_word(document: &Document, range: &Range<usize>) -> Option<Range<usize>> {
    let first = document.offset_to_position(range.start)?.line;
    let last = document.offset_to_position(range.end)?.line;
    for number in first..=last {
        let start = document.line_to_byte_offset(number)?;
        let text = document.line(number)?;
        for (offset, word) in text.unicode_word_indices() {
            let found = start + offset..start + offset + word.len();
            if found.start >= range.start && found.end <= range.end {
                return Some(found);
            }
        }
    }
    None
}

pub(super) fn containing_word(document: &Document, range: &Range<usize>) -> Option<Range<usize>> {
    let bounds = line(document, range.start)?;
    let text = document.line(document.offset_to_position(range.start)?.line)?;
    text.unicode_word_indices()
        .map(|(offset, word)| bounds.start + offset..bounds.start + offset + word.len())
        .find(|word| word.start <= range.start && word.end >= range.end)
}

pub(super) fn adjacent_word(
    document: &Document,
    range: &Range<usize>,
    forward: bool,
) -> Option<Range<usize>> {
    if forward {
        return first_word(document, &(range.end..document.byte_count()));
    }
    let last = document.offset_to_position(range.start)?.line;
    for number in (0..=last).rev() {
        let start = document.line_to_byte_offset(number)?;
        let text = document.line(number)?;
        if let Some(word) = text
            .unicode_word_indices()
            .rev()
            .map(|(offset, word)| start + offset..start + offset + word.len())
            .find(|word| word.end <= range.start)
        {
            return Some(word);
        }
    }
    None
}
