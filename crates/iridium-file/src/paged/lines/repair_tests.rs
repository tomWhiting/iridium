use super::{Cancellation, LineIndex, PagedText};
use crate::paged::source::BLOCK_BYTES;
use std::io::Write;

fn fixture(source: &str) -> (tempfile::NamedTempFile, PagedText, LineIndex) {
    let mut file = tempfile::NamedTempFile::new().expect("file");
    file.write_all(source.as_bytes()).expect("write");
    let mut text =
        PagedText::capture(file.path(), &Cancellation::default(), |_| {}).expect("capture");
    let mut index = LineIndex::new(&text);
    finish(&mut index, &mut text);
    (file, text, index)
}
fn finish(index: &mut LineIndex, text: &mut PagedText) -> usize {
    let mut steps = 0;
    while !index.progress().complete {
        index
            .advance(text, &Cancellation::default())
            .expect("advance");
        steps += 1;
    }
    steps
}
fn compare(index: &LineIndex, text: &mut PagedText) {
    let mut fresh = LineIndex::new(text);
    finish(&mut fresh, text);
    assert_eq!(index.progress(), fresh.progress());
    let count = index.progress().known_lines;
    for line in (0..count).step_by(13).chain([count - 1]) {
        let token = Cancellation::default();
        assert_eq!(
            index.line_span(text, line, &token).expect("repaired"),
            fresh.line_span(text, line, &token).expect("fresh")
        );
    }
}

#[test]
fn repair_reuses_suffix_after_edits_undo_and_redo() {
    let source = "hello\r\nworld\r🦀\n".repeat(20000);
    let (_file, mut text, mut index) = fixture(&source);
    text.replace(2..4, "\r\nnew\r").expect("edit");
    index
        .refresh(&text, &Cancellation::default())
        .expect("refresh");
    assert!(finish(&mut index, &mut text) <= 2);
    compare(&index, &mut text);
    assert!(text.undo().expect("undo"));
    index
        .refresh(&text, &Cancellation::default())
        .expect("refresh undo");
    assert!(finish(&mut index, &mut text) <= 2);
    compare(&index, &mut text);
    assert!(text.redo().expect("redo"));
    index
        .refresh(&text, &Cancellation::default())
        .expect("refresh redo");
    assert!(finish(&mut index, &mut text) <= 2);
    compare(&index, &mut text);
}

#[test]
fn repair_handles_crlf_joins_and_empty_files() {
    for source in ["", "\r", "\r\n", "a\r\nb\rc\n", "🦀"] {
        let boundaries: Vec<_> = source
            .char_indices()
            .map(|(at, _)| at)
            .chain([source.len()])
            .collect();
        for &at in &boundaries {
            for insertion in ["\r", "\n", "\r\n", "🦀\r"] {
                let (_file, mut text, mut index) = fixture(source);
                text.replace(at as u64..at as u64, insertion)
                    .expect("insert");
                index
                    .refresh(&text, &Cancellation::default())
                    .expect("refresh");
                finish(&mut index, &mut text);
                compare(&index, &mut text);
                text.replace(0..text.len(), "").expect("erase");
                index
                    .refresh(&text, &Cancellation::default())
                    .expect("refresh erase");
                finish(&mut index, &mut text);
                compare(&index, &mut text);
            }
        }
    }
}

#[test]
fn repair_across_checkpoints_handles_long_lines_and_large_insertions() {
    let source = format!(
        "{}\r\n{}\r\n",
        "a".repeat(BLOCK_BYTES * 4 - 1),
        "b".repeat(BLOCK_BYTES * 4)
    );
    let (_file, mut text, mut index) = fixture(&source);
    let insertion = "x\r\n".repeat(BLOCK_BYTES);
    text.replace(
        (BLOCK_BYTES - 2) as u64..(BLOCK_BYTES * 5) as u64,
        &insertion,
    )
    .expect("replace");
    index
        .refresh(&text, &Cancellation::default())
        .expect("refresh");
    finish(&mut index, &mut text);
    compare(&index, &mut text);
    text.replace(text.len()..text.len(), "\n").expect("append");
    index
        .refresh(&text, &Cancellation::default())
        .expect("refresh append");
    finish(&mut index, &mut text);
    compare(&index, &mut text);
}

#[test]
fn cancelled_refresh_is_atomic_and_missed_changes_are_explicit() {
    let (_file, mut text, mut index) = fixture("before\nafter");
    text.replace(0..0, "new\n").expect("edit");
    let before = index.progress();
    let token = Cancellation::default();
    token.cancel();
    assert!(index.refresh(&text, &token).is_err());
    assert_eq!(index.progress(), before);
    index
        .refresh(&text, &Cancellation::default())
        .expect("refresh");
    let prefix = index.progress();
    assert!(index.advance(&mut text, &token).is_err());
    assert_eq!(index.progress(), prefix);
    finish(&mut index, &mut text);
    compare(&index, &mut text);
    text.replace(0..0, "1").expect("edit");
    text.replace(0..0, "2").expect("edit");
    assert!(index.refresh(&text, &Cancellation::default()).is_err());
}
