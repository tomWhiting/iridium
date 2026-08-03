//! Tests for the state machine.
//!
//! Every one of these drives [`App`] the way the terminal does — a
//! [`TerminalInput`] in, a [`Surface`] out — with no terminal anywhere, which is
//! the whole reason the event loop is the only thing that is not in this
//! library. What they are checking is the part that is this crate's own: which
//! key reaches which verb, what a save refuses, what the prompt line says. The
//! kernel's editing behaviour is the kernel's to test.

use std::fs;
use std::path::PathBuf;

use iridium_editor::{KeyCode, KeyEvent, Language, Modifiers};
use iridium_tui::cell::{CellBuffer, CellContent, ColorDepth, Surface};
use iridium_tui::input::TerminalInput;

use super::prompt::Prompt;
use super::{App, Flow};
use crate::cli::Options;
use crate::file::tests::TempDir;

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
    let app = App::new(Options {
        path: Some(path.clone()),
        ..Options::default()
    })
    .expect("the session opened");
    (app, path)
}

/// A session editing an unnamed buffer.
fn unnamed() -> App {
    App::new(Options::default()).expect("the session opened")
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

#[test]
fn opening_a_file_loads_its_text_and_its_language() {
    let directory = TempDir::new("app-open");
    let (app, _) = open(&directory, "lib.rs", "fn main() {}\n");
    assert_eq!(app.editor().content(), "fn main() {}\n");
    assert_eq!(app.editor().language(), Some(Language::Rust));
    assert!(!app.is_dirty());
}

#[test]
fn a_path_with_nothing_at_it_is_a_new_empty_buffer() {
    // `iridium notes.md` in a fresh directory must open, not fail.
    let directory = TempDir::new("app-new-file");
    let path = directory.path().join("notes.md");
    let app = App::new(Options {
        path: Some(path),
        ..Options::default()
    })
    .expect("a file that is not there still opens");
    assert_eq!(app.editor().content(), "");
    assert!(!app.is_dirty(), "an empty new buffer is not modified");
}

#[test]
fn a_file_that_cannot_be_read_is_reported_before_the_terminal_opens() {
    let directory = TempDir::new("app-unreadable");
    // A directory is not a file, and reading one fails on every platform this
    // runs on. `expect_err` is not available: `App` is not `Debug`, because the
    // kernel's `Editor` is not.
    let outcome = App::new(Options {
        path: Some(directory.path().to_path_buf()),
        ..Options::default()
    });
    let Err(error) = outcome else {
        panic!("a directory is not a document");
    };
    assert!(!error.to_string().is_empty());
}

#[test]
fn typing_marks_the_document_modified_and_undoing_it_does_not() {
    // The reason dirtiness is compared against the file's bytes rather than
    // counted: a document typed into and then emptied back is clean.
    let directory = TempDir::new("app-dirty");
    let (mut app, _) = open(&directory, "a.txt", "hello");
    assert!(!app.is_dirty());

    type_text(&mut app, "x");
    assert!(app.is_dirty());

    assert_eq!(app.handle_input(&press(KeyCode::Backspace)), Flow::Running);
    assert_eq!(app.editor().content(), "hello");
    assert!(!app.is_dirty(), "back at what the file holds");
}

#[test]
fn saving_writes_the_file_and_leaves_the_buffer_clean() {
    let directory = TempDir::new("app-save");
    let (mut app, path) = open(&directory, "a.txt", "hello");
    type_text(&mut app, "!");

    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "!hello"
    );
    assert!(!app.is_dirty());
    assert!(app.message().is_some_and(|message| !message.is_error()));
}

#[test]
fn a_file_that_changed_on_disk_is_refused_until_the_save_is_forced() {
    let directory = TempDir::new("app-conflict");
    let (mut app, path) = open(&directory, "a.txt", "hello");
    type_text(&mut app, "!");
    fs::write(&path, "someone else wrote this").expect("the fixture could be rewritten");

    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert!(
        app.message().is_some_and(super::prompt::Message::is_error),
        "a changed file must be reported"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "someone else wrote this",
        "the refused save must not have written anything"
    );

    assert_eq!(app.handle_input(&ctrl_alt('s')), Flow::Running);
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "!hello"
    );
}

