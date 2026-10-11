//! Standard streaming reads over a bounded range of the current piece list.
use super::{PagedText, pieces::Piece, source::BLOCK_BYTES};
use std::{
    io::{self, Read},
    ops::Range,
};

/// Reader borrowing one content state. Its borrow prevents edits while reading.
pub struct PagedReader<'a> {
    text: &'a mut PagedText,
    piece: usize,
    within: u64,
    remaining: u64,
}
impl PagedText {
    /// Stream any byte range without materializing it, including partial UTF-8
    /// characters. Each read performs at most 64 KiB of source I/O. Read errors
    /// propagate; a failed read does not advance the reader's position.
    pub fn reader(&mut self, range: Range<u64>) -> io::Result<PagedReader<'_>> {
        let remaining = self.range_length(&range)?;
        let mut offset = range.start;
        let mut index = 0;
        for piece in &self.pieces {
            if offset < piece.len {
                break;
            }
            offset -= piece.len;
            index += 1;
        }
        Ok(PagedReader {
            text: self,
            piece: index,
            within: offset,
            remaining,
        })
    }
}
impl Read for PagedReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.remaining == 0 {
            return Ok(0);
        }
        let piece = *self
            .text
            .pieces
            .get(self.piece)
            .ok_or_else(|| io::Error::other("reader exhausted source spans"))?;
        let count = usize::try_from(
            (piece.len - self.within)
                .min(self.remaining)
                .min(out.len().min(BLOCK_BYTES) as u64),
        )
        .map_err(io::Error::other)?;
        self.text.source.read(
            Piece {
                offset: piece.offset + self.within,
                len: count as u64,
                ..piece
            },
            &mut out[..count],
        )?;
        self.within += count as u64;
        self.remaining -= count as u64;
        if self.within == piece.len {
            self.piece += 1;
            self.within = 0;
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paged::Cancellation;
    use crate::test_support::TempDir;
    #[test]
    fn reads_cross_piece_and_utf8_boundaries_without_changing_document() {
        let dir = TempDir::new("streamed-line");
        let path = dir.path().join("source");
        std::fs::write(&path, "a🦀z").expect("fixture");
        let mut text =
            PagedText::capture(&path, &Cancellation::default(), |_| {}).expect("capture");
        text.replace(1..1, "insert").expect("edit");
        let mut reader = text.reader(2..10).expect("range");
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).expect("read");
        assert_eq!(bytes, &"ainsert🦀z".as_bytes()[2..10]);
        assert_eq!(text.read(0..text.len()).expect("unchanged"), "ainsert🦀z");
    }
}
