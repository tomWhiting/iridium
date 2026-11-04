//! The file explorer in this face, driven by the bytes a terminal sends.
//!
//! Every key here starts as raw terminal input and goes through the same parse
//! and translation the driver uses, so a binding no terminal can deliver fails
//! here rather than in somebody's shell.

use std::fs;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use iridium_tui::cell::{CellBuffer, CellContent, ColorDepth, Surface};
use iridium_tui::input::{TerminalInput, translate};
use terminput::Event;

use super::{App, Flow, SearchTarget};
use crate::cli::Options;
use crate::file::test_support::TempDir;

/// How long anything on the reader thread is waited for.
const PATIENCE: Duration = Duration::from_secs(10);

/// Bytes a terminal sends, as this face receives them.
fn bytes(raw: &[u8]) -> TerminalInput {
    let event = Event::parse_from(raw)
        .unwrap_or_else(|error| panic!("{raw:?} did not parse: {error}"))
        .unwrap_or_else(|| panic!("{raw:?} is an incomplete sequence"));
    translate(event)
}

/// Feeds raw terminal bytes to the session.
fn send(app: &mut App, raw: &[u8]) {
    assert_eq!(
        app.handle_input(&bytes(raw)),
        Flow::Running,
        "{raw:?} ended the session"
    );
}