#[test]
fn saving_an_unnamed_buffer_asks_for_a_name_rather_than_failing() {
    let directory = TempDir::new("app-save-as");
    let path = directory.path().join("named.rs");
    let mut app = unnamed();
    // No brackets: auto-pairing is on by default, and this test is about the
    // name, not about what the kernel does with a typed `(`.
    type_text(&mut app, "hello");

    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert!(matches!(app.prompt(), Some(Prompt::SaveAs(_))));

    let typed = path
        .to_str()
        .expect("the temporary path is UTF-8")
        .to_owned();
    assert_eq!(
        app.handle_input(&TerminalInput::Paste(typed)),
        Flow::Running
    );
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);

    assert!(app.prompt().is_none());
    assert_eq!(
        fs::read_to_string(&path).expect("the file was created"),
        "hello"
    );
    assert!(!app.is_dirty());
    assert_eq!(
        app.editor().language(),
        Some(Language::Rust),
        "the name it was given is what says what it is"
    );
}

#[test]
fn quitting_a_clean_document_leaves_at_once() {
    let directory = TempDir::new("app-quit-clean");
    let (mut app, _) = open(&directory, "a.txt", "hello");
    assert_eq!(app.handle_input(&ctrl('q')), Flow::Exit);
}

#[test]
fn quitting_with_unsaved_changes_asks_first() {
    let directory = TempDir::new("app-quit-dirty");
    let (mut app, _) = open(&directory, "a.txt", "hello");
    type_text(&mut app, "x");

    assert_eq!(app.handle_input(&ctrl('q')), Flow::Running);
    assert!(matches!(app.prompt(), Some(Prompt::Confirm { .. })));

    assert_eq!(
        app.handle_input(&press(KeyCode::Char('n'))),
        Flow::Running,
        "answering no stays"
    );
    assert!(app.prompt().is_none());

    assert_eq!(app.handle_input(&ctrl('q')), Flow::Running);
    assert_eq!(app.handle_input(&press(KeyCode::Char('y'))), Flow::Exit);
}

#[test]
fn a_prompt_swallows_the_keys_the_editor_would_otherwise_bind() {
    // The confirmation is a question. A save chord typed into it must not
    // save, and a printable key must not reach the document.
    let directory = TempDir::new("app-modal");
    let (mut app, path) = open(&directory, "a.txt", "hello");
    type_text(&mut app, "x");
    assert_eq!(app.handle_input(&ctrl('q')), Flow::Running);

    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert_eq!(app.handle_input(&press(KeyCode::Char('z'))), Flow::Running);

    assert!(app.prompt().is_some(), "the question is still being asked");
    assert_eq!(app.editor().content(), "xhello");
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "hello"
    );
}

#[test]
fn go_to_line_moves_the_caret() {
    let directory = TempDir::new("app-goto");
    let (mut app, _) = open(&directory, "a.txt", "one\ntwo\nthree\nfour\n");

    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    assert!(matches!(app.prompt(), Some(Prompt::GotoLine(_))));

    type_text(&mut app, "3");
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);

    assert!(app.prompt().is_none());
    assert_eq!(
        app.editor().cursor().line,
        2,
        "line three, counting from one"
    );
}

#[test]
fn a_line_past_the_end_lands_on_the_last_one_and_says_so() {
    let directory = TempDir::new("app-goto-past");
    let (mut app, _) = open(&directory, "a.txt", "one\ntwo\n");

    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    type_text(&mut app, "900");
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);

    let last = app.editor().state().document.line_count() - 1;
    assert_eq!(app.editor().cursor().line, last);
    assert!(app.message().is_some(), "a clamped jump must say so");
}

#[test]
fn the_command_line_can_place_the_caret_at_start_up() {
    let directory = TempDir::new("app-line-option");
    let path = directory.path().join("a.txt");
    fs::write(&path, "one\ntwo\nthree\n").expect("the fixture could be written");
    let app = App::new(Options {
        path: Some(path),
        line: Some(2),
        ..Options::default()
    })
    .expect("the session opened");
    assert_eq!(app.editor().cursor().line, 1);
}

