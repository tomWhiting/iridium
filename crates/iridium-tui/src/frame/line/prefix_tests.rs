use super::{LineLayout, PREFIX_BYTE_BUDGET};
use std::io::{self, Read};

struct Chunks<'a> {
    bytes: &'a [u8],
    maximum: usize,
    consumed: usize,
}
impl Read for Chunks<'_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let count = self.bytes.len().min(out.len()).min(self.maximum);
        out[..count].copy_from_slice(&self.bytes[..count]);
        self.bytes = &self.bytes[count..];
        self.consumed += count;
        Ok(count)
    }
}

struct Failed;
impl Read for Failed {
    fn read(&mut self, _out: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("injected source failure"))
    }
}

#[test]
fn streamed_graphemes_match_contiguous_layout_at_all_chunk_sizes() {
    let text = format!("{}\t漢e\u{301}👨‍👩‍👧‍👦🇦🇺🇬🇧क्षx\u{200b}done", "a".repeat(4090));
    for maximum in [1, 2, 3, 7, 4095, 4096] {
        for end in [1, 80, 4091, 4095, 4100, 4200] {
            let reader = Chunks {
                bytes: text.as_bytes(),
                maximum,
                consumed: 0,
            };
            let actual = LineLayout::read_prefix(reader, text.len() as u64, 4, end).expect("read");
            let expected = LineLayout::new(&text, 4);
            assert_eq!(
                actual.clusters(),
                &expected.clusters()[..actual.clusters().len()]
            );
            for cell in 0..end.min(actual.width()) {
                assert_eq!(actual.cell_to_column(cell), expected.cell_to_column(cell));
            }
            for column in 0..actual.char_count() {
                assert_eq!(
                    actual.column_to_cell(column),
                    expected.column_to_cell(column)
                );
            }
        }
    }
}

#[test]
fn visible_prefix_does_not_read_or_retain_the_long_suffix() {
    let source = "x".repeat(2 * PREFIX_BYTE_BUDGET);
    let mut reader = Chunks {
        bytes: source.as_bytes(),
        maximum: 4096,
        consumed: 0,
    };
    let layout = LineLayout::read_prefix(&mut reader, source.len() as u64, 4, 81).expect("prefix");
    assert_eq!(layout.width(), 81);
    assert_eq!(layout.clusters().len(), 81);
    assert_eq!(reader.consumed, 4096);
    assert!(layout.column_to_cell(1_000_000) >= 81);
}

#[test]
fn source_failures_and_invalid_utf8_are_reported() {
    let error = LineLayout::read_prefix(&b"short"[..], 100, 4, 80).expect_err("short read");
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    for bytes in [b"a\xff".as_slice(), b"a\xf0\x9f".as_slice()] {
        assert_eq!(
            LineLayout::read_prefix(bytes, bytes.len() as u64, 4, 80)
                .expect_err("UTF8")
                .kind(),
            io::ErrorKind::InvalidData
        );
    }

    assert!(
        LineLayout::read_prefix(Failed, 100, 4, 80)
            .expect_err("I/O")
            .to_string()
            .contains("injected")
    );
}

#[test]
fn pathological_clusters_fail_explicitly_at_the_scan_budget() {
    let source = format!("a{}", "\u{301}".repeat(PREFIX_BYTE_BUDGET / 2));
    let error =
        LineLayout::read_prefix(source.as_bytes(), source.len() as u64, 4, 80).expect_err("budget");
    assert!(error.to_string().contains("scan budget"));
}

#[test]
fn the_same_layout_reads_edited_paged_source_spans() {
    use iridium_file::{
        paged::{Cancellation, PagedText},
        test_support::TempDir,
    };
    let dir = TempDir::new("streamed-line");
    let path = dir.path().join("source.txt");
    std::fs::write(&path, "a漢e\u{301}tail").expect("fixture");
    let mut source = PagedText::capture(&path, &Cancellation::default(), |_| {}).expect("capture");
    source.replace(1..1, "🇦🇺\t").expect("edit");
    let len = source.len();
    let layout = LineLayout::read_prefix(source.reader(0..len).expect("reader"), len, 4, 80)
        .expect("layout");
    assert_eq!(layout, LineLayout::new("a🇦🇺\t漢e\u{301}tail", 4));
}

#[test]
fn long_line_paint_and_hit_testing_keep_source_columns_and_hide_far_carets() {
    use crate::{
        cell::CellBuffer,
        frame::{CellPosition, Chrome, Frame},
    };
    use iridium_editor::{Editor, EditorConfig, Position};
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(&format!(
        "a\t漢e\u{301}{}",
        "x".repeat(2 * PREFIX_BYTE_BUDGET)
    ));
    let mut buffer = CellBuffer::new(80, 10);
    let mut frame = Frame::new();
    let geometry = frame.render(&editor, Chrome::default(), &mut buffer);
    assert_eq!(
        geometry.document_position(
            &editor,
            CellPosition {
                column: geometry.text.origin + 5,
                row: 0
            }
        ),
        Some(Position::new(0, 2))
    );
    editor.set_cursor(Position::new(0, 1_000_000));
    // A directly supplied viewport can leave the caret offscreen.
    let mut viewport = editor.state().viewport.clone();
    viewport.scroll_offset_x = 0.0;
    editor.set_viewport(viewport);
    let geometry = frame.render(&editor, Chrome::default(), &mut buffer);
    assert!(geometry.primary_caret.is_none());
}

#[test]
#[ignore = "manual frame performance measurement"]
fn measure_streamed_long_line_frames() {
    use crate::{
        cell::CellBuffer,
        frame::{Chrome, Frame},
    };
    use iridium_editor::{Editor, EditorConfig};
    use std::time::Instant;
    let mut results = Vec::new();
    for bytes in [150_000, 8_000_000] {
        let source = "漢a\t".repeat(bytes / 5);
        let mut editor = Editor::new(EditorConfig::default());
        editor.set_content(&source);
        let mut frame = Frame::new();
        let mut buffer = CellBuffer::new(100, 24);
        frame.render(&editor, Chrome::default(), &mut buffer);
        let mut samples = Vec::new();
        for _ in 0..31 {
            let start = Instant::now();
            frame.render(&editor, Chrome::default(), &mut buffer);
            samples.push(start.elapsed().as_micros());
        }
        samples.sort_unstable();
        results.push(format!(
            "{{\"source_bytes\":{},\"median_us\":{},\"max_us\":{}}}",
            source.len(),
            samples[15],
            samples[30]
        ));
    }
    std::fs::write(
        "/private/tmp/iridium-render-read-benchmark.json",
        format!("[{}]", results.join(",")),
    )
    .expect("benchmark output");
}
