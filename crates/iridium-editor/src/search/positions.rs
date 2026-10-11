//! Convert ordered matcher offsets with two forward scans of the source text.
use crate::document::{Position, Range};

/// Independent streams permit overlapping matches without rescanning prefixes.
pub(super) struct MatchPositions<'a> {
    starts: PositionStream<'a>,
    ends: PositionStream<'a>,
}

impl<'a> MatchPositions<'a> {
    pub(super) const fn new(text: &'a str) -> Self {
        Self {
            starts: PositionStream::new(text),
            ends: PositionStream::new(text),
        }
    }

    pub(super) fn range(&mut self, start: usize, end: usize) -> Result<Range, String> {
        if end < start {
            return Err("Search produced a reversed match range".to_owned());
        }
        Ok(Range::new(self.starts.at(start)?, self.ends.at(end)?))
    }
}

struct PositionStream<'a> {
    text: &'a str,
    offset: usize,
    position: Position,
}

impl<'a> PositionStream<'a> {
    const fn new(text: &'a str) -> Self {
        Self {
            text,
            offset: 0,
            position: Position::zero(),
        }
    }

    fn at(&mut self, offset: usize) -> Result<Position, String> {
        let Some(span) = self.text.get(self.offset..offset) else {
            return Err(
                "Search produced unordered offsets or an invalid UTF-8 boundary".to_owned(),
            );
        };
        for character in span.chars() {
            self.offset += character.len_utf8();
            // LF_CR treats CRLF as one break ending after LF. The position
            // between CR and LF remains on the preceding document line.
            if character == '\n'
                || (character == '\r' && self.text.as_bytes().get(self.offset) != Some(&b'\n'))
            {
                self.position.line += 1;
                self.position.column = 0;
            } else {
                self.position.column += 1;
            }
        }
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests;
