//! Timing harness for scrolling and per-keystroke cost, run by hand.
//!
//! Every test here is `#[ignore]`d: they print measurements rather than assert
//! them, because a timing threshold in CI is a flake. Run them in release:
//! `cargo nextest run -p iridium --release --run-ignored only --no-capture scroll_bench`.

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

/// Lines in the measured document.
const LINES: usize = 100_000;
/// The measured terminal.
const COLUMNS: usize = 200;
/// The measured terminal.
const ROWS: usize = 50;

/// A Rust-looking document of [`LINES`] lines, so highlighting is exercised.
fn document() -> String {
    let mut text = String::with_capacity(LINES * 48);
    for index in 0..LINES {
        let _ = writeln!(
            text,
            "    let value_{index} = compute({index}, \"text\"); // note"
        );
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
    time: Duration,
    bytes: usize,
}

impl Totals {
    fn report(&self, label: &str) {
        let frames = self.frames.max(1);
        println!(
            "{label}: {} frames, {:.1} us/frame (input + render + diff + write), {} bytes/frame",
            self.frames,
            self.time.as_secs_f64() * 1e6 / f64::from(frames),
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
        let cursor = app.render(surface);
        let damage = surface.damage();
        let frame = Frame {
            damage: &damage,
            capabilities,
            cursor,
        };
        write_frame(&mut out, &frame, &state).expect("a Vec accepts every write");
        surface.commit();
        totals.time += started.elapsed();
        totals.bytes += out.len();
        totals.frames += 1;
    }
    totals
}

/// A session on the measured document, already drawn once.
fn session(directory: &TempDir) -> (App, Surface) {
    let path = directory.path().join("measured.rs");
    fs::write(&path, document()).expect("the fixture could be written");
    let mut app = App::with_config(
        Options {
            path: Some(path),
            ..Options::default()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("the session opened");
    let mut surface = Surface::new(COLUMNS, ROWS, ColorDepth::TrueColor);
    app.render(&mut surface);
    surface.commit();
    (app, surface)
}

#[test]
#[ignore = "a measurement, run by hand in release"]
fn scroll_bench_arrow_and_page() {
    let directory = TempDir::new("scroll-bench");
    let (mut app, mut surface) = session(&directory);

    // Down from the top: past the first screen every press scrolls one line.
    let downs = vec![press(KeyCode::Down); 2_000];
    drive(&mut app, &mut surface, &downs).report("arrow down x2000");

    let pages = vec![press(KeyCode::PageDown); 200];
    drive(&mut app, &mut surface, &pages).report("page down x200");

    let notch = TerminalInput::Mouse(MouseEvent {
        kind: MouseEventKind::Scroll(ScrollDirection::Up),
        column: 40,
        row: 10,
        modifiers: MouseModifiers::empty(),
    });
    let notches = vec![notch; 2_000];
    drive(&mut app, &mut surface, &notches).report("wheel up x2000");
}

#[test]
#[ignore = "a measurement, run by hand in release"]
fn scroll_bench_keystroke_cost() {
    for listening in [false, true] {
        let mut editor = Editor::with_defaults();
        editor.set_content(&document());
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
            "typing into {LINES} lines, listener {}: {:.1} us/keystroke",
            if listening { "registered" } else { "absent" },
            elapsed.as_secs_f64() * 1e6 / f64::from(presses),
        );
    }
}
