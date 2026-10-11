use super::{Cancellation, MAX_WINDOW_BYTES, PagedText, source::BLOCK_BYTES};
use std::io::{self, Write};

fn document(bytes: &[u8]) -> (tempfile::NamedTempFile, PagedText) {
    let mut file = tempfile::NamedTempFile::new().expect("fixture");
    file.write_all(bytes).expect("fixture write");
    let text = PagedText::capture(file.path(), &Cancellation::default(), |_| {}).expect("capture");
    (file, text)
}
fn output(text: &mut PagedText) -> Vec<u8> {
    let mut bytes = Vec::new();
    text.write_to(&mut bytes, &Cancellation::default())
        .expect("stream");
    bytes
}

#[test]
fn utf8_block_boundaries_and_original_bytes_survive() {
    let source = format!("\u{feff}{}🦀e\u{301}\r\n", "a".repeat(BLOCK_BYTES - 4));
    let (_file, mut text) = document(source.as_bytes());
    assert_eq!(text.read(0..text.len()).expect("window"), source);
    assert_eq!(output(&mut text), source.as_bytes());
    assert_eq!(text.piece_count(), 1);
}

#[test]
fn invalid_and_truncated_utf8_report_absolute_offsets() {
    for tail in [vec![0xf0, 0x9f], vec![0xf0, 0x9f, 0xff]] {
        let mut file = tempfile::NamedTempFile::new().expect("fixture");
        file.write_all(&vec![b'a'; BLOCK_BYTES - 1])
            .expect("prefix");
        file.write_all(&tail).expect("tail");
        let result = PagedText::capture(file.path(), &Cancellation::default(), |_| {});
        let error = result.err().expect("invalid UTF8 must fail");
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(error.to_string().contains(&(BLOCK_BYTES - 1).to_string()));
    }
}

#[test]
fn replacement_undo_redo_and_save_identity_are_exact() {
    let (_file, mut text) = document("first\r\n🦀last".as_bytes());
    let original = output(&mut text);
    text.replace(7..11, "e\u{301}").expect("replace crab");
    text.mark_saved();
    text.replace(0..0, "before").expect("prepend");
    text.replace(text.len()..text.len(), "after")
        .expect("append");
    let final_bytes = output(&mut text);
    assert!(!text.is_at_save_point());
    assert!(text.undo().expect("undo append"));
    assert!(text.undo().expect("undo prepend"));
    assert!(text.is_at_save_point());
    assert!(text.undo().expect("undo replacement"));
    assert_eq!(output(&mut text), original);
    assert!(!text.is_at_save_point());
    assert!(!text.undo().expect("history exhausted"));
    for _ in 0..3 {
        assert!(text.redo().expect("redo"));
    }
    assert_eq!(output(&mut text), final_bytes);
    assert!(!text.redo().expect("redo exhausted"));
    text.replace(0..text.len(), "").expect("delete all");
    assert!(text.is_empty());
    assert_eq!(text.piece_count(), 0);
    assert!(text.undo().expect("restore all"));
    assert_eq!(output(&mut text), final_bytes);
}

#[test]
fn empty_document_insert_and_undo_coalesce_original_spans() {
    let (_file, mut text) = document(b"");
    text.replace(0..0, "").expect("empty no-op");
    assert!(!text.undo().expect("no history"));
    text.replace(0..0, "hello").expect("insert");
    text.replace(2..2, "🦀").expect("insert in middle");
    assert_eq!(text.piece_count(), 3);
    assert!(text.undo().expect("undo middle"));
    assert_eq!(text.piece_count(), 1);
    assert_eq!(output(&mut text), b"hello");
    assert!(text.undo().expect("undo first"));
    assert!(text.is_empty());
}

