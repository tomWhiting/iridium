//! Allocation-free line access through the standard byte-reader contract.
use super::Document;
use std::{
    io::{self, Read},
    ops::Range,
};

impl Document {
    /// Byte range of a line's content, excluding CR, LF or CRLF. No text copy.
    pub fn line_byte_range(&self, line: usize) -> Option<Range<usize>> {
        let start = self.line_to_byte_offset(line)?;
        let mut end = start + self.rope().line(line, ropey::LineType::LF_CR).len();
        if end > start && self.rope().get_byte(end - 1) == Some(b'\n') {
            end -= 1;
        }
        if end > start && self.rope().get_byte(end - 1) == Some(b'\r') {
            end -= 1;
        }
        Some(start..end)
    }
    /// Stream a line without allocating its whole contents. This standard
    /// `Read` interface also accepts native paged sources in host rendering.
    pub fn line_reader(&self, line: usize) -> Option<impl Read + '_> {
        let range = self.line_byte_range(line)?;
        Some(LineReader(self.rope().slice(range).bytes()))
    }
}
struct LineReader<'a>(ropey::iter::Bytes<'a>);
impl Read for LineReader<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let mut written = 0;
        for (slot, byte) in out.iter_mut().zip(self.0.by_ref()) {
            *slot = byte;
            written += 1;
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn line_readers_preserve_content_and_strip_only_line_endings() {
        let doc = Document::new("a🦀\r\n\rxyz\n");
        for (line, expected) in ["a🦀", "", "xyz", ""].into_iter().enumerate() {
            let mut reader = doc.line_reader(line).expect("reader");
            let mut text = String::new();
            reader.read_to_string(&mut text).expect("read");
            assert_eq!(text, expected);
            assert_eq!(doc.line_len(line), Some(expected.chars().count()));
        }
        assert!(doc.line_reader(4).is_none());
    }
}
