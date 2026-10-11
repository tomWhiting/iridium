use super::{Cancellation, LineIndex, PagedText};
use crate::paged::source::BLOCK_BYTES;
use std::io::{self, Write};

fn document(source: &[u8]) -> (tempfile::NamedTempFile, PagedText) {
    let mut file = tempfile::NamedTempFile::new().expect("fixture");
    file.write_all(source).expect("fixture write");
    let text = PagedText::capture(file.path(), &Cancellation::default(), |_| {}).expect("capture");
    (file, text)
}
fn index(text: &mut PagedText) -> LineIndex {
    let mut index = LineIndex::new(text);
    while !index.progress().complete {
        index
            .advance(text, &Cancellation::default())
            .expect("advance");
    }
    index
}
fn ranges(source: &[u8]) -> Vec<std::ops::Range<u64>> {
    let mut start = 0;
    let mut at = 0;
    let mut ranges = Vec::new();
    while at < source.len() {
        if source[at] == b'\r' || source[at] == b'\n' {
            let size = if source[at..].starts_with(b"\r\n") {
                2
            } else {
                1
            };
            at += size;
            ranges.push(start as u64..at as u64);
            start = at;
        } else {
            at += 1;
        }
    }
    ranges.push(start as u64..source.len() as u64);
    ranges
}

#[test]
fn all_line_endings_and_empty_final_lines_match_source() {
    for source in ["", "\r", "\n", "\r\n", "a\rb\n🦀\r\nc\r\r\n", "one line"] {
        let (_file, mut text) = document(source.as_bytes());
        let index = index(&mut text);
        let expected = ranges(source.as_bytes());
        assert_eq!(index.progress().known_lines, expected.len() as u64);
        for (number, range) in expected.iter().enumerate() {
            let span = index
                .line_span(&mut text, number as u64, &Cancellation::default())
                .expect("span")
                .expect("exists");
            assert_eq!(&span.bytes, range);
            assert!(span.complete);
            let expected_text = &source[usize::try_from(range.start).expect("start")
                ..usize::try_from(range.end).expect("end")];
            let window = index
                .window(&mut text, number as u64, 0, 1024, &Cancellation::default())
                .expect("window")
                .expect("exists");
            assert_eq!(window.text, expected_text.trim_end_matches(['\r', '\n']));
            assert!(!window.clipped_start);
            assert!(!window.clipped_end);
        }
        assert!(
            index
                .line_span(&mut text, expected.len() as u64, &Cancellation::default())
                .expect("absent")
                .is_none()
        );
    }
}

#[test]
fn checkpoint_boundary_resolves_crlf_and_bare_cr() {
    for ending in ["\r\n", "\rx", "\r\r", "\r"] {
        let source = format!("{}{ending}", "x".repeat(BLOCK_BYTES - 1));
        let (_file, mut text) = document(source.as_bytes());
        let mut index = LineIndex::new(&text);
        let progress = index
            .advance(&mut text, &Cancellation::default())
            .expect("block");
        assert_eq!(progress.indexed_bytes, BLOCK_BYTES as u64);
        if !progress.complete {
            assert_eq!(progress.known_lines, 1);
            let first = index
                .window(
                    &mut text,
                    0,
                    (BLOCK_BYTES - 5) as u64,
                    16,
                    &Cancellation::default(),
                )
                .expect("partial")
                .expect("line");
            assert_eq!(first.text, "xxxx");
            assert!(first.clipped_end);
        }
        while !index.progress().complete {
            index
                .advance(&mut text, &Cancellation::default())
                .expect("finish");
        }
        for (number, expected) in ranges(source.as_bytes()).iter().enumerate() {
            assert_eq!(
                index
                    .line_span(&mut text, number as u64, &Cancellation::default())
                    .expect("span")
                    .expect("line")
                    .bytes,
                *expected
            );
        }
    }
}

