//! Fixtures shared by the tests in this directory.
//!
//! Every helper here came from the single `mod tests` block the app was split
//! out of, unchanged: a session is opened on a real file in a real temporary
//! directory, and keys arrive as the same `KeyEvent`s winit would translate
//! into. Nothing here mocks the app — a test that passes against a fake is a
//! test that proves nothing about the shipped one.

use std::path::PathBuf;

use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_file::TextFile;
use iridium_file::test_support::TempDir;

use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};

/// A key press under the given modifiers.
pub(super) fn chord(key: KeyCode, modifiers: Modifiers) -> KeyEvent {
    KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    }
}

/// A plain key press.
pub(super) fn press(key: KeyCode) -> KeyEvent {
    chord(key, Modifiers::none())
}

/// The `Ctrl+S` save chord.
pub(super) fn ctrl_s() -> KeyEvent {
    chord(KeyCode::Char('s'), Modifiers::ctrl())
}

/// Opens a session on a file holding `text`.
pub(super) fn open(directory: &TempDir, name: &str, text: &str) -> (DesktopApp, PathBuf) {
    let path = directory.path().join(name);
    std::fs::write(&path, text).expect("the fixture file was written");
    let app = DesktopApp::new(Options {
        path: Some(path.clone()),
        ..Options::default()
    })
    .expect("the session opened");
    (app, path)
}

/// Types text into the app one plain key press at a time.
pub(super) fn type_into(app: &mut DesktopApp, text: &str) {
    for character in text.chars() {
        assert_eq!(app.press(&press(KeyCode::Char(character))), Flow::Running);
    }
}

/// A `Ctrl+Alt` chord.
pub(super) fn ctrl_alt(key: KeyCode) -> KeyEvent {
    chord(
        key,
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::none()
        },
    )
}

/// A ⌘⌥ chord — the mac spelling of the face's `Ctrl+Alt` shape.
pub(super) fn meta_alt(key: KeyCode) -> KeyEvent {
    chord(
        key,
        Modifiers {
            meta: true,
            alt: true,
            ..Modifiers::none()
        },
    )
}

/// A bare ⌘ chord.
pub(super) fn meta(key: KeyCode) -> KeyEvent {
    chord(
        key,
        Modifiers {
            meta: true,
            ..Modifiers::none()
        },
    )
}

/// A `⌘⇧<key>` chord.
pub(super) fn meta_shift(key: KeyCode) -> KeyEvent {
    chord(
        key,
        Modifiers {
            meta: true,
            shift: true,
            ..Modifiers::none()
        },
    )
}

/// Writes a second fixture and returns its path.
pub(super) fn fixture(directory: &TempDir, name: &str, text: &str) -> PathBuf {
    let path = directory.path().join(name);
    std::fs::write(&path, text).expect("the fixture file was written");
    path
}

/// `Ctrl+Shift+<key>` — the tab-walking chords.
pub(super) fn ctrl_shift(key: KeyCode) -> KeyEvent {
    chord(
        key,
        Modifiers {
            ctrl: true,
            shift: true,
            ..Modifiers::none()
        },
    )
}

/// The `Ctrl+W` close-tab chord. Shift is forbidden on this binding, so
/// it must not be set.
pub(super) fn ctrl_w() -> KeyEvent {
    chord(KeyCode::Char('w'), Modifiers::ctrl())
}

/// The label of the tab in front.
pub(super) fn front(app: &DesktopApp) -> Option<String> {
    app.test_document()
        .file
        .as_ref()
        .map(TextFile::display_name)
}
