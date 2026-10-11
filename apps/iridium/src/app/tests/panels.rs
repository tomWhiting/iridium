//! Panels behavior in the terminal host.
use super::*;

#[test]
fn the_palette_is_modal_and_escape_gives_the_document_back() {
    let mut app = unnamed();
    assert_eq!(app.handle_input(&ctrl('k')), Flow::Running);
    assert!(app.is_palette_open());

    // Typing while the palette is open aims at the query, not the document.
    type_text(&mut app, "x");
    assert_eq!(app.editor().expect("active editor").content(), "");

    assert_eq!(app.handle_input(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.is_palette_open());
    type_text(&mut app, "x");
    assert_eq!(app.editor().expect("active editor").content(), "x");
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
    let selection = &app.editor().expect("active editor").state().cursor.primary;
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
    assert_eq!(app.editor().expect("active editor").content(), "ab");

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
    assert_eq!(app.editor().expect("active editor").content(), "");
    assert!(app.is_history_open(), "a jump keeps the panel open");

    // And back down: the branch just left is still there.
    assert_eq!(app.handle_input(&press(KeyCode::Down)), Flow::Running);
    assert_eq!(app.handle_input(&press(KeyCode::Enter)), Flow::Running);
    assert_eq!(app.editor().expect("active editor").content(), "ab");
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

#[test]
fn ctrl_alt_e_opens_the_file_explorer_and_closes_it_again() {
    // ⭐ The chord and the command id are the kernel's, not this face's, so
    // this is also the test that the terminal inherited both for free.
    let directory = TempDir::new("tui-app-explorer-toggle");
    let (mut app, _) = open(&directory, "notes.md", "a file\n");

    assert_eq!(app.handle_input(&ctrl_alt('e')), Flow::Running);
    assert!(app.is_explorer_open(), "Ctrl+Alt+E did not open the panel");

    assert_eq!(app.handle_input(&ctrl_alt('e')), Flow::Running);
    assert!(!app.is_explorer_open(), "the toggle did not put it away");
}

#[test]
fn the_explorer_is_modal_so_typing_does_not_reach_the_document() {
    // The panel gives every printable character to its filter. A key that fell
    // through would be text typed into a document nobody was looking at.
    let directory = TempDir::new("tui-app-explorer-modal");
    let (mut app, _) = open(&directory, "notes.md", "");
    assert_eq!(app.handle_input(&ctrl_alt('e')), Flow::Running);

    type_text(&mut app, "zzz");
    assert_eq!(
        app.editor().expect("active editor").content(),
        "",
        "the filter query reached the document"
    );

    // ⭐ **Two presses, and the first one is not a bug.** The shared panel
    // takes the query back before it takes itself away — closing a panel
    // somebody has just typed into would throw away the narrowing and the
    // panel in one press. The terminal inherits that with the panel; nothing
    // in this face decides it, and this assertion is what would notice if the
    // rule changed on the other side of the crate boundary.
    assert_eq!(app.handle_input(&press(KeyCode::Escape)), Flow::Running);
    assert!(
        app.is_explorer_open(),
        "the first escape closed the panel instead of clearing its query"
    );

    assert_eq!(app.handle_input(&press(KeyCode::Escape)), Flow::Running);
    assert!(!app.is_explorer_open());
    type_text(&mut app, "z");
    assert_eq!(
        app.editor().expect("active editor").content(),
        "z",
        "the document never came back"
    );
}

#[test]
fn the_explorer_draws_the_directorys_files() {
    let directory = TempDir::new("tui-app-explorer-draws");
    let (mut app, _) = open(&directory, "notes.md", "");
    fs::write(directory.path().join("other.txt"), "x").expect("the fixture was written");
    assert_eq!(app.handle_input(&ctrl_alt('e')), Flow::Running);

    // The listing lands on a worker thread, and `render` is what collects it.
    // Several frames rather than one, for the same reason the panel's own
    // tests wait: a single frame proves nothing about a read that has not
    // finished, and asserting on one would be a flake by construction.
    let mut screen = String::new();
    for _ in 0..200 {
        screen = frame(&mut app, 80, 24).join("\n");
        if screen.contains("other.txt") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(
        screen.contains("other.txt"),
        "the panel never drew the directory it was opened on:\n{screen}"
    );
}

#[test]
fn opening_a_file_keeps_unsaved_changes_in_the_original_tab() {
    let directory = TempDir::new("tui-app-explorer-open-dirty");
    let (mut app, first) = open(&directory, "first.md", "one\n");
    let second = directory.path().join("second.md");
    fs::write(&second, "two\n").expect("fixture");
    type_text(&mut app, "x");
    assert!(app.is_dirty());
    assert_eq!(app.open_file(&second), Flow::Running);
    assert!(app.prompt.is_none());
    assert_eq!(app.editor().expect("active editor").content(), "two\n");
    assert_eq!(app.workspace.document_count(), 2);
    assert_eq!(app.open_file(&first), Flow::Running);
    assert_eq!(app.editor().expect("active editor").content(), "xone\n");
    assert!(app.is_dirty());
}

#[test]
fn opening_a_file_with_nothing_unsaved_just_opens_it() {
    let directory = TempDir::new("tui-app-explorer-open-clean");
    let (mut app, _) = open(&directory, "first.md", "one\n");
    let second = directory.path().join("second.md");
    fs::write(&second, "two\n").expect("the fixture was written");

    assert_eq!(app.open_file(&second), Flow::Running);
    assert!(app.prompt.is_none(), "a clean document was asked about");
    assert_eq!(app.editor().expect("active editor").content(), "two\n");
}

#[test]
fn opening_a_file_no_language_claims_stops_highlighting_the_last_one() {
    // #39's defect, in this face: a session that opened a `.rs` and then a
    // name nothing claims would keep highlighting the second as Rust, because
    // nothing had ever said stop.
    let directory = TempDir::new("tui-app-explorer-open-language");
    let (mut app, _) = open(&directory, "lib.rs", "fn main() {}\n");
    assert_eq!(
        app.editor().expect("active editor").language(),
        Some(Language::Rust)
    );

    let plain = directory.path().join("NOTES");
    fs::write(&plain, "just words\n").expect("the fixture was written");
    assert_eq!(app.open_file(&plain), Flow::Running);
    assert_eq!(
        app.editor().expect("active editor").language(),
        None,
        "the previous file's language survived the open"
    );
}
