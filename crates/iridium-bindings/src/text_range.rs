//! Byte-range validation and ranged text extraction from the rope.
//!
//! The edit spans reported by `takeLastEdit` carry byte offsets only. A
//! consumer that needs the bytes of the new span (e.g. an outbound
//! edit-proposal builder) must not materialize the whole document across
//! the wasm boundary per keystroke — that violates the 120fps/sub-8ms
//! budget. This module holds the target-independent math for slicing
//! exactly the requested range: byte-range validation (ordering, bounds,
//! UTF-8 character boundaries) and rope slicing. It is kept free of
//! `wasm-bindgen` so it compiles (and its unit tests run) on every
//! target.

use iridium_editor::document::Document;
use thiserror::Error;

/// Why a byte range was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ByteRangeError {
    /// The range's start is greater than its end.
    #[error("byte range start {start} is greater than end {end}")]
    Inverted {
        /// The offending start offset.
        start: usize,
        /// The offending end offset.
        end: usize,
    },
    /// The range's end lies beyond the end of the document.
    #[error("byte range end {end} is beyond the document length {len}")]
    OutOfBounds {
        /// The offending end offset.
        end: usize,
        /// The document length in bytes.
        len: usize,
    },
    /// An offset does not fall on a UTF-8 character boundary.
    #[error("byte offset {offset} is not on a UTF-8 character boundary")]
    NotCharBoundary {
        /// The offending offset.
        offset: usize,
    },
}

/// Validates that `start..end` is a well-formed byte range in `document`:
/// ordered, within bounds, and with both endpoints on UTF-8 character
/// boundaries.
///
/// Bounds are checked before boundaries, so the rope's boundary query is
/// never asked about an out-of-range offset.
pub fn validate_byte_range(
    document: &Document,
    start: usize,
    end: usize,
) -> Result<(), ByteRangeError> {
    if start > end {
        return Err(ByteRangeError::Inverted { start, end });
    }
    let len = document.byte_count();
    if end > len {
        return Err(ByteRangeError::OutOfBounds { end, len });
    }
    let rope = document.rope();
    for offset in [start, end] {
        if !rope.is_char_boundary(offset) {
            return Err(ByteRangeError::NotCharBoundary { offset });
        }
    }
    Ok(())
}

/// Returns the document text in `start..end` (byte offsets), slicing only
/// the requested range from the rope — the rest of the document is never
/// materialized.
pub fn text_range(document: &Document, start: usize, end: usize) -> Result<String, ByteRangeError> {
    validate_byte_range(document, start, end)?;
    Ok(document.rope().slice(start..end).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(content: &str) -> Document {
        Document::new(content)
    }

    #[test]
    fn slices_ascii_range() {
        let d = doc("hello\nworld");
        assert_eq!(text_range(&d, 0, 5), Ok("hello".to_string()));
        assert_eq!(text_range(&d, 6, 11), Ok("world".to_string()));
        assert_eq!(text_range(&d, 4, 7), Ok("o\nw".to_string()));
    }

    #[test]
    fn slices_full_document_and_empty_range() {
        let d = doc("abc");
        assert_eq!(text_range(&d, 0, 3), Ok("abc".to_string()));
        assert_eq!(text_range(&d, 2, 2), Ok(String::new()));
        assert_eq!(text_range(&d, 3, 3), Ok(String::new()));
    }

    #[test]
    fn slices_empty_document() {
        let d = doc("");
        assert_eq!(text_range(&d, 0, 0), Ok(String::new()));
    }

    #[test]
    fn slices_multibyte_range_on_boundaries() {
        // 'é' is 2 bytes; '日' is 3 bytes.
        let d = doc("aé日b");
        assert_eq!(text_range(&d, 1, 3), Ok("é".to_string()));
        assert_eq!(text_range(&d, 3, 6), Ok("日".to_string()));
        assert_eq!(text_range(&d, 0, 7), Ok("aé日b".to_string()));
    }

    #[test]
    fn rejects_inverted_range() {
        let d = doc("abc");
        assert_eq!(
            text_range(&d, 2, 1),
            Err(ByteRangeError::Inverted { start: 2, end: 1 })
        );
    }

    #[test]
    fn rejects_out_of_bounds_end() {
        let d = doc("abc");
        assert_eq!(
            text_range(&d, 0, 4),
            Err(ByteRangeError::OutOfBounds { end: 4, len: 3 })
        );
        // Start beyond the document is caught as an inverted-or-oob pair:
        // start > end is checked first, so an in-order but oob start also
        // reports through the end check.
        assert_eq!(
            text_range(&d, 4, 5),
            Err(ByteRangeError::OutOfBounds { end: 5, len: 3 })
        );
    }

    #[test]
    fn rejects_mid_character_offsets() {
        // 'é' occupies bytes 1..3; offset 2 splits it.
        let d = doc("aéb");
        assert_eq!(
            text_range(&d, 2, 4),
            Err(ByteRangeError::NotCharBoundary { offset: 2 })
        );
        assert_eq!(
            text_range(&d, 0, 2),
            Err(ByteRangeError::NotCharBoundary { offset: 2 })
        );
    }

    #[test]
    fn error_messages_name_the_offsets() {
        assert_eq!(
            ByteRangeError::Inverted { start: 5, end: 2 }.to_string(),
            "byte range start 5 is greater than end 2"
        );
        assert_eq!(
            ByteRangeError::OutOfBounds { end: 9, len: 3 }.to_string(),
            "byte range end 9 is beyond the document length 3"
        );
        assert_eq!(
            ByteRangeError::NotCharBoundary { offset: 2 }.to_string(),
            "byte offset 2 is not on a UTF-8 character boundary"
        );
    }
}
