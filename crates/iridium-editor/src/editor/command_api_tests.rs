//! Editor-level command surface: invoke by id, enumerate for a palette, contribute
//! a command from outside the kernel, and swap bindings on the live editor.
//!
//! Every one of these paths was unreachable before: the command registry existed
//! but no editor built one, the only way to run a command was to synthesize a
//! keypress, and the `KeyboardHandler` the editor drives had no accessor at all —
//! so a host wanting to rebind had to abandon [`Editor`] and duplicate the
//! sticky-column and addition-order state whose staleness has bitten this codebase
//! twice.
//!
//! The tests drive the real [`Editor`] and assert the resulting document and cursor
//! state, not just the returned tag.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{Editor, EditorConfig, EditorKeyResult};
use crate::commands::{
    CommandArgs, CommandCategory, CommandId, CommandMeta, KeyBinding, Keymap, KeymapError,
    ModifierPattern, ModifierState, StrokePattern, builtin,
};
use crate::document::Position;
use crate::input::{KeyCode, KeyEvent, Modifiers};

fn editor_with(content: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(content);
    editor
}

fn ctrl_stroke(key: KeyCode) -> StrokePattern {
    StrokePattern::new(
        key,
        ModifierPattern::NONE.with_ctrl(ModifierState::Required),
    )
}

/// An editor whose keymap carries a `Ctrl+K Ctrl+D` chord in a host layer.
///
/// The default keymap binds no multi-stroke sequence — `Ctrl+K` is reserved as
/// the command-palette leader, and a bare binding forecloses every chord under
/// it — so an editor-level test of chord behaviour has to supply its own. Doing
/// so keeps these tests about the pending-sequence machinery rather than about
/// which binding happens to be a chord.
fn editor_with_chord(content: &str) -> Editor {
    let mut editor = editor_with(content);
    let mut layer = Keymap::new("host-chord");
    layer.push(KeyBinding::new(
        ctrl_stroke(KeyCode::Char('k')),
        &[ctrl_stroke(KeyCode::Char('d'))],
        builtin::MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
    ));
    editor
        .push_keymap(layer)
        .expect("nothing in the default claims the Ctrl+K prefix");
    editor
}

// ===== Invoke by id =====

#[test]
fn every_command_the_palette_lists_can_be_invoked_by_id() {
    // The load-bearing claim: a palette renders `commands()` and must be able to
    // run what it renders. Two entries cannot come from a palette — the typing
    // fall-through has no character without a keystroke, and the explicit no-op
    // does nothing by definition — and both are still *invocable*, they simply
    // change nothing.
    let mut editor = editor_with("alpha beta\ngamma delta\n");
    let ids: Vec<String> = editor
        .commands()
        .palette_order()
        .iter()
        .map(|meta| meta.id().as_str().to_owned())
        .collect();
    assert_eq!(ids.len(), builtin::BUILTIN_COMMAND_COUNT);

    for id in &ids {
        editor.set_cursor(Position::new(0, 2));
        editor
            .run_command(id, CommandArgs::NONE)
            .unwrap_or_else(|error| panic!("palette entry `{id}` is not invocable: {error}"));
    }
}

#[test]
fn invoking_a_command_by_id_edits_the_document_undoably() {
    let mut editor = editor_with("one\ntwo\nthree\n");
    editor.set_cursor(Position::new(1, 0));

    let outcome = editor
        .run_command(builtin::LINES_DELETE.as_str(), CommandArgs::NONE)
        .expect("lines.delete is a kernel command");
    assert_eq!(outcome, EditorKeyResult::None);
    assert_eq!(editor.content(), "one\nthree\n");

    // Command-sourced: the palette invocation went through the undo tree, so it
    // reverses exactly like the keyboard path.
    assert!(editor.undo());
    assert_eq!(editor.content(), "one\ntwo\nthree\n");
}

#[test]
fn invoking_a_command_by_id_keeps_the_sticky_column() {
    // Invoke-by-id runs the same post-command bookkeeping as a keypress; if it did
    // not, a palette-driven "Cursor Down" would lose the preferred column that the
    // Down key preserves.
    let mut editor = editor_with("aaaaaaaaaa\nbb\ncccccccccc");
    editor.set_cursor(Position::new(0, 8));

    for _ in 0..2 {
        editor
            .run_command(builtin::CURSOR_LINE_DOWN.as_str(), CommandArgs::NONE)
            .expect("cursor.lineDown is a kernel command");
    }
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(2, 8),
        "the sticky column must survive an invocation by id"
    );
}

#[test]
fn invoking_a_command_by_id_honours_read_only_mode() {
    let mut editor = editor_with("keep me");
    editor.state_mut().read_only = true;

    editor
        .run_command(builtin::LINES_DELETE.as_str(), CommandArgs::NONE)
        .expect("the command exists");
    assert_eq!(
        editor.content(),
        "keep me",
        "a palette invocation must not bypass read-only mode"
    );
}

