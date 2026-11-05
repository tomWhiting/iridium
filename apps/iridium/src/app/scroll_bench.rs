//! Timing harness for scrolling and per-keystroke cost, run by hand.
//!
//! Every test here is `#[ignore]`d: they print measurements rather than assert
//! them, because a timing threshold in CI is a flake. Run them in release:
//! `cargo nextest run -p iridium --release --run-ignored only --no-capture scroll_bench`.
//!
//! The scroll measurement runs once per document size, 5,000, 50,000 and
//! 500,000 lines by default, so growth with the file shows in one run, and
//! for two shapes of Rust: flat statements, and the same lines inside
//! functions, which gives the file one fold region per twenty lines.
//! `SCROLL_BENCH_LINES=100000,1000000` picks other sizes, and
//! `SCROLL_BENCH_ROUNDS=50` repeats each scenario, to give a profiler time.

#![allow(clippy::print_stdout)]

use std::fmt::Write as _;
use std::fs;
use std::time::{Duration, Instant};

use iridium_editor::{Editor, KeyCode, KeyEvent, Modifiers};
use iridium_tui::cell::{ColorDepth, Surface};
use iridium_tui::driver::{Capabilities, Frame, TerminalState, write_frame};
use iridium_tui::input::{
    MouseEvent, MouseEventKind, MouseModifiers, ScrollDirection, TerminalInput,
};

use super::App;
use crate::cli::Options;
use crate::file::test_support::TempDir;

/// Document sizes the scroll measurement runs on unless told otherwise.
const SIZES: [usize; 3] = [5_000, 50_000, 500_000];
/// Lines in the document the keystroke measurement types into.
const TYPING_LINES: usize = 100_000;
/// The measured terminal.
const COLUMNS: usize = 200;
/// The measured terminal.
const ROWS: usize = 50;

/// The two shapes of measured document.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// Statements only: highlighted, and nothing to fold.
    Flat,
    /// The same statements in twenty-line functions, each one a fold region.
    Nested,
}

/// A Rust-looking document of `lines` lines, so highlighting is exercised.
fn document(lines: usize, shape: Shape) -> String {
    let mut text = String::with_capacity(lines * 48);
    for index in 0..lines {
        let _ = match (shape, index % 20) {
            (Shape::Nested, 0) => writeln!(text, "fn block_{index}() {{"),
            (Shape::Nested, 19) => writeln!(text, "}}"),
            _ => writeln!(
                text,
                "    let value_{index} = compute({index}, \"text\"); // note"
            ),
        };
    }
    text
}

/// A plain key press.
const fn press(key: KeyCode) -> TerminalInput {
    TerminalInput::Key(KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    })
}

/// What one frame cost: the input, the render, the diff and the bytes.
#[derive(Default)]
struct Totals {
    frames: u32,
    input: Duration,
    time: Duration,
    /// The slowest single frame: an average hides the one keystroke that
    /// restructured the file.
    worst: Duration,
    bytes: usize,
}

impl Totals {
    fn add(&mut self, other: &Self) {
        self.frames += other.frames;
        self.input += other.input;
        self.time += other.time;
        self.worst = self.worst.max(other.worst);
        self.bytes += other.bytes;
    }

    fn report(&self, label: &str) {
        let frames = self.frames.max(1);
        let per_frame = |time: Duration| time.as_secs_f64() * 1e6 / f64::from(frames);
        println!(
            "{label}: {} frames, {:.1} us/frame (input + render + diff + write), \
             {:.1} of it input, {:.1} the rest, worst {:.1} us, {} bytes/frame",
            self.frames,
            per_frame(self.time),
            per_frame(self.input),
            per_frame(self.time.saturating_sub(self.input)),
            self.worst.as_secs_f64() * 1e6,
            self.bytes / frames as usize,
        );
    }
}

/// Runs `inputs` through a session, timing each one to a written frame.
fn drive(app: &mut App, surface: &mut Surface, inputs: &[TerminalInput]) -> Totals {
    let state = TerminalState::new();
    let capabilities = Capabilities {
        color_depth: ColorDepth::TrueColor,
        keyboard_enhancement: true,
        synchronized_output: true,
    };
    let mut totals = Totals::default();
    let mut out: Vec<u8> = Vec::with_capacity(1 << 20);
    for input in inputs {
        out.clear();
        let started = Instant::now();
        app.handle_input(input);
        totals.input += started.elapsed();
        let cursor = app.render(surface);
        let damage = surface.damage();
        let frame = Frame {
            damage: &damage,
            capabilities,
            cursor,
        };
        write_frame(&mut out, &frame, &state).expect("a Vec accepts every write");
        surface.commit();
        let elapsed = started.elapsed();
        totals.time += elapsed;
        totals.worst = totals.worst.max(elapsed);
        totals.bytes += out.len();
        totals.frames += 1;
    }
    totals
}

