//! State that must neither leak to another tab nor disappear while hidden.
use super::*;
use iridium_editor::search::SearchOptions;

#[test]
fn multi_selection_and_undo_branches_survive_switching_and_closing_a_neighbour() {
    let dir = TempDir::new("terminal-tab-multicursors");
    let a = write(&dir, "a", "foo foo foo");
    let b = write(&dir, "b", "other");
    let mut app = app(&a);
    let editor = app.workspace.active_editor_mut().expect("active");
    editor
        .find("foo", &SearchOptions::default())
        .expect("query");
    assert_eq!(editor.select_search_matches(), Ok(3));
    editor.paste("one");
    assert!(editor.undo());
    editor.paste("two");
    assert!(editor.undo());
    let cursors = editor.state().cursor.clone();
    let branches = editor.history_branches();
    assert_eq!(branches.len(), 2);
    app.open_file(&b);
    key(&mut app, 'w', Modifiers::ctrl());
    let editor = app.workspace.active_editor_mut().expect("original tab");
    assert_eq!(editor.state().cursor, cursors);
    assert_eq!(editor.history_branches().len(), 2);
    assert!(editor.redo_branch(0));
    assert_eq!(editor.content(), "one one one");
}

#[test]
fn search_queries_are_local_to_each_document() {
    let dir = TempDir::new("terminal-tab-queries");
    let a = write(&dir, "a", "apple pear");
    let b = write(&dir, "b", "apple pear");
    let mut app = app(&a);
    key(&mut app, 'f', Modifiers::ctrl());
    for c in "apple".chars() {
        key(&mut app, c, Modifiers::none());
    }
    app.open_file(&b);
    assert!(!app.is_searching());
    key(&mut app, 'f', Modifiers::ctrl());
    for c in "pear".chars() {
        key(&mut app, c, Modifiers::none());
    }
    key(
        &mut app,
        ',',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    assert!(!app.is_searching());
    key(&mut app, 'f', Modifiers::ctrl());
    let session = app.workspace.active_payload().expect("active");
    assert_eq!(session.search.query(), "apple");
    assert_eq!(app.editor().expect("active").search_match_count(), 1);
}

#[test]
fn project_root_stays_at_the_invocation_directory_after_opening_a_nested_file() {
    let dir = TempDir::new("terminal-tab-project-root");
    let child = dir.path().join("child");
    fs::create_dir(&child).expect("directory");
    let nested = child.join("a.rs");
    fs::write(&nested, "fn main() {}").expect("file");
    let mut app = app(dir.path());
    assert!(app.is_explorer_open());
    app.open_file(&nested);
    assert_eq!(app.explorer_root().expect("root").path, dir.path());
}

#[test]
fn same_revision_documents_render_with_their_own_highlight_cache() {
    let dir = TempDir::new("terminal-tab-highlights");
    let a = write(&dir, "a.rs", "// this is a comment\n");
    let b = write(&dir, "b.rs", "fn main() { let x = 1; }\n");
    let mut app = app(&a);
    let mut screen = Surface::new(60, 10, ColorDepth::TrueColor);
    app.render(&mut screen);
    app.open_file(&b);
    app.render(&mut screen);
    let mut fresh = super::app(&b);
    let mut reference = Surface::new(60, 10, ColorDepth::TrueColor);
    fresh.render(&mut reference);
    assert_eq!(screen.back().row(0), reference.back().row(0));
    key(
        &mut app,
        ',',
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    app.render(&mut screen);
    let mut fresh = super::app(&a);
    fresh.render(&mut reference);
    assert_eq!(screen.back().row(0), reference.back().row(0));
}

#[test]
fn tab_and_search_geometry_stays_inside_every_small_screen() {
    let dir = TempDir::new("terminal-tab-tiny");
    let a = write(&dir, "漢字👩‍💻.rs", "text\n");
    let mut app = app(&a);
    key(&mut app, 'f', Modifiers::ctrl());
    for width in 0..30 {
        for height in 0..8 {
            let mut surface = Surface::new(width, height, ColorDepth::TrueColor);
            if let iridium_tui::driver::CursorState::At { row, column } = app.render(&mut surface) {
                assert!(
                    row < height && column < width,
                    "cursor outside {width}x{height}"
                );
            }
            let editor = app.editor().expect("active");
            let session = app.workspace.active_payload().expect("active");
            let layout = iridium_tui::frame::Frame::layout(
                editor,
                width,
                height,
                iridium_tui::frame::Chrome {
                    search: Some(&session.search),
                    tab_bar: true,
                    ..Default::default()
                },
            );
            assert_eq!(editor.state().viewport.visible_lines, layout.text_rows);
            if let Some((start, count)) = layout.search_rows {
                assert!(
                    start + count <= layout.tab_row.unwrap_or_else(|| height.saturating_sub(1))
                );
            }
        }
    }
}

#[test]
fn relative_save_as_uses_the_explicit_project_directory() {
    let dir = TempDir::new("terminal-tab-project-save");
    let mut app = app(dir.path());
    app.explorer = None;
    paste(&mut app, "project notes");
    app.write_as(Path::new("notes.md"));
    assert_eq!(
        fs::read_to_string(dir.path().join("notes.md")).expect("saved in project"),
        "project notes"
    );
    assert!(!app.is_dirty());
}

#[test]
fn four_tabs_that_fit_are_contiguous_and_include_the_active_tab() {
    let dir = TempDir::new("terminal-tab-four-labels");
    let paths: Vec<_> = ["a", "b", "c", "d"]
        .iter()
        .map(|name| write(&dir, name, "text"))
        .collect();
    let mut app = app(paths.first().expect("first"));
    for path in paths.iter().skip(1) {
        app.open_file(path);
    }
    let mut surface = Surface::new(36, 8, ColorDepth::TrueColor);
    app.render(&mut surface);
    assert_eq!(row(&surface, 6).trim_end(), " 1:a  2:b  3:c [4:d]");
}
