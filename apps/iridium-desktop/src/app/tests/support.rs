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
use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;

use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};
use crate::overlay::{PaintedFrame, PaintedPanel, PanelGeometry, PanelKind, PanelRect};
use crate::units::{index_to_f32, pixel_to_index};

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

/// The row pitch the pointer fixtures place panels on, in pixels.
///
/// A round number rather than the shipped font's: these tests are about which
/// row a pixel lands on, and a pitch nobody can divide in their head makes a
/// failure read as arithmetic when it is routing.
pub(super) const ROW_PITCH: f32 = 20.0;

/// The padding the pointer fixtures give a panel, in pixels.
pub(super) const PANEL_PAD: f32 = 8.0;

/// A panel placed at `(x, y)` with `rows` interior rows.
///
/// Built by hand rather than through `panel_geometry`, deliberately: what
/// these tests exercise is what the *pointer* does with a placement, and a
/// fixture that went through the placement arithmetic would fail for two
/// unrelated reasons. The placement arithmetic has its own tests, without a
/// GPU, in [`crate::overlay`].
pub(super) fn placed(x: f32, y: f32, rows: usize, columns: usize) -> PanelGeometry {
    let char_width = 8.0;
    PanelGeometry {
        exterior: PanelRect {
            x,
            y,
            width: 2.0_f32.mul_add(PANEL_PAD, index_to_f32(columns) * char_width),
            height: 2.0_f32.mul_add(PANEL_PAD, index_to_f32(rows) * ROW_PITCH),
            radius: 0.0,
        },
        content_x: x + PANEL_PAD,
        content_y: y + PANEL_PAD,
        line_height: ROW_PITCH,
        char_width,
        scale: 1.0,
    }
}

/// The middle of interior row `row` of `geometry`.
pub(super) fn row_center(geometry: PanelGeometry, row: usize) -> (f32, f32) {
    (
        geometry.content_x + 1.0,
        index_to_f32(row).mul_add(geometry.line_height, geometry.content_y)
            + geometry.line_height / 2.0,
    )
}

/// Tells the app that the last frame painted these panels, in this order,
/// each drawing as many rows as [`placed`] gave it room for.
///
/// This is the seam the whole pointer ladder is tested through: the record is
/// what a press is resolved against, and it is owned by the app rather than by
/// the GPU painter precisely so a test can state it.
pub(super) fn painted(app: &mut DesktopApp, panels: &[(PanelKind, PanelGeometry)]) {
    app.painted = PaintedFrame {
        panels: panels
            .iter()
            .map(|&(kind, geometry)| PaintedPanel {
                kind,
                geometry: Some(geometry),
                rows: rows_of(geometry),
            })
            .collect(),
        tabs: None,
    };
}

/// How many rows a fixture placement was built to hold.
fn rows_of(geometry: PanelGeometry) -> usize {
    let interior = 2.0_f32.mul_add(-PANEL_PAD, geometry.exterior.height);
    pixel_to_index((interior / geometry.line_height).round())
}

/// Puts the pointer at `(x, y)`.
pub(super) fn point_at(app: &mut DesktopApp, position: (f32, f32)) {
    app.pointer.set_position(position.0, position.1);
}

/// One wheel gesture downwards — towards the end of whatever it lands on.
///
/// ⚠️ **A pixel delta rather than a line delta, and that is what makes the
/// document half of these tests able to fail.** A line delta is scaled by the
/// compositor's line height, and these sessions have no window and therefore
/// no compositor — so a line delta would resolve to zero and "the document did
/// not move" would be true no matter what the routing did. A pixel delta is
/// what a trackpad reports and needs no font, so a press routed wrongly moves
/// the document by a number the test can see.
pub(super) fn wheel_down() -> MouseScrollDelta {
    MouseScrollDelta::PixelDelta(PhysicalPosition { x: 0.0, y: -60.0 })
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