/// The sizes to measure: `SCROLL_BENCH_LINES` if set, else [`SIZES`].
fn sizes() -> Vec<usize> {
    std::env::var("SCROLL_BENCH_LINES").map_or_else(
        |_| SIZES.to_vec(),
        |list| {
            list.split(',')
                .map(|size| {
                    size.trim()
                        .parse()
                        .expect("SCROLL_BENCH_LINES is a list of counts")
                })
                .collect()
        },
    )
}

/// How many times each scenario repeats: `SCROLL_BENCH_ROUNDS`, else once.
fn rounds() -> usize {
    std::env::var("SCROLL_BENCH_ROUNDS").map_or(1, |rounds| {
        rounds.parse().expect("SCROLL_BENCH_ROUNDS is a count")
    })
}

/// A session on a measured document, already drawn once.
fn session(directory: &TempDir, lines: usize, shape: Shape) -> (App, Surface) {
    let path = directory.path().join("measured.rs");
    fs::write(&path, document(lines, shape)).expect("the fixture could be written");
    let mut app = App::with_config(
        Options {
            path: Some(path),
            // Typed characters insert, as they do once a modal session is in
            // insert mode; the measurement is the edit, not the mode switch.
            non_modal: true,
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("the session opened");
    // The first parse runs in the background; waiting for it here means every
    // measured frame types into a parsed file, as a person would.
    app.workspace
        .active_editor_mut()
        .expect("the file is open")
        .settle_syntax();
    let mut surface = Surface::new(COLUMNS, ROWS, ColorDepth::TrueColor);
    app.render(&mut surface);
    surface.commit();
    (app, surface)
}

#[test]
#[ignore = "a measurement, run by hand in release"]
fn scroll_bench_arrow_and_page() {
    let notch = TerminalInput::Mouse(MouseEvent {
        kind: MouseEventKind::Scroll(ScrollDirection::Up),
        column: 40,
        row: 10,
        modifiers: MouseModifiers::empty(),
    });
    for (shape, lines) in [Shape::Flat, Shape::Nested]
        .into_iter()
        .flat_map(|shape| sizes().into_iter().map(move |lines| (shape, lines)))
    {
        let directory = TempDir::new("scroll-bench");
        let (mut app, mut surface) = session(&directory, lines, shape);
        let mut downs = Totals::default();
        let mut pages = Totals::default();
        let mut wheel = Totals::default();
        for _ in 0..rounds() {
            // Down from the top: past the first screen every press scrolls
            // one line. The pages and notches then bring the view back up
            // near where it started, so every round measures the same walk.
            app.workspace
                .active_editor_mut()
                .expect("a document is open")
                .set_cursor(iridium_editor::Position::new(0, 0));
            app.ensure_caret_visible();
            downs.add(&drive(
                &mut app,
                &mut surface,
                &vec![press(KeyCode::Down); 2_000],
            ));
            pages.add(&drive(
                &mut app,
                &mut surface,
                &vec![press(KeyCode::PageDown); 200],
            ));
            wheel.add(&drive(&mut app, &mut surface, &vec![notch.clone(); 2_000]));
        }
        println!("{shape:?}, {lines} lines:");
        downs.report("  arrow down");
        pages.report("  page down");
        wheel.report("  wheel up");
    }
}

#[test]
#[ignore = "a measurement, run by hand in release"]
fn scroll_bench_keystroke_cost() {
    for listening in [false, true] {
        let mut editor = Editor::with_defaults();
        editor.set_content(&document(TYPING_LINES, Shape::Flat));
        if listening {
            editor.add_listener(|_| {});
        }
        let key = KeyEvent {
            key: KeyCode::Char('x'),
            modifiers: Modifiers::none(),
            is_repeat: false,
        };
        let presses = 2_000_u32;
        let started = Instant::now();
        for _ in 0..presses {
            editor.handle_key(&key);
        }
        let elapsed = started.elapsed();
        println!(
            "typing into {TYPING_LINES} lines, listener {}: {:.1} us/keystroke",
            if listening { "registered" } else { "absent" },
            elapsed.as_secs_f64() * 1e6 / f64::from(presses),
        );
    }
    // Typing over a selection is an edit that keeps the length, so whether
    // the buffer still matches the file cannot be read off the length. Each
    // keystroke selects one character of an identifier mid-file and types
    // over it, timed to a written frame, which is where the face asks that.
    // The nested shape, because it is valid Rust: the flat one is all parse
    // errors, and tree-sitter's recovery from those swamps everything else.
    let select = TerminalInput::Key(KeyEvent {
        key: KeyCode::Right,
        modifiers: Modifiers::shift(),
        is_repeat: false,
    });
    let type_over = press(KeyCode::Char('x'));
    for lines in sizes() {
        let directory = TempDir::new("keystroke-bench");
        let (mut app, mut surface) = session(&directory, lines, Shape::Nested);
        let mut typed = Totals::default();
        for _ in 0..rounds() {
            for keystroke in 0..1_000 {
                // Column 8 starts `value_`: eight identifier characters, on
                // the statement lines between one function's braces.
                let line = lines / 2 + 1 + (keystroke / 8) % 18;
                let column = 8 + keystroke % 8;
                app.workspace
                    .active_editor_mut()
                    .expect("a document is open")
                    .set_cursor(iridium_editor::Position::new(line, column));
                app.ensure_caret_visible();
                drive(&mut app, &mut surface, std::slice::from_ref(&select));
                typed.add(&drive(
                    &mut app,
                    &mut surface,
                    std::slice::from_ref(&type_over),
                ));
            }
        }
        typed.report(&format!("typing over a selection, {lines} lines"));
    }
}

/// The keys that type `text`, one press per character.
fn typed(text: &str) -> Vec<TerminalInput> {
    text.chars().map(|ch| press(KeyCode::Char(ch))).collect()
}

/// A session on a nested document of `lines` lines with the caret at `column`
/// of the first statement of the middle function, drawn once.
fn typing_session(directory: &TempDir, lines: usize, column: usize) -> (App, Surface) {
    let (mut app, mut surface) = session(directory, lines, Shape::Nested);
    // Line 0 of every twenty opens a function, so line 1 is its first
    // statement.
    let line = (lines / 2) / 20 * 20 + 1;
    app.workspace
        .active_editor_mut()
        .expect("the file is open")
        .set_cursor(iridium_editor::Position::new(line, column));
    app.ensure_caret_visible();
    drive(&mut app, &mut surface, &[]);
    (app, surface)
}

/// Times each of `keys` to a written frame, typed at `column` of the first
/// statement of the middle function of a fresh nested document.
fn typing_case(lines: usize, column: usize, keys: &[TerminalInput]) -> Totals {
    let directory = TempDir::new("typing-bench");
    let (mut app, mut surface) = typing_session(&directory, lines, column);
    let mut totals = Totals::default();
    for key in keys {
        totals.add(&drive(&mut app, &mut surface, std::slice::from_ref(key)));
    }
    totals
}

#[test]
#[ignore = "a measurement, run by hand in release"]
fn scroll_bench_typing_through_invalid_code() {
    // Real typing passes through invalid code constantly, and tree-sitter's
    // recovery from it can cost far more than a reparse of valid code. Each
    // case types at column 4 of a statement, before its `let`, so nothing the
    // editor auto-closes closes it.
    //
    // An unclosed brace: `if ready {` and then a statement, so everything
    // after it nests one level deeper until the end of the file.
    let mut brace = typed("if ready {");
    brace.extend(
        typed(" let more = ready + 1;")
            .into_iter()
            .cycle()
            .take(110),
    );
    // An unclosed quote: a Rust string may span lines, so every quote after
    // this one in the file flips between opening and closing a string.
    let mut quote = typed("\"");
    quote.extend(
        typed("words typed into a string ")
            .into_iter()
            .cycle()
            .take(119),
    );
    for lines in sizes() {
        typing_case(lines, 4, &brace).report(&format!("unclosed brace, {lines} lines"));
        typing_case(lines, 4, &quote).report(&format!("unclosed quote, {lines} lines"));
        // Enter splits the statement inside its identifier, and Backspace
        // joins it again.
        let directory = TempDir::new("enter-bench");
        let (mut app, mut surface) = typing_session(&directory, lines, 12);
        let mut enters = Totals::default();
        let mut backspaces = Totals::default();
        for _ in 0..200 {
            enters.add(&drive(&mut app, &mut surface, &[press(KeyCode::Enter)]));
            backspaces.add(&drive(&mut app, &mut surface, &[press(KeyCode::Backspace)]));
        }
        enters.report(&format!("enter, {lines} lines"));
        backspaces.report(&format!("backspace, {lines} lines"));
    }
}