#[test]
fn long_line_windows_do_not_materialize_or_scan_the_whole_line() {
    let source = format!("{}🦀tail\r\nnext", "a".repeat(BLOCK_BYTES * 33));
    let (_file, mut text) = document(source.as_bytes());
    let index = index(&mut text);
    assert_eq!(index.checkpoint_count(), 35);
    let column = (BLOCK_BYTES * 33) as u64;
    let window = index
        .window(&mut text, 0, column, 4, &Cancellation::default())
        .expect("window")
        .expect("line");
    assert_eq!(window.text, "🦀");
    assert!(window.clipped_start);
    assert!(window.clipped_end);
    assert_eq!(window.bytes, column..column + 4);
    let tail = index
        .window(&mut text, 0, column + 4, 100, &Cancellation::default())
        .expect("window")
        .expect("line");
    assert_eq!(tail.text, "tail");
    assert!(!tail.clipped_end);
}

#[test]
fn incomplete_codepoints_are_withheld_and_interior_columns_refused() {
    let source = format!("{}🦀tail", "a".repeat(BLOCK_BYTES - 2));
    let (_file, mut text) = document(source.as_bytes());
    let mut index = LineIndex::new(&text);
    index
        .advance(&mut text, &Cancellation::default())
        .expect("first block");
    let window = index
        .window(
            &mut text,
            0,
            (BLOCK_BYTES - 4) as u64,
            8,
            &Cancellation::default(),
        )
        .expect("partial")
        .expect("line");
    assert_eq!(window.text, "aa");
    assert!(window.clipped_end);
    index
        .advance(&mut text, &Cancellation::default())
        .expect("finish");
    let window = index
        .window(
            &mut text,
            0,
            (BLOCK_BYTES - 4) as u64,
            4,
            &Cancellation::default(),
        )
        .expect("capped")
        .expect("line");
    assert_eq!(window.text, "aa");
    assert!(
        index
            .window(
                &mut text,
                0,
                (BLOCK_BYTES - 1) as u64,
                4,
                &Cancellation::default()
            )
            .is_err()
    );
    assert!(
        index
            .window(
                &mut text,
                0,
                text_length(&source) + 1,
                4,
                &Cancellation::default()
            )
            .is_err()
    );
    assert!(
        index
            .window(&mut text, 0, 0, 3, &Cancellation::default())
            .is_err()
    );
}
fn text_length(text: &str) -> u64 {
    text.len() as u64
}

#[test]
fn indexes_reject_other_documents_and_edits_but_allow_exact_undo() {
    let (_file, mut text) = document(b"one\ntwo");
    let index = index(&mut text);
    let (_other, mut other) = document(b"one\ntwo");
    assert!(
        index
            .line_span(&mut other, 0, &Cancellation::default())
            .is_err()
    );
    text.replace(0..3, "new").expect("same-size edit");
    assert!(
        index
            .line_span(&mut text, 0, &Cancellation::default())
            .is_err()
    );
    assert!(text.undo().expect("undo"));
    assert_eq!(
        index
            .window(&mut text, 0, 0, 32, &Cancellation::default())
            .expect("restored index")
            .expect("line")
            .text,
        "one"
    );
    text.replace(0..0, "prefix\n").expect("branch edit");
    assert!(
        index
            .line_span(&mut text, 0, &Cancellation::default())
            .is_err()
    );
}

#[test]
fn cancellation_preserves_index_frontier_and_can_resume() {
    let (_file, mut text) = document(&vec![b'\n'; BLOCK_BYTES * 2]);
    let mut index = LineIndex::new(&text);
    let token = Cancellation::default();
    token.cancel();
    let before = index.progress();
    assert_eq!(
        index.advance(&mut text, &token).expect_err("cancel").kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(index.progress(), before);
    index
        .advance(&mut text, &Cancellation::default())
        .expect("resume");
    assert_eq!(index.progress().known_lines, BLOCK_BYTES as u64 + 1);
    assert!(index.line_span(&mut text, 0, &token).is_err());
}

#[test]
fn dense_mixed_newlines_match_oracle_across_sparse_checkpoints() {
    let source = "a\r🦀\n\r\nbbb\r\rxyz\n".repeat(12000);
    let (_file, mut text) = document(source.as_bytes());
    let index = index(&mut text);
    let expected = ranges(source.as_bytes());
    assert_eq!(index.progress().known_lines, expected.len() as u64);
    assert!(index.checkpoint_count() < 10);
    for (number, range) in expected.iter().enumerate().step_by(71) {
        assert_eq!(
            index
                .line_span(&mut text, number as u64, &Cancellation::default())
                .expect("lookup")
                .expect("line")
                .bytes,
            *range
        );
    }
}