#[test]
fn invoking_an_unimplemented_id_reports_it_rather_than_doing_nothing() {
    let mut editor = editor_with("x");
    let error = editor
        .run_command("host.doThing", CommandArgs::NONE)
        .expect_err("the kernel implements no such command");
    assert!(error.to_string().contains("host.doThing"), "{error}");
    assert!(!Editor::implements_command("host.doThing"));
    assert!(Editor::implements_command(builtin::CLIPBOARD_COPY.as_str()));
}

// ===== Contributing a command from outside the kernel =====

#[test]
fn a_host_command_is_registered_listed_bound_and_reported() {
    let mut editor = editor_with("body");
    let id = CommandId::from_static("host.toSnakeCase");

    // 1. Register: the palette can now see it.
    editor
        .register_command(
            CommandMeta::new(id.clone(), "To snake_case", CommandCategory::EDITING)
                .with_description("Contributed by the host, implemented by the host.")
                .mutating(),
        )
        .expect("a fresh id registers");
    assert!(editor.commands().contains(id.as_str()));
    assert!(
        editor
            .commands()
            .palette_order()
            .iter()
            .any(|meta| meta.id() == &id),
        "a registered host command must appear in palette order"
    );

    // 2. Bind it. Validation passes precisely because it is registered.
    let mut user = Keymap::new("host");
    user.push(KeyBinding::new(
        ctrl_stroke(KeyCode::Char('t')),
        &[],
        id.clone(),
    ));
    editor.push_keymap(user).expect("the id is registered");

    // 3. The keypress reports the command instead of vanishing.
    let outcome = editor.handle_key(&KeyEvent::new(KeyCode::Char('t'), Modifiers::ctrl()));
    assert_eq!(
        outcome,
        EditorKeyResult::HostCommand {
            command: id,
            args: CommandArgs::NONE,
        }
    );
    assert_eq!(editor.content(), "body", "the kernel must not have edited");
}

#[test]
fn a_host_may_not_shadow_a_kernel_command_id() {
    let mut editor = editor_with("");
    let error = editor
        .register_command(CommandMeta::new(
            builtin::CLIPBOARD_COPY,
            "Not Copy",
            CommandCategory::EDITING,
        ))
        .expect_err("kernel ids are taken");
    assert!(error.to_string().contains("clipboard.copy"), "{error}");
}

// ===== Rebinding the live editor =====

#[test]
fn a_user_keymap_rebinds_the_live_editor() {
    let mut editor = editor_with("alpha beta");
    editor.set_cursor(Position::new(0, 0));

    let mut user = Keymap::new("user");
    // Move select-all onto Ctrl+B, and unbind Ctrl+A entirely.
    user.push(KeyBinding::unbound(ctrl_stroke(KeyCode::Char('a')), &[]));
    user.push(KeyBinding::new(
        ctrl_stroke(KeyCode::Char('b')),
        &[],
        builtin::SELECTION_SELECT_ALL,
    ));
    editor
        .push_keymap(user)
        .expect("valid against the registry");
    assert_eq!(editor.keymap().len(), 2);

    editor.handle_key(&KeyEvent::new(KeyCode::Char('b'), Modifiers::ctrl()));
    assert_eq!(editor.state().cursor.primary.anchor, Position::new(0, 0));
    assert_eq!(editor.state().cursor.primary.head, Position::new(0, 10));

    // Ctrl+A is unbound, so it carries no text and is simply ignored.
    editor.set_cursor(Position::new(0, 3));
    editor.handle_key(&KeyEvent::new(KeyCode::Char('a'), Modifiers::ctrl()));
    assert_eq!(editor.state().cursor.primary.head, Position::new(0, 3));
    assert_eq!(editor.content(), "alpha beta");

    // Popping the layer restores the default meaning.
    editor.pop_keymap().expect("the user layer");
    editor.handle_key(&KeyEvent::new(KeyCode::Char('a'), Modifiers::ctrl()));
    assert_eq!(editor.state().cursor.primary.head, Position::new(0, 10));
}

#[test]
fn a_user_keymap_naming_an_unknown_command_is_rejected_at_load_time() {
    let mut editor = editor_with("");
    let mut user = Keymap::new("user");
    user.push(KeyBinding::new(
        ctrl_stroke(KeyCode::Char('t')),
        &[],
        CommandId::from_static("clipbord.copy"),
    ));

    let error = editor
        .push_keymap(user)
        .expect_err("a typo must not become a silently dead key");
    assert!(
        matches!(error, KeymapError::UnknownCommand { .. }),
        "{error}"
    );
    assert_eq!(
        editor.keymap().len(),
        1,
        "a rejected layer must not be half-applied"
    );
}

