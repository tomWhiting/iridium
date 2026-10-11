//! Config behavior in the terminal host.
use super::*;

#[test]
fn a_binding_from_the_users_file_runs_in_this_face() {
    let binding = iridium_editor::KeyBinding::parse(
        "f9",
        iridium_editor::commands::builtin::EDIT_DELETE_TO_LINE_END,
    )
    .expect("the fixture is a key sequence");

    let mut unbound = unnamed();
    for character in "hello".chars() {
        unbound.handle_input(&press(KeyCode::Char(character)));
    }
    unbound.handle_input(&press(KeyCode::Home));
    unbound.handle_input(&press(KeyCode::F9));
    assert_eq!(
        unbound.editor().expect("active editor").content(),
        "hello",
        "F9 must do nothing without the user's file, or this proves nothing"
    );

    let mut app = with_bindings(vec![binding]);
    for character in "hello".chars() {
        app.handle_input(&press(KeyCode::Char(character)));
    }
    app.handle_input(&press(KeyCode::Home));
    app.handle_input(&press(KeyCode::F9));
    assert_eq!(
        app.editor().expect("active editor").content(),
        "",
        "the user's `f9` binding did not reach this face"
    );
}

/// ⭐ **#117: the user's layer reaches this face's panels, not just its editor.**
///
/// A panel is modal and resolves against its own stack, so the layer installed
/// on the editor never reaches it. Nothing else in this suite would notice the
/// hand-over being dropped — the palette would answer its defaults and look
/// entirely well.
///
/// ⚠️ The palette is driven with a scratch [`Editor`] rather than this app's,
/// because both would be borrows of `app` at once. What is under test is which
/// *keymap* the palette holds, which the scratch editor does not affect.
#[test]
fn a_user_binding_reaches_this_faces_command_palette() {
    let ctrl_j = KeyEvent {
        key: KeyCode::Char('j'),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    };
    let scratch = iridium_editor::Editor::with_defaults();
    let mru = iridium_editor::commands::palette::CommandMru::default();

    let mut plain = with_bindings(Vec::new());
    plain.palette.open();
    plain.palette.paste("ab");
    plain.palette.handle_key(&ctrl_j, &scratch, &mru);
    assert_eq!(
        plain.palette.query(),
        "ab",
        "`ctrl+j` must do nothing unbound, or the assertion below proves nothing"
    );

    let binding = iridium_editor::KeyBinding::parse(
        "ctrl+j",
        iridium_editor::commands::builtin::PALETTE_QUERY_BACKSPACE,
    )
    .expect("the fixture is a key sequence");
    let mut app = with_bindings(vec![binding]);
    app.palette.open();
    app.palette.paste("ab");
    app.palette.handle_key(&ctrl_j, &scratch, &mru);
    assert_eq!(
        app.palette.query(),
        "a",
        "the user's palette binding did not reach the panel this face owns"
    );
}

/// A file with a bad line in it says so on the way in, rather than dropping the
/// binding in silence.
#[test]
fn a_refused_binding_is_reported_on_the_message_line() {
    let bad = iridium_editor::KeyBinding::parse(
        "f10",
        iridium_editor::CommandId::new("nobody.registered"),
    )
    .expect("the fixture is a key sequence");

    let mut app = with_bindings(vec![bad]);
    let rows = frame(&mut app, 80, 10);
    assert!(
        rows.iter().any(|row| row.contains("1 problem")),
        "a refused binding must reach the strip, got {rows:?}"
    );
}

/// And a clean configuration says nothing at all — the strip is for news.
#[test]
fn a_clean_configuration_leaves_the_message_line_alone() {
    let mut app = with_bindings(Vec::new());
    let rows = frame(&mut app, 80, 10);
    assert!(
        !rows.iter().any(|row| row.contains("problem")),
        "nothing was wrong and the strip said otherwise: {rows:?}"
    );
}
