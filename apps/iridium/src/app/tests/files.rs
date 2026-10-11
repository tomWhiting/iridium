//! Files behavior in the terminal host.
use super::*;
use crate::app::modal_tests::legacy_options;

#[test]
fn opening_a_file_loads_its_text_and_its_language() {
    let directory = TempDir::new("app-open");
    let (app, _) = open(&directory, "lib.rs", "fn main() {}\n");
    assert_eq!(
        app.editor().expect("active editor").content(),
        "fn main() {}\n"
    );
    assert_eq!(
        app.editor().expect("active editor").language(),
        Some(Language::Rust)
    );
    assert!(!app.is_dirty());
}

#[test]
fn a_path_with_nothing_at_it_is_a_new_empty_buffer() {
    // `iridium notes.md` in a fresh directory must open, not fail.
    let directory = TempDir::new("app-new-file");
    let path = directory.path().join("notes.md");
    let app = App::new(Options {
        path: Some(path),
        ..legacy_options()
    })
    .expect("a file that is not there still opens");
    assert_eq!(app.editor().expect("active editor").content(), "");
    assert!(!app.is_dirty(), "an empty new buffer is not modified");
}

#[test]
#[cfg(unix)]
fn a_file_that_cannot_be_read_is_reported_before_the_terminal_opens() {
    // ⚠️ **This used to pass a directory**, on the reasoning that reading one
    // fails everywhere. #112d step 6 made a directory a project, so that path
    // now succeeds and the test would have gone on passing for the wrong
    // reason — it asserts an error, and one would still have come from
    // somewhere. A file with its read bit off is the unreadable thing now, and
    // it is unix-only because permissions are.
    //
    // `expect_err` is not available: `App` is not `Debug`, because the kernel's
    // `Editor` is not.
    use std::os::unix::fs::PermissionsExt as _;

    let directory = TempDir::new("app-unreadable");
    let path = directory.path().join("secret.txt");
    fs::write(&path, "unreadable").expect("the fixture could be written");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000))
        .expect("the fixture's permissions could be set");

    let outcome = App::new(Options {
        path: Some(path),
        ..legacy_options()
    });
    let Err(error) = outcome else {
        panic!("a file that cannot be read is not a document");
    };
    assert!(!error.to_string().is_empty());
}

#[test]
fn a_directory_argument_opens_it_as_a_project_with_the_tree_already_up() {
    // ⭐ R5, and the case Tom named: `iridium ~/project` comes up with the tree
    // on screen, not with an empty buffer and a chord to discover.
    let directory = TempDir::new("app-project");
    fs::write(directory.path().join("alpha.txt"), "a").expect("the fixture could be written");

    let mut app = App::new(Options {
        path: Some(directory.path().to_path_buf()),
        ..legacy_options()
    })
    .expect("a directory opens as a project");

    assert!(
        app.is_explorer_open(),
        "the tree was not up when the editor was"
    );
    assert!(
        app.file().is_none(),
        "a directory names no file, so the buffer is unnamed"
    );
    assert_eq!(app.editor().expect("active editor").content(), "");

    // The panel reads on a worker thread, so the listing lands some frames
    // later; the render is what polls it.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let screen = frame(&mut app, 80, 24).join("\n");
        if screen.contains("alpha.txt") {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the project's files never reached the screen:\n{screen}"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn a_path_that_does_not_exist_is_a_file_and_never_an_empty_project() {
    // The trap the `is_dir` decision could have introduced: a mistyped
    // directory name silently becoming a project would throw away the name the
    // user typed, and `iridium notes.md` in a fresh directory is the promise
    // the usage text makes.
    let directory = TempDir::new("app-typo");
    let app = App::new(Options {
        path: Some(directory.path().join("projekt")),
        ..legacy_options()
    })
    .expect("a path with nothing at it still opens");

    assert!(!app.is_explorer_open(), "a typo became a project");
    assert!(
        app.file().is_some(),
        "the name the user typed was not kept as a file to write to"
    );
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
    assert_eq!(app.editor().expect("active editor").content(), "hello");
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
        app.message()
            .is_some_and(crate::app::prompt::Message::is_error),
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
        app.editor().expect("active editor").language(),
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
    assert_eq!(app.editor().expect("active editor").content(), "xhello");
    assert_eq!(
        fs::read_to_string(&path).expect("the file is there"),
        "hello"
    );
}
