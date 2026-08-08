//! The light/dark toggle, and the surfaces it has to reach.
//!
//! A theme lives in more than one place in this face — the workspace hands it
//! to every open editor, and the compositor keeps its own copy because the
//! clear colour and the retained shaped buffers are built from it. These tests
//! are the record of "all of them, or the window lies about what theme it is
//! wearing".

use iridium_config::theme::ThemeChoice;
use iridium_editor::KeyCode;
use iridium_file::test_support::TempDir;

use super::support::*;
use crate::app::startup::Options;
use crate::app::state::{DesktopApp, Flow};

/// A session that asked for a theme on the command line.
fn opened_with(choice: ThemeChoice) -> Result<DesktopApp, crate::app::startup::StartupError> {
    DesktopApp::new(Options {
        theme: Some(choice),
        ..Options::default()
    })
}

#[test]
fn the_toggle_swaps_the_theme_and_swaps_it_back() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    assert!(
        app.workspace.theme().is_dark,
        "the session starts on the dark preset"
    );

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(!app.workspace.theme().is_dark, "Ctrl+Alt+T reaches light");
    assert_eq!(
        app.workspace.theme().name,
        "Iridium Platinum",
        "the light side of the toggle is the ruled preset, not a generic light"
    );

    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(
        app.workspace.theme().is_dark,
        "a toggle that only went one way would need a second key to come back"
    );
}

#[test]
fn the_mac_spelling_reaches_the_same_toggle() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    assert_eq!(app.press(&meta_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(!app.workspace.theme().is_dark, "⌘⌥T reaches light too");
}

/// The theme has to reach a tab that was not in front when it changed.
///
/// ⚠️ This is the failure a single-editor test cannot see. The kernel's
/// `Workspace::set_theme` walks every open document precisely so a background
/// tab does not come forward still wearing the old preset, and a face that
/// reached only `active_editor` would pass every other test in this file while
/// producing a window whose tabs disagree with each other.
#[test]
fn the_toggle_reaches_a_background_tab() {
    let directory = TempDir::new("desktop-theme-tabs");
    let (mut app, _first) = open(&directory, "a.txt", "first");
    let second = fixture(&directory, "b.txt", "second");
    app.dropped(&second);
    assert_eq!(app.workspace.tab_count(), 2);

    // Toggled with the *second* tab in front, so the first is the one nothing
    // was looking at when the theme moved.
    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(!app.test_editor().state().theme.is_dark);

    assert_eq!(app.press(&ctrl_shift(KeyCode::Char('['))), Flow::Running);
    assert_eq!(app.test_editor().content(), "first", "the other tab");
    assert!(
        !app.test_editor().state().theme.is_dark,
        "the background tab came forward still wearing the dark preset"
    );
}

// ---------------------------------------------------------------------------
// Following the system appearance, and the pin that stops it — D-2 and D-7.
// ---------------------------------------------------------------------------

/// Without a `--theme`, the window wears whatever the system is wearing.
#[test]
fn a_session_that_asked_for_nothing_follows_the_system() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    assert!(app.workspace.theme().is_dark, "the default is dark");

    assert!(
        app.follow_system_appearance(false),
        "a light system is followed"
    );
    assert!(!app.workspace.theme().is_dark);

    assert!(
        app.follow_system_appearance(true),
        "and so is a change back"
    );
    assert!(app.workspace.theme().is_dark);
}

/// ⭐ The whole reason the pin exists.
///
/// macOS under "Auto" flips at sunset. Without this, the flip silently
/// reverses a choice the user made deliberately, at a moment they did not
/// pick — the failure is not that the theme is wrong but that nothing said
/// anything decided against them.
#[test]
fn the_manual_toggle_stops_the_session_following() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    assert_eq!(app.press(&ctrl_alt(KeyCode::Char('t'))), Flow::Running);
    assert!(!app.workspace.theme().is_dark, "the toggle reached light");

    assert!(
        !app.follow_system_appearance(true),
        "a pinned session declines the system's appearance"
    );
    assert!(
        !app.workspace.theme().is_dark,
        "and the manual choice survives the flip"
    );
}

