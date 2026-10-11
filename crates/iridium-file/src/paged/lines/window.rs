//! Bounded UTF-8 windows from indexed lines, without allocating full lines.
use super::{Cancellation, LineIndex, PagedText};
use crate::paged::MAX_WINDOW_BYTES;
use std::{io, ops::Range};

/// A bounded portion of one line's content, excluding line-ending bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineWindow {
    /// Exact document byte range of the returned text.
    pub bytes: Range<u64>,
    /// Valid UTF-8; a final partial character is withheld until the next window.
    /// Grapheme segmentation and display-cell widths belong to the renderer.
    pub text: String,
    /// Whether content precedes this window on the same line.
    pub clipped_start: bool,
    /// Whether more content exists, or may exist beyond the indexing frontier.
    pub clipped_end: bool,
}
impl LineIndex {
    /// Read at most `max_bytes` (4 bytes through 1 MiB) from one known line.
    /// `byte_column` counts source bytes, not graphemes or display cells, and
    /// must fall on a UTF-8 boundary. Columns past the known content are refused.
    /// A partial final codepoint is omitted; no whole-line allocation occurs.
    pub fn window(
        &self,
        text: &mut PagedText,
        line: u64,
        byte_column: u64,
        max_bytes: usize,
        cancel: &Cancellation,
    ) -> io::Result<Option<LineWindow>> {
        if !(4..=MAX_WINDOW_BYTES).contains(&(max_bytes as u64)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "line window must be between 4 bytes and 1 MiB",
            ));
        }
        let Some(span) = self.line_span(text, line, cancel)? else {
            return Ok(None);
        };
        let mut body_end = span.bytes.end;
        let tail_start = body_end.saturating_sub(2).max(span.bytes.start);
        let tail = text.read_bytes(tail_start..body_end)?;
        if tail.last() == Some(&b'\n') {
            body_end -= 1;
            if tail.first() == Some(&b'\r') {
                body_end -= 1;
            }
        } else if tail.last() == Some(&b'\r') {
            body_end -= 1;
        }
        let start = span
            .bytes
            .start
            .checked_add(byte_column)
            .filter(|&start| start <= body_end)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "column exceeds known line content",
                )
            })?;
        if !text.boundary(start)? {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "column is inside a UTF-8 character",
            ));
        }
        let end = start.saturating_add(max_bytes as u64).min(body_end);
        cancel.check()?;
        let mut bytes = text.read_bytes(start..end)?;
        match std::str::from_utf8(&bytes) {
            Ok(_) => {},
            Err(error) if error.error_len().is_none() => bytes.truncate(error.valid_up_to()),
            Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        }
        let end = start + bytes.len() as u64;
        let window = LineWindow {
            bytes: start..end,
            text: String::from_utf8(bytes)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
            clipped_start: byte_column > 0,
            clipped_end: end < body_end || !span.complete,
        };
        cancel.check()?;
        Ok(Some(window))
    }
}