#[test]
fn a_read_only_document_refuses_both_the_edit_and_the_save() {
    let directory = TempDir::new("app-read-only");
    let path = directory.path().join("a.txt");
    fs::write(&path, "hello").expect("the fixture could be written");
    let mut app = App::new(Options {
        path: Some(path.clone()),
        read_only: true,
        ..Options::default()
    })
    .expect("the session opened");

    type_text(&mut app, "x");
    assert_eq!(
        app.editor().content(),
        "hello",
        "the kernel refused the edit"
    );

    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert!(
        app.message().is_some_and(super::prompt::Message::is_error),
        "a save that could only write a stale buffer is refused"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "hello"
    );
}

#[test]
fn reloading_over_unsaved_changes_asks_first() {
    let directory = TempDir::new("app-reload");
    let (mut app, path) = open(&directory, "a.txt", "hello");
    type_text(&mut app, "x");
    fs::write(&path, "what is really there").expect("the fixture could be rewritten");

    assert_eq!(app.handle_input(&press(KeyCode::F5)), Flow::Running);
    assert!(matches!(app.prompt(), Some(Prompt::Confirm { .. })));

    assert_eq!(app.handle_input(&press(KeyCode::Char('y'))), Flow::Running);
    assert_eq!(app.editor().content(), "what is really there");
    assert!(!app.is_dirty());
}

#[test]
fn reloading_a_clean_document_asks_nothing() {
    let directory = TempDir::new("app-reload-clean");
    let (mut app, path) = open(&directory, "a.txt", "hello");
    fs::write(&path, "changed underneath").expect("the fixture could be rewritten");

    assert_eq!(app.handle_input(&press(KeyCode::F5)), Flow::Running);
    assert!(app.prompt().is_none());
    assert_eq!(app.editor().content(), "changed underneath");
}

#[test]
fn a_paste_arrives_whole_rather_than_as_key_presses() {
    let mut app = unnamed();
    assert_eq!(
        app.handle_input(&TerminalInput::Paste("two\nlines".to_owned())),
        Flow::Running
    );
    assert_eq!(app.editor().content(), "two\nlines");
}

#[test]
fn folding_hides_lines_and_going_to_one_reveals_it_again() {
    let directory = TempDir::new("app-folds");
    let (mut app, _) = open(
        &directory,
        "lib.rs",
        "fn main() {\n    let a = 1;\n    let b = 2;\n}\n",
    );
    assert!(
        app.editor().fold_region_count() > 0,
        "the Rust grammar found nothing to fold"
    );

    assert_eq!(app.handle_input(&ctrl_alt('f')), Flow::Running);
    assert!(app.editor().folded_count() > 0);
    assert!(app.editor().is_line_hidden(1), "the body is hidden");

    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    type_text(&mut app, "2");
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);

    assert!(
        !app.editor().is_line_hidden(1),
        "a jump to a folded line must reveal it"
    );
    assert_eq!(app.editor().cursor().line, 1);
}

#[test]
fn folding_never_leaves_the_caret_on_a_line_nobody_can_see() {
    let directory = TempDir::new("app-fold-caret");
    let (mut app, _) = open(
        &directory,
        "lib.rs",
        "fn main() {\n    let a = 1;\n    let b = 2;\n}\n",
    );

    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    type_text(&mut app, "2");
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);
    assert_eq!(app.editor().cursor().line, 1);

    assert_eq!(app.handle_input(&ctrl_alt('f')), Flow::Running);
    assert!(app.editor().is_line_hidden(1), "the body is folded away");
    assert_eq!(
        app.editor().cursor().line,
        0,
        "the caret was lifted onto the fold's header"
    );
}

#[test]
fn unfolding_everything_is_a_key_of_its_own() {
    let directory = TempDir::new("app-unfold-all");
    let (mut app, _) = open(
        &directory,
        "lib.rs",
        "fn main() {\n    let a = 1;\n    let b = 2;\n}\n",
    );

    assert_eq!(app.handle_input(&ctrl_alt('f')), Flow::Running);
    assert!(app.editor().folded_count() > 0);

    assert_eq!(app.handle_input(&alt('u')), Flow::Running);
    assert_eq!(app.editor().folded_count(), 0);
    assert!(!app.editor().is_line_hidden(1));
}

