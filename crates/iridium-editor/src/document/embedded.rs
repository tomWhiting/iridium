//! Retained fence markers make language lookup independent of document size.
use iridium_lang::Language;
use ropey::{LineType, Rope};
use std::ops::Range;

use super::{Document, Position};

#[derive(Debug, Clone)]
struct Marker {
    start: usize,
    end: usize,
    delimiter: u8,
    width: usize,
    language: Option<Language>,
    bare: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct FenceIndex {
    markers: Vec<Marker>,
}

impl FenceIndex {
    fn scan(&mut self, rope: &Rope, range: Range<usize>) {
        let first = rope.byte_to_line_idx(range.start, LineType::LF_CR);
        for line in first..rope.len_lines(LineType::LF_CR) {
            let start = rope.line_to_byte_idx(line, LineType::LF_CR);
            if start >= range.end { break; }
            let text = rope.line(line, LineType::LF_CR);
            // Only a short line prefix can contain an opening fence and its label.
            let prefix: String = text.chars().take(256).collect();
            let indent = prefix.bytes().take_while(|&b| b == b' ').count();
            if indent > 3 { continue; }
            let trimmed = &prefix[indent..];
            let Some(delimiter @ (b'`' | b'~')) = trimmed.bytes().next() else { continue; };
            let width = trimmed.bytes().take_while(|&b| b == delimiter).count();
            if width < 3 { continue; }
            let info = trimmed[width..].trim();
            if delimiter == b'`' && info.contains('`') { continue; }
            let label = info.split_whitespace().next().map_or("", |label| label);
            let language = if label.eq_ignore_ascii_case("jsonc") {
                Some(Language::Json)
            } else { Language::from_id(label) };
            self.markers.push(Marker { start, end: start + text.len(), delimiter, width, language, bare: info.is_empty() });
        }
        self.markers.sort_by_key(|marker| marker.start);
    }

    fn language_at(&self, byte: usize) -> Option<Language> {
        let mut opening: Option<&Marker> = None;
        for marker in self.markers.iter().take_while(|marker| marker.start < byte) {
            if let Some(open) = opening {
                if marker.bare && marker.delimiter == open.delimiter && marker.width >= open.width {
                    opening = None;
                }
            } else if byte >= marker.end {
                opening = Some(marker);
            }
        }
        opening.and_then(|marker| marker.language)
    }
}

impl Document {
    /// Resolves the language at a caret, including labelled Markdown fences.
    #[must_use]
    pub fn language_at(&self, position: Position) -> Option<Language> {
        let language = self.language().and_then(Language::from_id)?;
        if language != Language::Markdown { return Some(language); }
        let byte = self.position_to_offset(position)?;
        let index = self.fences.get_or_init(|| {
            let mut index = FenceIndex::default();
            index.scan(self.rope(), 0..self.byte_count());
            index
        });
        index.language_at(byte).or(Some(language))
    }

    pub(super) fn fence_edit_window(&self, start: usize, end: usize) -> Option<Range<usize>> {
        self.fences.get()?;
        let rope = self.rope();
        let first = rope.byte_to_line_idx(start, LineType::LF_CR);
        let last = rope.byte_to_line_idx(end, LineType::LF_CR) + 1;
        let end = if last < self.line_count() { rope.line_to_byte_idx(last, LineType::LF_CR) } else { self.byte_count() };
        Some(rope.line_to_byte_idx(first, LineType::LF_CR)..end)
    }

    pub(super) fn update_fences(&mut self, affected: Option<Range<usize>>, removed: usize, inserted: usize) {
        let Some(range) = affected else { return; };
        let Some(mut index) = self.fences.take() else { return; };
        index.markers.retain(|marker| !range.contains(&marker.start));
        for marker in &mut index.markers {
            if marker.start >= range.end {
                marker.start = marker.start - removed + inserted;
                marker.end = marker.end - removed + inserted;
            }
        }
        index.scan(self.rope(), range.start..range.end - removed + inserted);
        self.fences = std::sync::OnceLock::from(index);
    }
}
