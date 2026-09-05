//! CPU-only cell geometry and key measurements; no renderer or timing pass threshold.

use iridium_editor::cell_layout::{CellColumn, CellRowMap, CellWrapParameters, ScreenRow};
use iridium_editor::editor::CellInputOptions;
use iridium_editor::{Editor, KeyCode, KeyEvent, Position};
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::time::{Duration, Instant};

type BenchResult<T> = Result<T, Box<dyn std::error::Error>>;

const PREPARATIONS: u32 = 100;
const HITS: u32 = 10_000;
const KEYS: u32 = 100;
const WIDTH: usize = 60;

fn report(
    output: &mut impl Write,
    case: &str,
    operation: &str,
    iterations: u32,
    elapsed: Duration,
) -> BenchResult<()> {
    writeln!(
        output,
        "{{\"crate\":\"iridium-editor\",\"case\":\"{case}\",\"operation\":\"{operation}\",\"iterations\":{iterations},\"total_ns\":{},\"mean_ns\":{}}}",
        elapsed.as_nanos(),
        elapsed.as_nanos() / u128::from(iterations)
    )?;
    Ok(())
}

fn fixture(text: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor.set_cursor(Position::zero());
    editor.set_undo_group_timeout_ms(0);
    editor
}

fn measure(output: &mut impl Write, case: &str, text: &str) -> BenchResult<()> {
    let editor = fixture(text);
    let state = editor.state();
    let parameters = CellWrapParameters::new(WIDTH, 4);
    let start = Instant::now();
    for _ in 0..PREPARATIONS {
        black_box(CellRowMap::prepare(
            &state.document,
            &state.fold_state,
            parameters,
        )?);
    }
    report(output, case, "map_prepare", PREPARATIONS, start.elapsed())?;
    let map = CellRowMap::prepare(&state.document, &state.fold_state, parameters)?;
    let start = Instant::now();
    for iteration in 0..HITS {
        let cell = usize::try_from(iteration)? % WIDTH;
        let row = usize::try_from(iteration)? % map.total_rows();
        black_box(map.position_at(ScreenRow(row), CellColumn(cell))?);
    }
    report(output, case, "prepared_hit", HITS, start.elapsed())?;
    let input = CellInputOptions {
        wrap: parameters,
        visible_rows: 8,
    };
    let mut elapsed = Duration::ZERO;
    // Each sample has identical initial history. Setup is outside key timing;
    // no growing undo tree or set_content reset enters the measured interval.
    for _ in 0..KEYS {
        let mut editor = fixture(text);
        let start = Instant::now();
        black_box(editor.handle_cell_key(&KeyEvent::simple(KeyCode::Char('x')), input)?);
        elapsed += start.elapsed();
        if editor.cursor() != Position::new(0, 1) {
            return Err(format!("{case}: measured key did not advance one scalar").into());
        }
    }
    report(output, case, "ordinary_key_dispatch", KEYS, elapsed)?;
    Ok(())
}

fn main() -> BenchResult<()> {
    let mut output = BufWriter::new(std::io::stdout().lock());
    for (case, text) in [
        ("short", "Hello e\u{301}界 👩‍🔬".to_owned()),
        (
            "multiline",
            "let value = \"e\u{301}界👩‍🔬\";\n\tvalue\n".repeat(64),
        ),
        ("long_unbroken_unicode", "e\u{301}界👩‍🔬".repeat(1024)),
    ] {
        measure(&mut output, case, &text)?;
    }
    output.flush()?;
    Ok(())
}
