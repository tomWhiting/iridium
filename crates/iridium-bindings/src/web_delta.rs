//! Pure helpers for the web editor seam's byte-range delta mapping.

#[derive(Debug)]
pub struct TextReplacement {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Maps a pre-edit byte offset through sorted, non-overlapping replacements.
/// Endpoints within a replacement associate with the end of inserted text.
pub fn map_offset(offset: usize, edits: &[TextReplacement]) -> usize {
    let mut inserted_bytes = 0_usize;
    let mut removed_bytes = 0_usize;
    for edit in edits {
        if offset < edit.start {
            break;
        }
        let mapped_start = edit.start + inserted_bytes - removed_bytes;
        if offset <= edit.end {
            return mapped_start + edit.text.len();
        }
        inserted_bytes += edit.text.len();
        removed_bytes += edit.end - edit.start;
    }
    offset + inserted_bytes - removed_bytes
}

#[cfg(test)]
mod tests {
    use super::{TextReplacement, map_offset};

    #[test]
    fn maps_offsets_across_multiple_replacements() {
        let edits = vec![
            TextReplacement {
                start: 2,
                end: 4,
                text: "long".to_owned(),
            },
            TextReplacement {
                start: 8,
                end: 10,
                text: "x".to_owned(),
            },
        ];

        assert_eq!(map_offset(1, &edits), 1);
        assert_eq!(map_offset(3, &edits), 6);
        assert_eq!(map_offset(7, &edits), 9);
        assert_eq!(map_offset(9, &edits), 11);
        assert_eq!(map_offset(12, &edits), 13);
    }

    #[test]
    fn insertion_at_selection_endpoint_moves_endpoint_after_inserted_text() {
        let edits = vec![TextReplacement {
            start: 5,
            end: 5,
            text: "abc".to_owned(),
        }];

        assert_eq!(map_offset(5, &edits), 8);
    }
}
