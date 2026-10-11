//! Stream only the grapheme prefix needed by the visible cell interval.
use super::LineLayout;
use iridium_editor::Document;
use std::io::{self, Read};
use unicode_segmentation::UnicodeSegmentation as _;

/// Per-layout source budget, bounding even pathological zero-width text or
/// one enormous combining cluster. Budget failures must be shown to the user.
pub const PREFIX_BYTE_BUDGET: usize = 1024 * 1024;
const BLOCK: usize = 4096;

impl LineLayout {
    /// Read a line until `end_cell` is covered. The final grapheme of each
    /// chunk is retained until the following chunk settles its boundary.
    /// This keeps tabs, combining marks, flags and ZWJ sequences correct across
    /// reader boundaries. No whole-line String or cluster vector is built.
    ///
    /// Counts describe the measured prefix, which may end before the line.
    /// A position beyond the prefix maps at or beyond `end_cell`, so it cannot
    /// paint a false caret inside the viewport. Read/UTF-8/budget errors are
    /// returned, never converted into an empty or truncated successful layout.
    pub fn read_prefix(
        mut reader: impl Read,
        byte_len: u64,
        tab_width: usize,
        end_cell: usize,
    ) -> io::Result<Self> {
        let mut layout = Self::default();
        if end_cell == 0 || byte_len == 0 {
            return Ok(layout);
        }
        let mut pending = Vec::new();
        let mut read = 0_u64;
        let mut block = [0; BLOCK];
        loop {
            let budget =
                PREFIX_BYTE_BUDGET.saturating_sub(usize::try_from(read).map_err(io::Error::other)?);
            if budget == 0 {
                return Err(io::Error::other("line layout exceeds 1 MiB scan budget"));
            }
            let wanted = usize::try_from((byte_len - read).min(BLOCK.min(budget) as u64))
                .map_err(io::Error::other)?;
            let count = reader.read(&mut block[..wanted])?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "line reader ended early",
                ));
            }
            read += count as u64;
            pending.extend_from_slice(&block[..count]);
            let eof = read == byte_len;
            let valid = match std::str::from_utf8(&pending) {
                Ok(text) => text,
                Err(error) if error.error_len().is_none() && !eof => {
                    std::str::from_utf8(&pending[..error.valid_up_to()])
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
                },
                Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
            };
            let mut clusters = valid.grapheme_indices(true).peekable();
            let mut used = 0;
            while let Some((offset, cluster)) = clusters.next() {
                if !eof && clusters.peek().is_none() {
                    break;
                }
                layout.push_cluster(cluster, tab_width);
                used = offset + cluster.len();
                if layout.width() >= end_cell {
                    return Ok(layout);
                }
            }
            if eof {
                return Ok(layout);
            }
            pending.drain(..used);
        }
    }
    /// Measure the visible prefix through the kernel's allocation-free reader.
    pub(crate) fn document_prefix(
        document: &Document,
        line: usize,
        tab_width: usize,
        end_cell: usize,
    ) -> io::Result<Self> {
        let range = document.line_byte_range(line).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "line is outside document")
        })?;
        let reader = document.line_reader(line).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "line reader is unavailable")
        })?;
        Self::read_prefix(reader, range.len() as u64, tab_width, end_cell)
    }
}
