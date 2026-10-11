//! Tests for the state machine.
//!
//! Every one of these drives [`App`] the way the terminal does — a
//! [`TerminalInput`] in, a [`Surface`] out — with no terminal anywhere, which is
//! the whole reason the event loop is the only thing that is not in this
//! library. What they are checking is the part that is this crate's own: which
//! key reaches which verb, what a save refuses, what the prompt line says. The
//! kernel's editing behaviour is the kernel's to test.

use crate::app::modal_tests::legacy_options;
use std::fs;
use std::path::PathBuf;

use iridium_editor::{KeyCode, KeyEvent, Language, Modifiers};
use iridium_tui::cell::{CellBuffer, CellContent, ColorDepth, Surface};
use iridium_tui::input::TerminalInput;

use super::prompt::Prompt;
use super::{App, Flow};
use crate::cli::Options;
use crate::file::test_support::TempDir;

/// A key press with no modifiers.
fn press(key: KeyCode) -> TerminalInput {
    TerminalInput::Key(KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    })
}

/// A bare `Ctrl` chord.
fn ctrl(character: char) -> TerminalInput {
    TerminalInput::Key(KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    })
}

/// An `Alt` chord with no other modifier.
fn alt(character: char) -> TerminalInput {
    TerminalInput::Key(KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers {
            shift: false,
            ctrl: false,
            alt: true,
            meta: false,
            alt_graph: false,
        },
        is_repeat: false,
    })
}

/// A `Ctrl+Alt` chord.
fn ctrl_alt(character: char) -> TerminalInput {
    TerminalInput::Key(KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers {
            shift: false,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        },
        is_repeat: false,
    })
}

/// Types text into the session, one key press per character.
fn type_text(app: &mut App, text: &str) {
    for character in text.chars() {
        assert_eq!(
            app.handle_input(&press(KeyCode::Char(character))),
            Flow::Running
        );
    }
}

/// A session editing a file that holds `text`.
fn open(directory: &TempDir, name: &str, text: &str) -> (App, PathBuf) {
    let path = directory.path().join(name);
    fs::write(&path, text).expect("the fixture could be written");
    let app = App::with_config(
        Options {
            path: Some(path.clone()),
            ..legacy_options()
        },
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("the session opened");
    (app, path)
}

/// A session editing an unnamed buffer.
fn unnamed() -> App {
    App::with_config(
        legacy_options(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("the session opened")
}

/// The text of one row, with continuation cells contributing nothing.
fn row_text(buffer: &CellBuffer, row: usize) -> String {
    let mut out = String::new();
    let Some(cells) = buffer.row(row) else {
        return out;
    };
    for cell in cells {
        match cell.content() {
            CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
            CellContent::Continuation => {},
        }
    }
    out
}

/// Every row of one frame drawn at this size.
fn frame(app: &mut App, columns: usize, rows: usize) -> Vec<String> {
    let mut surface = Surface::new(columns, rows, ColorDepth::TrueColor);
    app.render(&mut surface);
    (0..rows).map(|row| row_text(surface.back(), row)).collect()
}

/// A session started with `bindings` as the user's configuration.
fn with_bindings(bindings: Vec<iridium_editor::KeyBinding>) -> App {
    App::with_config(
        legacy_options(),
        iridium_config::UserConfig {
            bindings,
            ..iridium_config::UserConfig::default()
        },
        None,
    )
    .expect("the session opened")
}

mod clipboard;
/// ⭐ **The terminal face reads `[keys]`.** Until this landed, `apps/iridium`
/// touched `iridium_config` only for themes, so a binding somebody wrote worked
/// in the desktop app and did nothing here.
///
/// ⚠️ Asserted by **pressing the key and reading the document**, not by looking
/// at the keymap. A layer that is installed but outranked, or installed on an
/// `Editor` this session does not own, satisfies an inspection and fails the
/// user — and the version of this test that inspected a bare `Editor` went on
/// passing with the wiring removed from the constructor entirely.
mod config;
mod editing;
mod explorer;
mod files;
mod fold_reveal;
mod folds;
mod navigation;
mod panels;
mod search_paste;
mod structural;
mod view;

mod structural_mode;
