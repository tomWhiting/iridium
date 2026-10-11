//! Exercise divider capture, shared geometry and keyboard controls together.
use super::sidebar::Focus;
use super::workbench_tests::{app, key, settle};
use super::{App, workbench};
use iridium_editor::{KeyCode, Modifiers};
use iridium_file::test_support::TempDir;
use iridium_tui::cell::{ColorDepth, Surface};
use iridium_tui::frame::Palette;
use iridium_tui::input::{MouseButton, MouseEvent, MouseEventKind, MouseModifiers, TerminalInput};

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    app.handle_input(&TerminalInput::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: MouseModifiers::empty(),
    }));
}

#[test]
fn control_b_and_control_e_toggle_visibility_and_focus_from_both_panes() {
    let dir = TempDir::new("sidebar-keys");
    let mut app = app(&dir);
    key(&mut app, KeyCode::Char('b'), Modifiers::ctrl());
    settle(&mut app);
    assert_eq!(app.sidebar_columns(), 32);
    assert!(app.focus == Focus::Explorer);
    key(&mut app, KeyCode::Char('e'), Modifiers::ctrl());
    assert!(app.focus == Focus::Document);
    key(&mut app, KeyCode::Char('e'), Modifiers::ctrl());
    assert!(app.focus == Focus::Explorer);
    key(&mut app, KeyCode::Char('b'), Modifiers::ctrl());
    assert!(app.explorer.is_none());
    key(&mut app, KeyCode::Char('e'), Modifiers::ctrl());
    settle(&mut app);
    assert_eq!(app.sidebar_columns(), 32);
    assert!(app.focus == Focus::Explorer);
}

#[test]
fn dragging_divider_resizes_without_opening_files_or_selecting_document_text() {
    let dir = TempDir::new("sidebar-drag");
    std::fs::write(dir.path().join("file.rs"), "fn main() {}\n").expect("fixture");
    let mut app = app(&dir);
    app.run_palette_command(&workbench::SIDEBAR);
    settle(&mut app);
    let selection = app.editor().expect("editor").state().cursor.clone();
    let tabs = app.workspace.document_count();
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 31, 2);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 43, 2);
    assert_eq!(app.sidebar_columns(), 44);
    // Release over the tab strip still belongs to the captured divider.
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 43, 22);
    assert!(app.sidebar_drag.is_none());
    assert!(app.mouse_anchor.is_none());
    assert_eq!(app.workspace.document_count(), tabs);
    assert_eq!(app.editor().expect("editor").state().cursor, selection);
    assert_eq!(app.mouse_layout().expect("layout").sidebar_columns, 44);
    let mut surface = Surface::new(100, 24, ColorDepth::TrueColor);
    app.render(&mut surface);
    let divider = surface.back().get(43, 5).expect("divider");
    let palette = Palette::from_theme(app.editor().expect("editor").get_theme());
    assert_eq!(divider.style(), palette.overlay_quiet());
    assert_ne!(divider.style(), palette.overlay());
    app.run_palette_command(&workbench::SIDEBAR);
    app.run_palette_command(&workbench::SIDEBAR);
    settle(&mut app);
    assert_eq!(app.sidebar_columns(), 44);
}

#[test]
fn resizing_clamps_and_recovers_after_terminal_shrink_and_cancels_on_focus_loss() {
    let dir = TempDir::new("sidebar-bounds");
    let mut app = app(&dir);
    app.run_palette_command(&workbench::SIDEBAR);
    settle(&mut app);
    let resize = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::none()
    };
    key(&mut app, KeyCode::Right, resize);
    assert_eq!(app.sidebar_columns(), 36);
    key(&mut app, KeyCode::Left, resize);
    assert_eq!(app.sidebar_columns(), 32);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 31, 1);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 99, 1);
    assert_eq!(app.sidebar_columns(), 50);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 0, 1);
    assert_eq!(app.sidebar_columns(), 20);
    app.run_palette_command(&workbench::WIDER);
    assert_eq!(app.sidebar_columns(), 24);
    app.resize(30, 24);
    assert_eq!(app.sidebar_columns(), 0);
    app.resize(100, 24);
    assert_eq!(app.sidebar_columns(), 24);
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 23, 1);
    app.handle_input(&TerminalInput::FocusLost);
    mouse(&mut app, MouseEventKind::Drag(MouseButton::Left), 40, 1);
    assert_eq!(app.sidebar_columns(), 24);
    assert!(app.mouse_anchor.is_none());
}