#[test]
fn the_fold_key_takes_the_region_around_the_caret() {
    let directory = TempDir::new("app-fold-toggle");
    let (mut app, _) = open(
        &directory,
        "lib.rs",
        "fn main() {\n    let a = 1;\n    let b = 2;\n}\n",
    );

    // The caret is on the body, not on the line the region starts at.
    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    type_text(&mut app, "3");
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);

    assert_eq!(app.handle_input(&alt('f')), Flow::Running);
    assert!(
        app.editor().folded_count() > 0,
        "the region around the caret folded"
    );
}

#[test]
fn the_statusline_names_the_file_and_marks_unsaved_changes() {
    let directory = TempDir::new("app-statusline");
    let (mut app, _) = open(&directory, "notes.md", "hello\n");

    let clean = frame(&mut app, 40, 6);
    let status = clean.last().expect("there is a statusline").clone();
    assert!(status.contains("notes.md"), "{status:?}");
    assert!(!status.contains("[+]"), "{status:?}");

    type_text(&mut app, "x");
    let dirty = frame(&mut app, 40, 6);
    let status = dirty.last().expect("there is a statusline").clone();
    assert!(status.contains("[+]"), "{status:?}");
}

#[test]
fn an_open_prompt_takes_the_statusline_row() {
    let directory = TempDir::new("app-prompt-row");
    let (mut app, _) = open(&directory, "notes.md", "hello\n");
    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    type_text(&mut app, "12");

    let rows = frame(&mut app, 40, 6);
    let status = rows.last().expect("there is a statusline");
    assert!(status.starts_with("Go to line: 12"), "{status:?}");
    assert!(!status.contains("notes.md"), "{status:?}");
}

#[test]
fn a_message_is_shown_until_the_next_key() {
    let directory = TempDir::new("app-message");
    let (mut app, _) = open(&directory, "a.txt", "hello");
    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);

    let rows = frame(&mut app, 40, 6);
    let status = rows.last().expect("there is a statusline");
    assert!(status.starts_with("wrote a.txt"), "{status:?}");

    type_text(&mut app, "x");
    assert!(app.message().is_none(), "the next key clears it");
}

#[test]
fn the_document_text_reaches_the_screen() {
    let directory = TempDir::new("app-paint");
    let (mut app, _) = open(&directory, "a.txt", "alpha\nbeta\n");
    let rows = frame(&mut app, 40, 6);
    assert!(rows[0].contains("alpha"), "{:?}", rows[0]);
    assert!(rows[1].contains("beta"), "{:?}", rows[1]);
}

#[test]
fn a_search_panel_opens_and_closes_without_disturbing_the_document() {
    let directory = TempDir::new("app-search");
    let (mut app, _) = open(&directory, "a.txt", "alpha\nbeta\nalpha\n");

    assert_eq!(app.handle_input(&ctrl('f')), Flow::Running);
    assert!(app.is_searching());

    type_text(&mut app, "beta");
    assert_eq!(
        app.editor().content(),
        "alpha\nbeta\nalpha\n",
        "typing into the panel is not typing into the document"
    );
    assert_eq!(app.editor().search_match_count(), 1);

    assert_eq!(app.handle_input(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.is_searching());
}

#[test]
fn a_save_chord_still_works_while_the_search_panel_is_open() {
    // The panel reports what it does not bind, so the host's verbs stay live.
    let directory = TempDir::new("app-search-save");
    let (mut app, path) = open(&directory, "a.txt", "alpha\n");
    type_text(&mut app, "x");

    assert_eq!(app.handle_input(&ctrl('f')), Flow::Running);
    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "xalpha\n"
    );
}

#[test]
fn a_resize_is_written_into_the_kernels_viewport() {
    let directory = TempDir::new("app-resize");
    let (mut app, _) = open(&directory, "a.txt", "one\ntwo\nthree\n");
    app.resize(80, 25);

    let viewport = &app.editor().state().viewport;
    assert!(
        (viewport.line_height - 1.0).abs() < f32::EPSILON,
        "one cell is one unit"
    );
    // One row is the statusline's, so the document has the rest.
    assert_eq!(viewport.visible_lines, 24);
}

#[test]
fn focus_events_change_nothing_in_the_document() {
    let mut app = unnamed();
    type_text(&mut app, "abc");
    assert_eq!(app.handle_input(&TerminalInput::FocusLost), Flow::Running);
    assert_eq!(app.handle_input(&TerminalInput::FocusGained), Flow::Running);
    assert_eq!(app.editor().content(), "abc");
}

