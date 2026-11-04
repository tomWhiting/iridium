//! Scrolling in the terminal face, driven the way the terminal drives it: keys
//! and wheel notches in, the kernel's view and caret out.

use iridium_editor::{EditorConfig, KeyCode, KeyEvent, Modifiers, Position};
use iridium_tui::cell::{ColorDepth, Surface};
use iridium_tui::input::{TerminalInput, WheelDirection, WheelScroll};

use super::{App, Flow};
use crate::cli::Options;

/// Text rows on the screen these tests use: 21 rows less the statusline.
const TEXT_ROWS: usize = 20;

/// A session over `text`, sized 80 by 21, with `config` as its settings.
fn session_with(text: &str, config: EditorConfig) -> App {
    let mut app = App::with_config(
        Options::default(),
        iridium_config::UserConfig {
            editor: config,
            ..iridium_config::UserConfig::default()
        },
        None,
    )
    .expect("the session opened");
    app.editor.set_content(text);
    app.resize(80, TEXT_ROWS + 1);
    app
}

fn session(text: &str) -> App {
    session_with(text, EditorConfig::default())
}

/// `count` numbered lines.
fn lines(count: usize) -> String {
    (0..count)
        .map(|index| format!("line {index}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, key: KeyCode, modifiers: Modifiers) {
    let event = TerminalInput::Key(KeyEvent {
        key,
        modifiers,
        is_repeat: false,
    });
    assert_eq!(app.handle_input(&event), Flow::Running);
}

fn wheel(app: &mut App, direction: WheelDirection, column: u16, row: u16) {
    let event = TerminalInput::Scroll(WheelScroll {
        direction,
        column,
        row,
        modifiers: Modifiers::none(),
    });
    assert_eq!(app.handle_input(&event), Flow::Running);
}

const ALT: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: true,
    meta: false,
    alt_graph: false,
};

#[test]
fn the_wheel_scrolls_the_document_and_leaves_the_caret() {
    let mut app = session(&lines(200));
    wheel(&mut app, WheelDirection::Down, 10, 5);
    wheel(&mut app, WheelDirection::Down, 10, 5);
    assert_eq!(app.editor().view_top(), 6, "three lines a notch");
    wheel(&mut app, WheelDirection::Up, 10, 5);
    assert_eq!(app.editor().view_top(), 3);
    assert_eq!(app.editor().cursor(), Position::new(0, 0));
}

#[test]
fn a_key_that_moves_nothing_keeps_the_scroll_and_a_motion_returns_to_the_caret() {
    let mut app = session(&lines(200));
    for _ in 0..10 {
        wheel(&mut app, WheelDirection::Down, 10, 5);
    }
    assert_eq!(app.editor().view_top(), 30);

    press(&mut app, KeyCode::Escape, Modifiers::none());
    assert_eq!(
        app.editor().view_top(),
        30,
        "nothing moved, so the view stays"
    );

    press(&mut app, KeyCode::Right, Modifiers::none());
    assert_eq!(app.editor().view_top(), 0, "a motion brings the caret back");
}

#[test]
fn the_wheel_does_nothing_outside_the_text_or_under_a_modal_panel() {
    let mut app = session(&lines(200));
    let status_row = u16::try_from(TEXT_ROWS).expect("small");
    wheel(&mut app, WheelDirection::Down, 10, status_row);
    assert_eq!(app.editor().view_top(), 0, "the statusline is not the text");
    wheel(&mut app, WheelDirection::Right, 10, 5);
    assert_eq!(app.editor().view_top(), 0, "nothing scrolls sideways yet");

    press(&mut app, KeyCode::Char('k'), Modifiers::ctrl());
    assert!(app.is_palette_open());
    wheel(&mut app, WheelDirection::Down, 10, 5);
    assert_eq!(app.editor().view_top(), 0, "the palette owns the screen");
}

#[test]
fn moving_past_the_bottom_of_the_screen_keeps_the_sticky_column() {
    // The defect this surface was built for: scrolling used to reset the
    // keyboard handler, so the first line past the edge lost the column.
    let text: String = (0..80)
        .map(|index| {
            if index % 2 == 0 {
                "a long line of text here"
            } else {
                "ab"
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let mut app = session(&text);
    app.editor.set_cursor(Position::new(0, 20));
    for line in 1..60 {
        press(&mut app, KeyCode::Down, Modifiers::none());
        let expected = if line % 2 == 0 { 20 } else { 2 };
        assert_eq!(
            app.editor().cursor(),
            Position::new(line, expected),
            "line {line}, view at {}",
            app.editor().view_top()
        );
    }
    assert!(app.editor().view_top() > 0, "the test must have scrolled");
}

#[test]
fn the_caret_keeps_its_margin_from_the_bottom_edge() {
    let mut app = session(&lines(200));
    for _ in 0..60 {
        press(&mut app, KeyCode::Down, Modifiers::none());
        let row = app.editor().caret_row().expect("the caret is followed");
        assert!(
            row <= TEXT_ROWS - 1 - 5,
            "row {row} is in the bottom margin"
        );
    }
}

#[test]
fn the_margin_is_read_from_the_settings() {
    let mut app = session_with(
        &lines(200),
        EditorConfig {
            scroll_margin: 0,
            ..EditorConfig::default()
        },
    );
    for _ in 0..TEXT_ROWS - 1 {
        press(&mut app, KeyCode::Down, Modifiers::none());
    }
    assert_eq!(app.editor().view_top(), 0);
    assert_eq!(app.editor().caret_row(), Some(TEXT_ROWS - 1));
    press(&mut app, KeyCode::Down, Modifiers::none());
    assert_eq!(app.editor().view_top(), 1);
}

#[test]
fn ctrl_arrows_scroll_a_line_and_leave_the_caret() {
    let mut app = session(&lines(200));
    app.editor.set_cursor(Position::new(8, 0));
    press(&mut app, KeyCode::Down, Modifiers::ctrl());
    press(&mut app, KeyCode::Down, Modifiers::ctrl());
    assert_eq!(app.editor().view_top(), 2);
    assert_eq!(app.editor().cursor(), Position::new(8, 0));
}

#[test]
fn paging_keeps_the_caret_on_its_row() {
    let mut app = session(&lines(500));
    for _ in 0..8 {
        press(&mut app, KeyCode::Down, Modifiers::none());
    }
    let row = app.editor().caret_row();
    press(&mut app, KeyCode::PageDown, Modifiers::none());
    assert_eq!(app.editor().cursor().line, 8 + TEXT_ROWS);
    assert_eq!(app.editor().caret_row(), row);
    press(&mut app, KeyCode::PageDown, ALT);
    assert_eq!(app.editor().cursor().line, 8 + TEXT_ROWS + TEXT_ROWS / 2);
    assert_eq!(app.editor().caret_row(), row);
}

#[test]
fn recentre_puts_the_caret_line_in_the_middle() {
    let mut app = session(&lines(500));
    app.editor.set_cursor(Position::new(100, 0));
    app.ensure_caret_visible();
    press(&mut app, KeyCode::Char('l'), Modifiers::ctrl());
    assert_eq!(app.editor().caret_row(), Some((TEXT_ROWS - 1) / 2));
}

#[test]
fn alt_m_toggles_mouse_capture_and_says_so() {
    let mut app = session("text");
    assert!(
        app.wants_mouse_capture(),
        "a session starts with the mouse on"
    );
    press(&mut app, KeyCode::Char('m'), ALT);
    assert!(!app.wants_mouse_capture());
    assert!(
        app.message()
            .is_some_and(|message| message.text().contains("mouse off")),
        "{:?}",
        app.message().map(super::prompt::Message::text)
    );
    press(&mut app, KeyCode::Char('m'), ALT);
    assert!(app.wants_mouse_capture());
}

#[test]
fn a_frame_after_a_wheel_scroll_draws_the_new_top_line() {
    let mut app = session(&lines(200));
    wheel(&mut app, WheelDirection::Down, 10, 5);
    let mut surface = Surface::new(80, TEXT_ROWS + 1, ColorDepth::TrueColor);
    app.render(&mut surface);
    let first: String = surface
        .back()
        .row(0)
        .expect("a first row")
        .iter()
        .filter_map(|cell| match cell.content() {
            iridium_tui::cell::CellContent::Grapheme(grapheme) => {
                let mut text = String::new();
                grapheme.push_to(&mut text);
                Some(text)
            },
            iridium_tui::cell::CellContent::Continuation => None,
        })
        .collect();
    assert!(first.contains("line 3"), "{first:?}");
}