/// Types text one byte-sequence per character.
fn type_text(app: &mut App, text: &str) {
    let mut buffer = [0_u8; 4];
    for character in text.chars() {
        send(app, character.encode_utf8(&mut buffer).as_bytes());
    }
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

/// One frame at 100 by 30, as text.
fn frame(app: &mut App) -> String {
    let mut surface = Surface::new(100, 30, ColorDepth::TrueColor);
    app.render(&mut surface);
    (0..30)
        .map(|row| row_text(surface.back(), row))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A directory with two files, and a session started on it as a project.
fn project(name: &str) -> (TempDir, App) {
    let directory = TempDir::new(name);
    fs::write(directory.path().join("alpha.txt"), "a").expect("the fixture was written");
    fs::write(directory.path().join("beta.txt"), "b").expect("the fixture was written");
    let app = App::new(Options {
        path: Some(directory.path().to_path_buf()),
        ..Options::default()
    })
    .expect("a directory opens as a project");
    (directory, app)
}

/// A waker that reports on a channel, installed the way the event loop does.
fn wakes(app: &mut App) -> Receiver<()> {
    let (sender, receiver) = mpsc::channel();
    app.set_waker(Arc::new(move || {
        let _ = sender.send(());
    }));
    receiver
}

/// Renders frames, each after a wake, until `wanted` is on screen.
///
/// Blocks on the waker rather than sleeping, so a listing that lands without
/// waking the loop fails here instead of being found by a later frame.
fn until_drawn(app: &mut App, woken: &Receiver<()>, wanted: &str) -> String {
    let deadline = Instant::now() + PATIENCE;
    loop {
        let screen = frame(app);
        if screen.contains(wanted) {
            return screen;
        }
        let left = deadline.saturating_duration_since(Instant::now());
        assert!(
            woken.recv_timeout(left).is_ok(),
            "nothing woke the loop and `{wanted}` never arrived:\n{screen}"
        );
    }
}

/// The lines of the listing being edited.
fn listing(app: &App) -> Vec<String> {
    let editor = app
        .explorer
        .as_ref()
        .and_then(iridium_tui::frame::FileExplorerPanel::buffer_editor)
        .expect("the listing is being edited");
    let document = &editor.state().document;
    (0..document.line_count())
        .map(|line| document.line(line).unwrap_or_default())
        .collect()
}

#[test]
fn ctrl_s_from_a_terminal_asks_and_y_applies_and_the_new_listing_is_drawn_unprompted() {
    let (directory, mut app) = project("tui-face-explorer-apply");
    let woken = wakes(&mut app);
    until_drawn(&mut app, &woken, "beta.txt");

    send(&mut app, b"\t");
    // The caret starts at the end of the first line, `alpha.txt`.
    type_text(&mut app, "2");
    assert_eq!(listing(&app)[0], "alpha.txt2");

    // Ctrl+S is 0x13 from every terminal there is.
    send(&mut app, b"\x13");
    let screen = frame(&mut app);
    assert!(
        screen.contains("alpha.txt2"),
        "Ctrl+S showed no plan:\n{screen}"
    );
    assert!(
        directory.path().join("alpha.txt").exists(),
        "Ctrl+S applied without asking"
    );

    while woken.try_recv().is_ok() {}
    send(&mut app, b"y");
    assert!(
        directory.path().join("alpha.txt2").exists(),
        "y did not apply the rename"
    );
    assert!(!directory.path().join("alpha.txt").exists());

    // The re-read lands on the reader thread; the wake is what draws it.
    let screen = until_drawn(&mut app, &woken, "alpha.txt2");
    assert!(
        !screen.contains("alpha.txt\n"),
        "the stale name is still drawn:\n{screen}"
    );
}

#[test]
fn enter_from_a_terminal_makes_a_new_line_and_applying_it_creates_the_file() {
    let (directory, mut app) = project("tui-face-explorer-enter");
    let woken = wakes(&mut app);
    until_drawn(&mut app, &woken, "beta.txt");

    send(&mut app, b"\t");
    send(&mut app, b"\r");
    type_text(&mut app, "gamma.txt");
    assert_eq!(listing(&app), ["alpha.txt", "gamma.txt", "beta.txt"]);

    send(&mut app, b"\x13");
    send(&mut app, b"y");
    assert!(
        directory.path().join("gamma.txt").is_file(),
        "the new line created nothing"
    );
}

#[test]
fn escape_from_a_terminal_leaves_the_text_and_a_second_throws_edits_away() {
    let (_directory, mut app) = project("tui-face-explorer-escape");
    let woken = wakes(&mut app);
    until_drawn(&mut app, &woken, "beta.txt");

    send(&mut app, b"\t");
    type_text(&mut app, "x");
    send(&mut app, b"\x1b");
    assert!(
        app.message().is_some(),
        "leaving with unapplied edits said nothing"
    );
    send(&mut app, b"\x1b");
    let editing = app
        .explorer
        .as_ref()
        .is_some_and(|explorer| explorer.buffer_editor().is_some());
    assert!(!editing, "the second escape did not leave the text");
    assert!(
        !app.explorer
            .as_ref()
            .is_some_and(iridium_tui::frame::FileExplorerPanel::has_unapplied_edits),
        "the thrown-away edits are still held"
    );
}

#[test]
fn reopening_the_explorer_roots_it_at_the_project_not_the_working_directory() {
    let (directory, mut app) = project("tui-face-explorer-reopen");
    let woken = wakes(&mut app);
    until_drawn(&mut app, &woken, "beta.txt");

    // Ctrl+Alt+E, as a terminal sends it: Escape, then Ctrl+E.
    send(&mut app, b"\x1b\x05");
    assert!(
        !app.is_explorer_open(),
        "the chord did not close the explorer"
    );
    send(&mut app, b"\x1b\x05");
    assert!(
        app.is_explorer_open(),
        "the chord did not reopen the explorer"
    );

    let name = directory
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .expect("the fixture has a name")
        .to_owned();
    let screen = until_drawn(&mut app, &woken, "beta.txt");
    assert!(
        screen.contains(&name),
        "the explorer reopened somewhere else:\n{screen}"
    );
}

#[test]
fn a_bracketed_paste_lands_in_the_listing_at_the_caret() {
    let (_directory, mut app) = project("tui-face-explorer-paste");
    let woken = wakes(&mut app);
    until_drawn(&mut app, &woken, "beta.txt");

    send(&mut app, b"\t");
    send(&mut app, b"\x1b[200~\none.txt\ntwo.txt\x1b[201~");
    assert_eq!(
        listing(&app),
        ["alpha.txt", "one.txt", "two.txt", "beta.txt"],
        "the paste did not reach the listing"
    );
    assert_eq!(app.editor().content(), "", "the paste reached the document");
}

#[test]
fn a_copy_in_the_listing_pastes_back_into_it() {
    let (_directory, mut app) = project("tui-face-explorer-clipboard");
    let woken = wakes(&mut app);
    until_drawn(&mut app, &woken, "beta.txt");

    send(&mut app, b"\t");
    // Shift+Home selects `alpha.txt`; Ctrl+C copies it, End and Ctrl+V put it back.
    send(&mut app, b"\x1b[1;2H");
    send(&mut app, b"\x03");
    send(&mut app, b"\x1b[F");
    send(&mut app, b"\x16");
    assert_eq!(listing(&app)[0], "alpha.txtalpha.txt");
}

#[test]
fn ctrl_f_in_the_listing_searches_the_listing_and_escape_gives_it_back() {
    let (_directory, mut app) = project("tui-face-explorer-search");
    let woken = wakes(&mut app);
    until_drawn(&mut app, &woken, "beta.txt");

    send(&mut app, b"\t");
    send(&mut app, b"\x06");
    assert!(app.is_searching(), "Ctrl+F opened no search");
    assert_eq!(app.search_target, SearchTarget::Explorer);

    type_text(&mut app, "beta");
    assert_eq!(
        listing(&app),
        ["alpha.txt", "beta.txt"],
        "the query was typed into the listing"
    );
    let matches = app
        .explorer
        .as_ref()
        .and_then(|explorer| explorer.buffer_editor())
        .map(|editor| editor.search_state().all_matches().len());
    assert_eq!(matches, Some(1), "the listing was not searched");
    let screen = frame(&mut app);
    assert!(
        screen.contains("beta"),
        "the search panel was not drawn:\n{screen}"
    );

    send(&mut app, b"\x1b");
    assert!(!app.is_searching(), "escape left the search open");
    assert!(
        app.explorer
            .as_ref()
            .is_some_and(|explorer| explorer.buffer_editor().is_some()),
        "closing the search left the listing too"
    );
}