/// `--theme` pins from the first frame, not from the first toggle.
///
/// The ordering matters and is easy to get backwards: `Shell::open` reads the
/// system appearance *after* `DesktopApp::new` has applied the flag, so a flag
/// that did not pin would be overwritten before the first frame was drawn —
/// the option would appear to do nothing at all on a machine whose appearance
/// disagreed with it.
#[test]
fn the_theme_flag_pins_before_the_window_exists() {
    let mut app = opened_with(ThemeChoice::Light).expect("a --theme light session opened");
    assert!(!app.workspace.theme().is_dark, "the flag reached the theme");
    assert_eq!(
        app.workspace.theme().name,
        "Iridium Platinum",
        "`--theme light` is the ruled preset, the same one the toggle reaches"
    );

    assert!(
        !app.follow_system_appearance(true),
        "an explicit --theme is a pin, so a dark system does not override it"
    );
    assert!(!app.workspace.theme().is_dark);
}

/// `--theme dark` is a pin too, even though it names what the session would
/// have started on anyway. Asking for dark and getting light at sunrise is the
/// same defect as the other way round.
#[test]
fn asking_for_the_default_still_pins() {
    let mut app = opened_with(ThemeChoice::Dark).expect("a --theme dark session opened");
    assert!(
        !app.follow_system_appearance(false),
        "`--theme dark` is a choice, not a coincidence"
    );
    assert!(app.workspace.theme().is_dark);
}

/// An appearance that changes nothing costs no frame.
///
/// macOS delivers `ThemeChanged` for appearance changes that are not the
/// light/dark one, and a session that repainted on each would burn frames on
/// an accent-colour change.
#[test]
fn an_appearance_that_matches_is_not_reapplied() {
    let mut app = DesktopApp::new(Options::default()).expect("an empty session opened");
    assert!(
        !app.follow_system_appearance(true),
        "the session is already dark"
    );
}

/// A `--theme` path that cannot be read stops the session rather than opening
/// a window in the wrong colours.
///
/// Deliberately unlike a bad `config.toml`, which falls back to defaults and
/// carries on: that file is *found* by the editor and its absence is normal,
/// while this path was typed by whoever ran the command.
#[test]
fn a_theme_file_that_cannot_be_read_stops_the_session() {
    let directory = TempDir::new("desktop-theme-missing");
    let missing = directory.path().join("nowhere.json");
    let error = opened_with(ThemeChoice::File(missing))
        .expect_err("a missing theme file is fatal")
        .to_string();
    assert!(
        error.contains("nowhere.json"),
        "the file that could not be read is named: {error}"
    );
}

/// And a file that exists but is neither format reports both complaints.
#[test]
fn a_theme_file_in_neither_format_reports_both_parsers() {
    let directory = TempDir::new("desktop-theme-garbage");
    let path = fixture(&directory, "nonsense.json", "{ this is not even JSON");
    let error = opened_with(ThemeChoice::File(path))
        .expect_err("an unparsable theme is fatal")
        .to_string();
    assert!(
        error.contains("Iridium theme") && error.contains("VS Code"),
        "being told only half of why it failed is the least useful half: {error}"
    );
}

/// ⚠️ **A known gap, pinned here rather than left to be discovered.**
///
/// Well-formed JSON that is nobody's theme loads *successfully* as an empty
/// VS Code theme, because every field of that format is optional in VS Code
/// itself — see `iridium_config::theme`'s note on parser ordering. So a native
/// theme with a typo in its structure does not report the typo; it opens a
/// window in default colours and says nothing.
///
/// This test asserts the behaviour that exists, not the behaviour that should.
/// Recorded as #100; fixing it means deciding what "empty enough to refuse"
/// means in the kernel's VS Code parser, which both faces share and which is
/// not this change's to move.
#[test]
fn a_json_document_with_no_theme_fields_is_accepted_today() {
    let directory = TempDir::new("desktop-theme-empty");
    let path = fixture(
        &directory,
        "nonsense.json",
        "{ \"this\": \"is not a theme\" }",
    );
    assert!(
        opened_with(ThemeChoice::File(path)).is_ok(),
        "if this now fails, #100 was fixed and this test should become the \
         assertion that it reports the file"
    );
}
