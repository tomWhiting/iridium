//! Injectable native IO and terminal delivery, with no real clipboard access.
use super::*;
use crate::clipboard::{Backend, ClipboardMode, Provider};
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
struct Pasteboard {
    text: String,
    fail_read: bool,
    fail_write: bool,
}
struct Stub(Rc<RefCell<Pasteboard>>);
impl Provider for Stub {
    fn read(&mut self) -> Result<String, String> {
        let state = self.0.borrow();
        if state.fail_read {
            Err("pasteboard unavailable".into())
        } else {
            Ok(state.text.clone())
        }
    }
    fn write(&mut self, text: &str) -> Result<(), String> {
        let mut state = self.0.borrow_mut();
        if state.fail_write {
            return Err("pasteboard refused write".into());
        }
        state.text = text.into();
        Ok(())
    }
}

fn native(app: &mut App) -> Rc<RefCell<Pasteboard>> {
    let state = Rc::new(RefCell::new(Pasteboard::default()));
    app.clipboard_backend = Backend::System(Box::new(Stub(Rc::clone(&state))));
    state
}

fn content(app: &mut App, text: &str) {
    app.workspace
        .active_editor_mut()
        .expect("editor")
        .set_content(text);
}

#[test]
fn native_copy_cut_paste_and_undo_preserve_exact_multiline_text() {
    let mut app = unnamed();
    let state = native(&mut app);
    let text = "cafe\u{301}\r\n猫\t👩🏽‍💻";
    content(&mut app, text);
    app.handle_input(&ctrl('a'));
    app.handle_input(&ctrl('x'));
    assert_eq!(state.borrow().text, text);
    assert_eq!(app.editor().expect("editor").content(), "");
    app.handle_input(&ctrl('z'));
    assert_eq!(app.editor().expect("editor").content(), text);
    state.borrow_mut().text = "other app".into();
    app.handle_input(&ctrl('v'));
    assert_eq!(app.editor().expect("editor").content(), "other app");
    app.handle_input(&ctrl('z'));
    assert_eq!(app.editor().expect("editor").content(), text);
}

#[test]
fn native_failures_leave_cut_and_paste_targets_untouched() {
    let mut app = unnamed();
    let state = native(&mut app);
    content(&mut app, "original");
    app.clipboard = "stale cache".into();
    state.borrow_mut().fail_write = true;
    app.handle_input(&ctrl('a'));
    app.handle_input(&ctrl('x'));
    assert_eq!(app.editor().expect("editor").content(), "original");
    assert_eq!(app.clipboard, "stale cache");
    state.borrow_mut().fail_read = true;
    app.handle_input(&ctrl('v'));
    assert_eq!(app.editor().expect("editor").content(), "original");
    assert!(
        frame(&mut app, 90, 12)
            .iter()
            .any(|line| line.contains("unavailable"))
    );
}

#[test]
fn native_field_cut_commits_only_after_delivery_and_paste_reads_current_system_value() {
    let mut app = unnamed();
    let state = native(&mut app);
    content(&mut app, "cat");
    app.handle_input(&ctrl('f'));
    app.handle_input(&TerminalInput::Paste("cat".into()));
    app.handle_input(&ctrl('a'));
    state.borrow_mut().fail_write = true;
    app.handle_input(&ctrl('x'));
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .query(),
        "cat"
    );
    state.borrow_mut().fail_write = false;
    app.handle_input(&ctrl('x'));
    assert_eq!(state.borrow().text, "cat");
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .query(),
        ""
    );
    state.borrow_mut().text = "dog".into();
    app.handle_input(&ctrl('v'));
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .query(),
        "dog"
    );
    assert_eq!(app.editor().expect("editor").content(), "cat");
}