#[test]
fn a_user_keymap_that_strands_a_lower_chord_is_rejected_at_load_time() {
    // The commonest customization there is — binding the bare chord leader —
    // silently made a `Ctrl+K Ctrl+D` chord below it unreachable. It is now a
    // diagnostic, and the documented fix (also unbinding the chord) clears it.
    //
    // The chord being stranded now comes from a host layer rather than the
    // default keymap, which reserves `Ctrl+K` and binds nothing on it. The
    // diagnostic is about the *relationship* between two layers, so a host chord
    // exercises it exactly as the default one used to.
    let mut editor = editor_with_chord("");
    let mut greedy = Keymap::new("user");
    greedy.push(KeyBinding::new(
        ctrl_stroke(KeyCode::Char('k')),
        &[],
        builtin::LINES_DELETE,
    ));

    let error = editor
        .push_keymap(greedy.clone())
        .expect_err("the host chord would be stranded");
    match error {
        KeymapError::CrossLayerShadowedSequence {
            prefix, shadowed, ..
        } => {
            assert_eq!(prefix, "ctrl+k");
            // The display form is round-trippable, so it names every modifier the
            // stranded sequence constrains and nothing it does not.
            assert_eq!(shadowed, "ctrl+k ctrl+d");
        },
        other => panic!("expected a cross-layer shadowing error, got {other:?}"),
    }
    assert_eq!(
        editor.keymap().len(),
        2,
        "a rejected layer must not be half-applied"
    );

    // The documented fix, driven entirely from the diagnostic: its `shadowed` text
    // is the round-trippable display form, so it parses straight back into the
    // sequence to unbind. Suppression matches a sequence exactly, which is why the
    // diagnostic has to be quotable like this rather than approximated by hand.
    let (first, rest) = KeyBinding::parse_sequence("ctrl+k ctrl+d").expect("parses");
    let mut fixed = greedy;
    fixed.push(KeyBinding::unbound(first, &rest));
    editor
        .push_keymap(fixed)
        .expect("unbinding the chord clears the shadow");

    // And Ctrl+K now runs the user's command immediately rather than pending.
    editor.set_content("one\ntwo\n");
    editor.set_cursor(Position::new(0, 0));
    editor.handle_key(&KeyEvent::new(KeyCode::Char('k'), Modifiers::ctrl()));
    assert_eq!(editor.content(), "two\n");
    assert!(editor.pending_key_sequence().is_empty());
}

#[test]
fn a_config_loaded_keymap_is_canonicalized_so_resolution_stays_allocation_free() {
    let mut editor = editor_with("");
    let json = r#"{
        "name": "user",
        "bindings": [
            {"keys":[{"key":{"Char":"t"},"modifiers":{"ctrl":"required"}}],
             "command":"clipboard.copy"}
        ]
    }"#;
    let keymap: Keymap = serde_json::from_str(json).expect("valid keymap json");
    assert!(
        !keymap.bindings()[0].command().unwrap().is_static(),
        "a deserialized id owns its storage"
    );

    editor.push_keymap(keymap).expect("valid");
    let pushed = editor.keymap().layers().last().expect("the user layer");
    assert!(
        pushed.bindings()[0].command().unwrap().is_static(),
        "pushing must intern ids so no keystroke allocates"
    );
}

// ===== Modes and the pending indicator =====

#[test]
fn the_pending_sequence_is_visible_to_the_host_and_abortable() {
    // A chord leader consumes the keypress; without an observable pending state the
    // editor simply looks unresponsive, and the host has nothing to render.
    let mut editor = editor_with_chord("hi");
    editor.set_cursor(Position::new(0, 2));

    assert_eq!(
        editor.handle_key(&KeyEvent::new(KeyCode::Char('k'), Modifiers::ctrl())),
        EditorKeyResult::None
    );
    assert_eq!(editor.pending_key_sequence().len(), 1);
    assert_eq!(editor.pending_key_sequence()[0].key, KeyCode::Char('k'));

    // Focus loss must clear it, or the first key after the user returns is eaten.
    assert!(editor.abort_pending_key_sequence());
    assert!(editor.pending_key_sequence().is_empty());
    assert!(!editor.abort_pending_key_sequence());

    editor.handle_key(&KeyEvent::simple(KeyCode::Char('!')));
    assert_eq!(editor.content(), "hi!");
}

#[test]
fn the_editor_exposes_and_accepts_a_mode() {
    let mut editor = editor_with("");
    assert_eq!(editor.mode(), None, "the default keymap is non-modal");

    let normal = crate::commands::ModeName::from_static("normal");
    editor.set_mode(Some(normal.clone()));
    assert_eq!(editor.mode(), Some(&normal));

    editor.set_mode(None);
    assert_eq!(editor.mode(), None);
}
