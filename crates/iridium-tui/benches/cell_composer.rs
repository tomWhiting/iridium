//! Frame preparation, repeated use and retained-snapshot edit costs without a TTY or time cap.

use iridium_editor::cell_layout::ScreenRow;
use iridium_editor::{Editor, KeyCode, KeyEvent, Language, Position};
use iridium_tui::cell::CellBuffer;
use iridium_tui::frame::{CellFrameOptions, Frame};
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

type BenchResult<T> = Result<T, Box<dyn std::error::Error>>;

const PREPARATIONS: u32 = 100;
const PAINTS: u32 = 100;
const HITS: u32 = 10_000;
const EDITS: u32 = 100;
const WIDTH: usize = 60;
const HEIGHT: usize = 8;

const fn options() -> CellFrameOptions<'static> {
    CellFrameOptions {
        columns: WIDTH,
        rows: HEIGHT,
        first_row: ScreenRow(0),
        chrome: None,
    }
}

fn report(
    output: &mut impl Write,
    case: &str,
    language: Option<Language>,
    operation: &str,
    iterations: u32,
    elapsed: Duration,
) -> BenchResult<()> {
    let language = language.as_ref().map_or("plain_text", Language::id);
    writeln!(
        output,
        "{{\"crate\":\"iridium-tui\",\"case\":\"{case}\",\"language\":\"{language}\",\"operation\":\"{operation}\",\"iterations\":{iterations},\"total_ns\":{},\"mean_ns\":{}}}",
        elapsed.as_nanos(),
        elapsed.as_nanos() / u128::from(iterations)
    )?;
    Ok(())
}

fn fixture(text: &str, language: Option<Language>) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    match language {
        Some(language) => editor.set_language(language),
        None => editor.clear_language(),
    }
    editor.set_cursor(Position::zero());
    editor.set_undo_group_timeout_ms(0);
    editor
}

fn stable_frame(
    output: &mut impl Write,
    case: &str,
    text: &str,
    language: Option<Language>,
) -> BenchResult<()> {
    let editor = fixture(text, language);
    let mut frame = Frame::new();
    let mut buffer = CellBuffer::new(WIDTH, HEIGHT);
    let start = Instant::now();
    for _ in 0..PREPARATIONS {
        black_box(Frame::prepare_cells(&editor, options())?);
    }
    report(
        output,
        case,
        language,
        "frame_prepare",
        PREPARATIONS,
        start.elapsed(),
    )?;
    let prepared = Frame::prepare_cells(&editor, options())?;
    frame.render_cells(&prepared, &mut buffer)?;
    let start = Instant::now();
    for _ in 0..PAINTS {
        black_box(frame.render_cells(&prepared, &mut buffer)?);
    }
    report(
        output,
        case,
        language,
        "prepared_paint",
        PAINTS,
        start.elapsed(),
    )?;
    let visible_rows = HEIGHT.min(prepared.layout().total_rows);
    if visible_rows == 0 {
        return Err(format!("{case}: benchmark has no visible document row").into());
    }
    let start = Instant::now();
    for iteration in 0..HITS {
        let column = usize::try_from(iteration)? % WIDTH;
        let row = usize::try_from(iteration)? % visible_rows;
        black_box(
            prepared
                .position_at(column, row)?
                .ok_or("benchmark document hit unexpectedly absent")?,
        );
    }
    report(
        output,
        case,
        language,
        "prepared_hit",
        HITS,
        start.elapsed(),
    )?;
    Ok(())
}

fn edit_cost(
    output: &mut impl Write,
    case: &str,
    text: &str,
    language: Option<Language>,
    retain_frame: bool,
) -> BenchResult<()> {
    let mut key_elapsed = Duration::ZERO;
    let mut total_elapsed = Duration::ZERO;
    for _ in 0..EDITS {
        let mut editor = fixture(text, language);
        let mut frame = Frame::new();
        let mut buffer = CellBuffer::new(WIDTH, HEIGHT);
        let prepared = Frame::prepare_cells(&editor, options())?;
        let input = prepared.input_options();
        // With syntax enabled, warm the query in both cases. A retained Frame
        // holds its immutable Rope snapshot; dropping it releases that owner.
        // Plain text is a separate control without a highlighting snapshot.
        frame.render_cells(&prepared, &mut buffer)?;
        drop(prepared);
        if !retain_frame {
            frame = Frame::new();
        }
        let start = Instant::now();
        black_box(editor.handle_cell_key(&KeyEvent::simple(KeyCode::Char('x')), input)?);
        key_elapsed += start.elapsed();
        let prepared = Frame::prepare_cells(&editor, options())?;
        black_box(frame.render_cells(&prepared, &mut buffer)?);
        total_elapsed += start.elapsed();
        // With syntax enabled both paint paths reconstruct the capture-name
        // Vec. The retained path also fences the old Rope/cache on a change.
        // Compiled queries remain process-wide OnceLock cached. These are
        // aggregate timings, not isolated COW or allocator measurements.
        if editor.cursor() != Position::new(0, 1) {
            return Err(format!("{case}: measured edit did not advance one scalar").into());
        }
    }
    let (key_operation, paint_operation) = if retain_frame {
        (
            "key_with_retained_frame",
            "edit_prepare_paint_retained_frame",
        )
    } else {
        (
            "key_without_retained_frame",
            "edit_prepare_paint_fresh_frame",
        )
    };
    report(output, case, language, key_operation, EDITS, key_elapsed)?;
    report(
        output,
        case,
        language,
        paint_operation,
        EDITS,
        total_elapsed,
    )?;
    Ok(())
}

fn main() -> BenchResult<()> {
    let mut output = BufWriter::new(std::io::stdout().lock());
    for language in [None, Some(Language::Rust)] {
        for (case, text) in [
            ("short", "Hello e\u{301}界 👩‍🔬".to_owned()),
            (
                "multiline",
                "let value = \"e\u{301}界👩‍🔬\";\n\tvalue\n".repeat(64),
            ),
            ("long_unbroken_unicode", "e\u{301}界👩‍🔬".repeat(1024)),
        ] {
            stable_frame(&mut output, case, &text, language)?;
            edit_cost(&mut output, case, &text, language, false)?;
            edit_cost(&mut output, case, &text, language, true)?;
        }
    }
    output.flush()?;
    Ok(())
}
