//! Editing behavior in the terminal host.
use super::*;
use crate::app::modal_tests::legacy_options;

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
        app.editor().expect("active editor").cursor().line,
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

    let last = app
        .editor()
        .expect("active editor")
        .state()
        .document
        .line_count()
        - 1;
    assert_eq!(app.editor().expect("active editor").cursor().line, last);
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
        ..legacy_options()
    })
    .expect("the session opened");
    assert_eq!(app.editor().expect("active editor").cursor().line, 1);
}

#[test]
fn a_read_only_document_refuses_both_the_edit_and_the_save() {
    let directory = TempDir::new("app-read-only");
    let path = directory.path().join("a.txt");
    fs::write(&path, "hello").expect("the fixture could be written");
    let mut app = App::new(Options {
        path: Some(path.clone()),
        read_only: true,
        ..legacy_options()
    })
    .expect("the session opened");

    type_text(&mut app, "x");
    assert_eq!(
        app.editor().expect("active editor").content(),
        "hello",
        "the kernel refused the edit"
    );

    assert_eq!(app.handle_input(&ctrl('s')), Flow::Running);
    assert!(
        app.message()
            .is_some_and(crate::app::prompt::Message::is_error),
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
    assert_eq!(
        app.editor().expect("active editor").content(),
        "what is really there"
    );
    assert!(!app.is_dirty());
}

#[test]
fn reloading_a_clean_document_asks_nothing() {
    let directory = TempDir::new("app-reload-clean");
    let (mut app, path) = open(&directory, "a.txt", "hello");
    fs::write(&path, "changed underneath").expect("the fixture could be rewritten");

    assert_eq!(app.handle_input(&press(KeyCode::F5)), Flow::Running);
    assert!(app.prompt().is_none());
    assert_eq!(
        app.editor().expect("active editor").content(),
        "changed underneath"
    );
}

#[test]
fn a_paste_arrives_whole_rather_than_as_key_presses() {
    let mut app = unnamed();
    assert_eq!(
        app.handle_input(&TerminalInput::Paste("two\nlines".to_owned())),
        Flow::Running
    );
    assert_eq!(app.editor().expect("active editor").content(), "two\nlines");
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
        app.editor().expect("active editor").fold_region_count() > 0,
        "the Rust grammar found nothing to fold"
    );

    assert_eq!(app.handle_input(&ctrl_alt('f')), Flow::Running);
    assert!(app.editor().expect("active editor").folded_count() > 0);
    assert!(
        app.editor().expect("active editor").is_line_hidden(1),
        "the body is hidden"
    );

    assert_eq!(app.handle_input(&ctrl('g')), Flow::Running);
    type_text(&mut app, "2");
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);

    assert!(
        !app.editor().expect("active editor").is_line_hidden(1),
        "a jump to a folded line must reveal it"
    );
    assert_eq!(app.editor().expect("active editor").cursor().line, 1);
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
    assert_eq!(app.editor().expect("active editor").cursor().line, 1);

    assert_eq!(app.handle_input(&ctrl_alt('f')), Flow::Running);
    assert!(
        app.editor().expect("active editor").is_line_hidden(1),
        "the body is folded away"
    );
    assert_eq!(
        app.editor().expect("active editor").cursor().line,
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
    assert!(app.editor().expect("active editor").folded_count() > 0);

    assert_eq!(app.handle_input(&alt('u')), Flow::Running);
    assert_eq!(app.editor().expect("active editor").folded_count(), 0);
    assert!(!app.editor().expect("active editor").is_line_hidden(1));
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
        app.editor().expect("active editor").folded_count() > 0,
        "the region around the caret folded"
    );
}
