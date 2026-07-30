//! Integration test: everything a host outside this crate needs in order to
//! contribute a command.
//!
//! This has to be an *integration* test. Unit tests inside the crate can reach
//! `pub(crate)` items, so they cannot prove that a real host — `iridium-bindings`,
//! a terminal face, a notebook embedding — can reach anything at all. Two of the
//! three things exercised here were previously impossible from outside:
//!
//! - the per-cursor edit builders lived behind a `pub(crate)` module, so a host
//!   command could only ever have edited the primary cursor;
//! - there was no way to invoke a command by id, so a palette could render a list
//!   and run none of it.
//!
//! If this file stops compiling, host extensibility has regressed even though every
//! in-crate test still passes.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use iridium_editor::input::keyboard::editing::{self, CaretPlacement, CursorEdit};
use iridium_editor::input::keyboard::motions;
use iridium_editor::{
    CommandArgs, CommandCategory, CommandId, CommandMeta, Editor, EditorConfig, EditorKeyResult,
    KeyBinding, KeyCode, KeyEvent, Keymap, Modifiers, Position, Range,
};

/// The host command a real extension would register.
const HOST_COMMAND: CommandId = CommandId::from_static("host.surroundWithBrackets");

#[test]
fn a_host_command_edits_every_cursor_through_the_public_api() {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content("one\ntwo\nthree");
    editor.set_cursor(Position::new(0, 3));
    // Two more cursors, so a primary-only implementation would be visible.
    editor.handle_key(&KeyEvent::new(
        KeyCode::Down,
        Modifiers {
            shift: false,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        },
    ));
    editor.handle_key(&KeyEvent::new(
        KeyCode::Down,
        Modifiers {
            shift: false,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        },
    ));
    assert_eq!(editor.state().cursor.cursor_count(), 3);

    // The host builds one edit per cursor with the kernel's own builder, then
    // applies the reversible command it produces. Nothing else may mutate a
    // document, which is what keeps the undo tree correct.
    let command = {
        let state = editor.state();
        let edits: Vec<(CursorEdit, CaretPlacement)> = state
            .cursor
            .all_selections()
            .map(|selection| {
                (
                    CursorEdit::replace(
                        Range::new(selection.head, selection.head),
                        String::from("<>"),
                    ),
                    CaretPlacement::collapsed(1),
                )
            })
            .collect();
        editing::build_multi_cursor_command_placed(&state.document, &state.cursor, edits)
            .expect("three edits must produce a command")
    };
    editor.apply_command(command);

    assert_eq!(
        editor.content(),
        // The third cursor sits at the sticky column 3 of "three", not at its end.
        "one<>\ntwo<>\nthr<>ee",
        "a host command must reach every cursor, not just the primary"
    );
    assert!(editor.undo(), "a host edit must be undoable");
    assert_eq!(editor.content(), "one\ntwo\nthree");
}

#[test]
fn a_host_registers_lists_binds_and_receives_its_own_command() {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content("payload");

    editor
        .register_command(
            CommandMeta::new(
                HOST_COMMAND,
                "Surround With Brackets",
                CommandCategory::EDITING,
            )
            .with_description("Implemented by the host, listed by the editor.")
            .mutating(),
        )
        .expect("a fresh id registers");
    assert!(
        editor
            .commands()
            .palette_order()
            .iter()
            .any(|meta| meta.id() == &HOST_COMMAND),
        "a host command must be enumerable for the palette"
    );

    let mut keymap = Keymap::new("host");
    keymap.push(
        KeyBinding::parse("ctrl-alt-b", HOST_COMMAND).expect("the text form of a chord parses"),
    );
    editor
        .push_keymap(keymap)
        .expect("a registered id validates against the registry");

    let outcome = editor.handle_key(&KeyEvent::new(
        KeyCode::Char('b'),
        Modifiers {
            shift: false,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        },
    ));
    assert_eq!(
        outcome,
        EditorKeyResult::HostCommand {
            command: HOST_COMMAND,
            args: CommandArgs::NONE,
        },
        "the resolved id must reach the host that implements it"
    );
    assert_eq!(editor.content(), "payload");
}

#[test]
fn a_palette_invokes_a_kernel_command_by_id_with_no_keystroke() {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content("alpha\nbeta\ngamma");
    editor.set_cursor(Position::new(1, 0));

    // What a palette does: pick an entry it enumerated, run it by id.
    let entry = editor
        .commands()
        .palette_order()
        .iter()
        .find(|meta| meta.title() == "Delete Line")
        .map(|meta| meta.id().as_str().to_owned())
        .expect("the palette lists Delete Line");
    editor
        .run_command(&entry, CommandArgs::NONE)
        .expect("a kernel command runs by id");

    assert_eq!(editor.content(), "alpha\ngamma");
    assert!(editor.undo());
    assert_eq!(editor.content(), "alpha\nbeta\ngamma");

    // The kernel's motion primitives are reachable too, so a host command can
    // compute positions the same way the kernel does.
    let state = editor.state();
    assert_eq!(
        motions::char_right(&state.document, Position::new(0, 0)),
        Position::new(0, 1)
    );
}