#[test]
fn invalid_edits_and_reads_preserve_document_and_history() {
    let (_file, mut text) = document("a🦀z".as_bytes());
    for (start, end) in [(2, 3), (2, 2), (9, 9), (5, 1)] {
        assert_eq!(
            text.replace(start..end, "x")
                .expect_err("invalid edit")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            text.read(start..end).expect_err("invalid read").kind(),
            io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(output(&mut text), "a🦀z".as_bytes());
    assert!(text.is_at_save_point());
    assert!(!text.undo().expect("no history"));
    let (_file, mut large) = document(&vec![
        b'x';
        usize::try_from(MAX_WINDOW_BYTES + 1).expect("window")
    ]);
    assert_eq!(
        large.read(0..large.len()).expect_err("bounded read").kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn external_source_changes_do_not_change_captured_bytes() {
    let (mut file, mut text) = document(b"original");
    file.as_file_mut().set_len(0).expect("truncate");
    file.write_all(b"different contents").expect("overwrite");
    assert_eq!(output(&mut text), b"original");
}

#[test]
fn source_mutation_during_capture_is_reported() {
    let mut file = tempfile::NamedTempFile::new().expect("fixture");
    file.write_all(&vec![b'x'; BLOCK_BYTES * 2]).expect("write");
    let result = PagedText::capture(file.path(), &Cancellation::default(), |_| {
        file.as_file().set_len(0).expect("concurrent truncation");
    });
    assert!(
        result
            .err()
            .expect("capture must refuse")
            .to_string()
            .contains("source changed")
    );
}

#[test]
fn cancellation_stops_capture_and_output_without_changing_history() {
    let (file, mut text) = document(&vec![b'a'; BLOCK_BYTES * 3]);
    let cancel = Cancellation::default();
    let result = PagedText::capture(file.path(), &cancel, |_| cancel.cancel());
    assert_eq!(
        result.err().expect("cancelled capture").kind(),
        io::ErrorKind::Interrupted
    );
    let mut sink = CancelWriter {
        cancel: Cancellation::default(),
        written: 0,
    };
    let token = sink.cancel.clone();
    assert_eq!(
        text.write_to(&mut sink, &token)
            .expect_err("cancelled stream")
            .kind(),
        io::ErrorKind::Interrupted
    );
    assert_eq!(sink.written, BLOCK_BYTES);
    assert!(text.is_at_save_point());
    assert!(!text.undo().expect("no history"));
}
struct CancelWriter {
    cancel: Cancellation,
    written: usize,
}
impl Write for CancelWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.written += bytes.len();
        self.cancel.cancel();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn output_failure_does_not_acknowledge_a_save() {
    let (_file, mut text) = document(b"abc");
    text.replace(1..2, "more").expect("edit");
    let mut too_short = [0_u8; 2];
    let error = text
        .write_to(&mut too_short.as_mut_slice(), &Cancellation::default())
        .expect_err("output full");
    assert_eq!(error.kind(), io::ErrorKind::WriteZero);
    assert!(!text.is_at_save_point());
    assert_eq!(output(&mut text), b"amorec");
    assert!(text.undo().expect("undo still available"));
}

#[test]
fn differential_unicode_edits_and_history_match_string_oracle() {
    let (_file, mut text) = document(b"start\r\n");
    let mut oracle = String::from("start\r\n");
    let mut states = vec![oracle.clone()];
    let mut seed = 12345_usize;
    for step in 0..600 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let mut boundaries: Vec<_> = oracle.char_indices().map(|(offset, _)| offset).collect();
        boundaries.push(oracle.len());
        let first = boundaries[seed % boundaries.len()];
        let second = boundaries[(seed / 17) % boundaries.len()];
        let range = first.min(second)..first.max(second);
        let insertion = ["🦀", "\r\n", "e\u{301}", "", "abc"][step % 5];
        if range.is_empty() && insertion.is_empty() {
            continue;
        }
        text.replace(range.start as u64..range.end as u64, insertion)
            .expect("edit");
        oracle.replace_range(range, insertion);
        states.push(oracle.clone());
        assert_eq!(text.read(0..text.len()).expect("read"), oracle);
        assert_eq!(output(&mut text), oracle.as_bytes());
    }
    for expected in states.iter().rev().skip(1) {
        assert!(text.undo().expect("undo"));
        assert_eq!(output(&mut text), expected.as_bytes());
    }
    for expected in states.iter().skip(1) {
        assert!(text.redo().expect("redo"));
        assert_eq!(output(&mut text), expected.as_bytes());
    }
    text.mark_saved();
    assert!(text.undo().expect("undo saved"));
    text.replace(0..0, "branch").expect("new branch");
    assert!(!text.is_at_save_point());
    assert!(!text.redo().expect("new edit clears redo"));
}

#[test]
fn spans_use_full_u64_offsets_without_intermediate_overflow() {
    use super::pieces::{self, Piece, Store};
    let original = [Piece {
        store: Store::Original,
        offset: u64::MAX - 100,
        len: 100,
    }];
    let (parts, removed) = pieces::splice(&original, 30..70, &[]).expect("high backing offset");
    assert_eq!(pieces::length(&parts).expect("length"), 60);
    assert_eq!(parts[1].offset, u64::MAX - 30);
    assert_eq!(removed[0].offset, u64::MAX - 70);
    let huge = [Piece {
        store: Store::Original,
        offset: 0,
        len: u64::MAX,
    }];
    let insertion = [Piece {
        store: Store::Added,
        offset: 0,
        len: 1,
    }];
    assert!(pieces::splice(&huge, 0..0, &insertion).is_err());
}