#[test]
fn terminal_cut_waits_for_delivery_and_reports_unacknowledged_transport() {
    let mut app = unnamed();
    app.configure_clipboard(ClipboardMode::Terminal, false);
    content(&mut app, "cut me");
    app.handle_input(&ctrl('a'));
    app.handle_input(&ctrl('x'));
    assert_eq!(app.editor().expect("editor").content(), "cut me");
    let mut bytes = Vec::new();
    app.flush_terminal_clipboard(|text| {
        iridium_tui::driver::clipboard::write_clipboard(&mut bytes, text)
    })
    .expect("write");
    assert_eq!(bytes, b"\x1b]52;c;Y3V0IG1l\x1b\\");
    assert_eq!(app.editor().expect("editor").content(), "");
    assert!(
        frame(&mut app, 100, 12)
            .iter()
            .any(|line| line.contains("Sent to terminal"))
    );
    app.handle_input(&ctrl('z'));
    assert_eq!(app.editor().expect("editor").content(), "cut me");
}

#[test]
fn terminal_write_failure_preserves_document_and_search_field_cuts() {
    for search in [false, true] {
        let mut app = unnamed();
        app.configure_clipboard(ClipboardMode::Terminal, false);
        content(&mut app, "original");
        if search {
            app.handle_input(&ctrl('f'));
            app.handle_input(&TerminalInput::Paste("query".into()));
        }
        app.handle_input(&ctrl('a'));
        app.handle_input(&ctrl('x'));
        let error = app
            .flush_terminal_clipboard(|_| {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "closed",
                ))
            })
            .expect_err("write fails");
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
        assert!(app.pending_clipboard.is_none());
        assert_eq!(app.editor().expect("editor").content(), "original");
        if search {
            assert_eq!(
                app.workspace
                    .active_payload()
                    .expect("session")
                    .search
                    .query(),
                "query"
            );
        }
    }
}

#[test]
fn terminal_paste_never_substitutes_stale_internal_text() {
    let mut app = unnamed();
    app.configure_clipboard(ClipboardMode::Terminal, false);
    app.clipboard = "stale".into();
    content(&mut app, "original");
    app.handle_input(&ctrl('v'));
    assert_eq!(app.editor().expect("editor").content(), "original");
    assert!(
        frame(&mut app, 100, 12)
            .iter()
            .any(|line| line.contains("paste shortcut"))
    );
    app.handle_input(&TerminalInput::Paste("actual".into()));
    assert_eq!(app.editor().expect("editor").content(), "actualoriginal");
}

#[test]
fn pending_cut_refuses_changed_document_selection_and_field() {
    for change in 0..3 {
        let mut app = unnamed();
        app.configure_clipboard(ClipboardMode::Terminal, false);
        content(&mut app, "original");
        app.handle_input(&ctrl('a'));
        app.handle_input(&ctrl('x'));
        match change {
            0 => content(&mut app, "changed"),
            1 => {
                app.handle_input(&press(KeyCode::Left));
            },
            _ => {
                app.handle_input(&ctrl('n'));
            },
        }
        let before = app.editor().expect("editor").content();
        app.flush_terminal_clipboard(|_| Ok(())).expect("delivery");
        assert_eq!(app.editor().expect("editor").content(), before);
        assert_eq!(app.clipboard, "original");
        assert!(
            frame(&mut app, 100, 12)
                .iter()
                .any(|line| line.contains("cancelled"))
        );
    }
    let mut app = unnamed();
    app.configure_clipboard(ClipboardMode::Terminal, false);
    app.handle_input(&ctrl('f'));
    app.handle_input(&TerminalInput::Paste("query".into()));
    app.handle_input(&ctrl('a'));
    app.handle_input(&ctrl('x'));
    app.handle_input(&TerminalInput::Paste("changed".into()));
    app.flush_terminal_clipboard(|_| Ok(())).expect("delivery");
    assert_eq!(
        app.workspace
            .active_payload()
            .expect("session")
            .search
            .query(),
        "changed"
    );
}

#[test]
fn oversized_terminal_cut_is_rejected_before_any_delivery() {
    let mut app = unnamed();
    app.configure_clipboard(ClipboardMode::Terminal, false);
    let text = "x".repeat(iridium_tui::driver::clipboard::MAX_CLIPBOARD_BYTES + 1);
    content(&mut app, &text);
    app.handle_input(&ctrl('a'));
    app.handle_input(&ctrl('x'));
    assert!(app.pending_clipboard.is_none());
    assert_eq!(app.editor().expect("editor").content(), text);
    assert!(
        frame(&mut app, 100, 12)
            .iter()
            .any(|line| line.contains("exceeds 1 MiB"))
    );
}
