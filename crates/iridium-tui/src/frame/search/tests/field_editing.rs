//! Focused-field selection is visible and cannot select or cut document text.
use super::*;
use iridium_editor::commands::builtin::{CLIPBOARD_COPY, CURSOR_WORD_LEFT_SELECT};
use iridium_editor::{KeyBinding, Keymap};

fn select(overlay: &mut SearchOverlay, editor: &mut Editor, key: KeyCode, word: bool) {
    chord(
        overlay,
        editor,
        key,
        Modifiers {
            shift: true,
            ctrl: word,
            ..Modifiers::none()
        },
    );
}

#[test]
fn select_all_copy_and_retype_target_query_without_changing_document_selection() {
    let mut editor = editor_with("cat dog cat");
    let mut overlay = opened(&mut editor);
    overlay.paste("cat", &mut editor);
    let selection = editor.state().cursor.clone();
    let searches = count_searches(&mut editor);
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('a'),
        Modifiers::ctrl(),
    );
    assert_eq!(
        chord(
            &mut overlay,
            &mut editor,
            KeyCode::Char('c'),
            Modifiers::ctrl()
        ),
        SearchOutcome::Copied("cat".into())
    );
    assert_eq!(editor.state().cursor, selection);
    assert_eq!(searches.load(Ordering::Relaxed), 0);
    overlay.paste("dog", &mut editor);
    assert_eq!(overlay.query(), "dog");
    assert_eq!(searches.load(Ordering::Relaxed), 1);
    assert_eq!(editor.search_match_count(), 1);
    assert_eq!(editor.content(), "cat dog cat");
}

#[test]
fn reversed_cluster_selection_collapses_and_typing_replaces_the_selected_clusters() {
    let mut editor = editor_with("");
    let mut overlay = opened(&mut editor);
    overlay.paste(&format!("e\u{301}{FAMILY}猫"), &mut editor);
    select(&mut overlay, &mut editor, KeyCode::Left, false);
    select(&mut overlay, &mut editor, KeyCode::Left, false);
    assert_eq!(
        chord(
            &mut overlay,
            &mut editor,
            KeyCode::Char('c'),
            Modifiers::ctrl()
        ),
        SearchOutcome::Copied(format!("{FAMILY}猫"))
    );
    press(&mut overlay, &mut editor, KeyCode::Right);
    assert_eq!(
        chord(
            &mut overlay,
            &mut editor,
            KeyCode::Char('x'),
            Modifiers::ctrl()
        ),
        SearchOutcome::Handled
    );
    select(&mut overlay, &mut editor, KeyCode::Home, false);
    type_text(&mut overlay, &mut editor, "x");
    assert_eq!(overlay.query(), "x");
    assert_eq!(editor.content(), "");
}

#[test]
fn word_selection_and_deletion_preserve_combining_marks_and_punctuation() {
    let mut editor = editor_with("");
    let mut overlay = opened(&mut editor);
    overlay.paste("cafe\u{301}:: 猫 dog", &mut editor);
    select(&mut overlay, &mut editor, KeyCode::Left, true);
    assert_eq!(
        deliver_cut(&mut overlay, &mut editor),
        SearchOutcome::Copied("dog".into())
    );
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Backspace,
        Modifiers::ctrl(),
    );
    assert_eq!(overlay.query(), "cafe\u{301}:: ");
    press(&mut overlay, &mut editor, KeyCode::Home);
    select(&mut overlay, &mut editor, KeyCode::Right, true);
    assert_eq!(
        chord(
            &mut overlay,
            &mut editor,
            KeyCode::Char('c'),
            Modifiers::ctrl()
        ),
        SearchOutcome::Copied("cafe\u{301}".into())
    );
    press(&mut overlay, &mut editor, KeyCode::Left);
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Delete,
        Modifiers::ctrl(),
    );
    assert_eq!(overlay.query(), ":: ");
}

#[test]
fn replacement_selection_is_independent_and_cut_allowed_for_read_only_documents() {
    let mut editor = editor_with("cat");
    editor.state_mut().read_only = true;
    let mut overlay = opened(&mut editor);
    overlay.paste("cat", &mut editor);
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('a'),
        Modifiers::ctrl(),
    );
    press(&mut overlay, &mut editor, KeyCode::Tab);
    overlay.paste("e\u{301}\r\n\t猫", &mut editor);
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('a'),
        Modifiers::ctrl(),
    );
    assert_eq!(
        deliver_cut(&mut overlay, &mut editor),
        SearchOutcome::Copied("e\u{301}\r\n\t猫".into())
    );
    assert_eq!(overlay.replacement(), "");
    assert_eq!(editor.search_match_count(), 1);
    press(&mut overlay, &mut editor, KeyCode::Tab);
    type_text(&mut overlay, &mut editor, "dog");
    assert_eq!(overlay.query(), "dog");
    assert_eq!(editor.content(), "cat");
}

#[test]
fn selection_background_covers_control_markers_and_wide_glyphs_after_scrolling() {
    let mut editor = editor_with("");
    let mut overlay = opened(&mut editor);
    overlay.paste("e\u{301}\r\n\t猫", &mut editor);
    chord(
        &mut overlay,
        &mut editor,
        KeyCode::Char('a'),
        Modifiers::ctrl(),
    );
    let selected = render(&editor, &overlay, 40, 8);
    press(&mut overlay, &mut editor, KeyCode::Right);
    let plain = render(&editor, &overlay, 40, 8);
    let (row, _) = selected.layout.search_rows.expect("search row");
    assert!(selected.find_row().contains("e\u{301}␍␊␉猫"));
    for column in 9..15 {
        assert_ne!(
            selected.background(column, row),
            plain.background(column, row)
        );
    }
    assert_eq!(selected.background(15, row), plain.background(15, row));
    overlay.paste("abcdefghijklmnopqrstuvwxyz", &mut editor);
    select(&mut overlay, &mut editor, KeyCode::Home, false);
    let narrow = render(&editor, &overlay, 22, 8);
    assert!(narrow.find_row().contains("e\u{301}␍␊␉猫"));
    assert!(narrow.layout.caret().is_some());
}

#[test]
fn custom_selection_and_copy_bindings_remain_reachable() {
    let mut editor = editor_with("");
    let mut keys = Keymap::new("user");
    keys.push(KeyBinding::parse("f8", CURSOR_WORD_LEFT_SELECT).expect("binding"));
    keys.push(KeyBinding::parse("f9", CLIPBOARD_COPY).expect("binding"));
    let mut overlay = SearchOverlay::new(&keys);
    overlay.open(&mut editor);
    overlay.paste("first last", &mut editor);
    press(&mut overlay, &mut editor, KeyCode::F8);
    assert_eq!(
        press(&mut overlay, &mut editor, KeyCode::F9),
        SearchOutcome::Copied("last".into())
    );
    press(&mut overlay, &mut editor, KeyCode::Delete);
    assert_eq!(overlay.query(), "first ");
}

fn deliver_cut(overlay: &mut SearchOverlay, editor: &mut Editor) -> SearchOutcome {
    let SearchOutcome::CutRequested(cut) =
        chord(overlay, editor, KeyCode::Char('x'), Modifiers::ctrl())
    else {
        return SearchOutcome::Handled;
    };
    let text = cut.text().to_owned();
    overlay
        .finish_cut(&cut, editor)
        .expect("clipboard delivered");
    SearchOutcome::Copied(text)
}