#[test]
fn the_palette_is_modal_and_escape_gives_the_document_back() {
    let mut app = unnamed();
    assert_eq!(app.handle_input(&ctrl('k')), Flow::Running);
    assert!(app.is_palette_open());

    // Typing while the palette is open aims at the query, not the document.
    type_text(&mut app, "x");
    assert_eq!(app.editor().content(), "");

    assert_eq!(app.handle_input(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.is_palette_open());
    type_text(&mut app, "x");
    assert_eq!(app.editor().content(), "x");
}

#[test]
fn a_kernel_command_runs_from_the_palette() {
    let mut app = unnamed();
    type_text(&mut app, "abc");
    assert_eq!(app.handle_input(&ctrl('k')), Flow::Running);
    type_text(&mut app, "select all");
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);
    assert!(
        !app.is_palette_open(),
        "running a command closes the palette"
    );
    let selection = &app.editor().state().cursor.primary;
    assert!(
        !selection.is_collapsed(),
        "Select All ran against the document"
    );
    assert!(
        app.message().is_none(),
        "a command that ran reports nothing: {:?}",
        app.message()
    );
}

#[test]
fn a_face_command_runs_from_the_palette() {
    // `Quit` is this face's own host command; resolving it through the
    // palette must reach the same dispatch the keypress does.
    let mut app = unnamed();
    assert_eq!(app.handle_input(&ctrl('k')), Flow::Running);
    type_text(&mut app, "quit");
    assert_eq!(
        app.handle_input(&press(KeyCode::Enter)),
        Flow::Exit,
        "Quit from the palette must leave, exactly as Ctrl+Q does"
    );
}

#[test]
fn the_history_panel_toggles_and_is_modal() {
    let mut app = unnamed();
    type_text(&mut app, "ab");
    assert_eq!(app.handle_input(&ctrl_alt('h')), Flow::Running);
    assert!(app.is_history_open());

    // Typing while the panel is open must not grow the tree being read.
    type_text(&mut app, "x");
    assert_eq!(app.editor().content(), "ab");

    assert_eq!(app.handle_input(&ctrl_alt('h')), Flow::Running);
    assert!(!app.is_history_open(), "the same chord closes it");
}

#[test]
fn jumping_from_the_history_panel_walks_real_document_states() {
    let mut app = unnamed();
    type_text(&mut app, "ab");
    assert_eq!(app.handle_input(&ctrl_alt('h')), Flow::Running);

    // Up from the current row is an earlier state; Enter takes the document
    // there while the panel stays open.
    assert_eq!(app.handle_input(&press(KeyCode::Up)), Flow::Running);
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);
    assert_eq!(app.editor().content(), "");
    assert!(app.is_history_open(), "a jump keeps the panel open");

    // And back down: the branch just left is still there.
    assert_eq!(app.handle_input(&press(KeyCode::Down)), Flow::Running);
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);
    assert_eq!(app.editor().content(), "ab");
}

#[test]
fn the_history_key_opens_the_undo_tree_rather_than_reporting_a_dead_key() {
    // `Ctrl+Alt+H` is the kernel's binding for `history.togglePanel`. A face
    // that answers it with "nothing runs it" leaves the undo tree — the
    // feature that makes work unlosable — navigable only blind.
    let mut app = unnamed();
    assert_eq!(app.handle_input(&ctrl_alt('h')), Flow::Running);
    assert!(
        app.message().is_none(),
        "the history key must open the panel, not report an unrun command: {:?}",
        app.message()
    );
}

#[test]
fn the_palette_key_opens_a_palette_rather_than_reporting_a_dead_key() {
    // `Ctrl+K` is the kernel's default binding for `palette.open`. A face
    // that answers it with "nothing runs it" has 41 palette-only commands
    // unreachable, which is the single biggest gap the walkthrough names.
    let mut app = unnamed();
    assert_eq!(app.handle_input(&ctrl('k')), Flow::Running);
    assert!(
        app.message().is_none(),
        "the palette key must open the palette, not report an unrun command: {:?}",
        app.message()
    );
}
