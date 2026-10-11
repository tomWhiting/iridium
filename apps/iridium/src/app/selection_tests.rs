//! Search focus must return to the document before a multicursor edit.

use crate::app::modal_tests::legacy_options;
use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_tui::cell::{CellContent, ColorDepth, Surface};
use iridium_tui::input::TerminalInput;

use super::{App, Flow};

fn press(app: &mut App, key: KeyCode, modifiers: Modifiers) {
    assert_eq!(
        app.handle_input(&TerminalInput::Key(KeyEvent {
            key,
            modifiers,
            is_repeat: false,
        })),
        Flow::Running
    );
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), Modifiers::none());
    }
}

fn frame_text(app: &mut App) -> String {
    let mut surface = Surface::new(100, 24, ColorDepth::TrueColor);
    app.render(&mut surface);
    let mut text = String::new();
    for row in 0..24 {
        for cell in surface.back().row(row).expect("row in the surface") {
            if let CellContent::Grapheme(grapheme) = cell.content() {
                grapheme.push_to(&mut text);
            }
        }
        text.push('\n');
    }
    text
}

#[test]
fn search_preview_select_all_and_edit_return_focus_to_the_document() {
    let mut app = App::with_config(
        legacy_options(),
        iridium_config::UserConfig::default(),
        None,
    )
    .expect("default configuration");
    app.resize(100, 24);
    app.workspace
        .active_editor_mut()
        .expect("active editor")
        .set_content("task_1 apple\ntask_22 pear\ntask_333 banana");
    let original = app
        .workspace
        .active_editor_mut()
        .expect("active editor")
        .content();
    let alt = Modifiers {
        alt: true,
        ..Modifiers::none()
    };
    press(&mut app, KeyCode::Char('f'), Modifiers::ctrl());
    assert!(app.search_open);
    press(&mut app, KeyCode::Char('r'), alt);
    type_text(&mut app, "task_[0-9]+");
    assert_eq!(
        app.workspace
            .active_editor_mut()
            .expect("active editor")
            .content(),
        original
    );
    let preview = frame_text(&mut app);
    assert!(preview.contains("1 of 3"), "{preview}");
    assert!(preview.contains("select all matches"), "{preview}");
    press(&mut app, KeyCode::Enter, alt);
    assert!(!app.search_open);
    assert!(
        !app.workspace
            .active_editor_mut()
            .expect("active editor")
            .search_state()
            .is_active
    );
    assert_eq!(
        app.workspace
            .active_editor_mut()
            .expect("active editor")
            .state()
            .cursor
            .cursor_count(),
        3
    );
    let selected = app
        .workspace
        .active_editor_mut()
        .expect("active editor")
        .state()
        .cursor
        .clone();
    let frame = frame_text(&mut app);
    assert!(frame.contains("3 selections"), "{frame}");
    type_text(&mut app, "job");
    assert_eq!(
        app.workspace
            .active_editor_mut()
            .expect("active editor")
            .content(),
        "job apple\njob pear\njob banana"
    );
    press(&mut app, KeyCode::Char('z'), Modifiers::ctrl());
    assert_eq!(
        app.workspace
            .active_editor_mut()
            .expect("active editor")
            .content(),
        original
    );
    assert_eq!(
        app.workspace
            .active_editor_mut()
            .expect("active editor")
            .state()
            .cursor,
        selected
    );
    press(&mut app, KeyCode::Char('f'), Modifiers::ctrl());
    assert!(app.search_open);
    assert_eq!(
        app.workspace
            .active_editor_mut()
            .expect("active editor")
            .search_match_count(),
        3
    );
}
