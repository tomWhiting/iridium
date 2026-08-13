//! What the desktop context menu's keymap guarantees — #117.
//!
//! Two kinds of assertion live here. The first are about the *table*: every verb
//! reachable, every binding shift-blind, the count documented. The second are
//! about the *seam*: a line a user could write in `config.toml` reaching the
//! menu and moving a real key.
//!
//! ⭐ The second kind is the point of the task, and is deliberately written
//! against [`KeyBinding::parse`] — the same parser `iridium_config` calls on the
//! text of the file — rather than against a hand-built binding.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use iridium_editor::commands::builtin::{
    CLIPBOARD_COPY, CLIPBOARD_CUT, CONTEXT_MENU_MODE, CONTEXT_MENU_SELECT_LAST,
    CONTEXT_MENU_SELECT_NEXT, EDIT_DELETE_TO_LINE_END, PALETTE_OPEN, default_registry,
    panel_command_metas,
};
use iridium_editor::{
    CommandId, Editor, KeyBinding, KeyCode, KeyEvent, Keymap, ModifierPattern, ModifierState,
    Modifiers,
};

use super::keymap::{DEFAULT_BINDING_COUNT, EVERY_PATTERN, default_keymap};
use super::resolve::Resolved;
use super::verb::Verb;
use super::{ContextMenu, MenuOutcome};

/// An empty layer: a menu built with this answers only its own defaults.
fn no_user_keys() -> Keymap {
    Keymap::new("empty")
}

/// A user layer holding one line as `config.toml` would have produced it.
fn user_layer(sequence: &str, command: CommandId) -> Keymap {
    let mut layer = Keymap::new("user");
    layer.push(KeyBinding::parse(sequence, command).expect("the fixture is a key sequence"));
    layer
}

/// A user layer holding one *unbind*, built the way `iridium_config` builds one.
fn unbind_layer(sequence: &str) -> Keymap {
    let (first, rest) =
        KeyBinding::parse_sequence(sequence).expect("the fixture is a key sequence");
    let mut layer = Keymap::new("user");
    layer.push(KeyBinding::unbound(first, &rest));
    layer
}

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A kernel carrying this face's commands and keymap, so every ruled row
/// resolves and the menu is the menu a user sees.
fn editor() -> Editor {
    let mut editor = Editor::with_defaults();
    for meta in crate::commands::command_metas() {
        editor
            .register_command(meta)
            .expect("the kernel accepted the command");
    }
    editor
        .push_keymap(crate::commands::keymap())
        .expect("the keymap validates");
    editor
}

/// A menu over that kernel, answering `user` on top of its own defaults.
fn menu_with(user: &Keymap, editor: &Editor) -> ContextMenu {
    ContextMenu::open(editor, 0.0, 0.0, user)
}

/// Which row `Enter` would run — the menu's selection, observed as the host
/// observes it.
fn selected(menu: &mut ContextMenu) -> MenuOutcome {
    menu.handle_key(&press(KeyCode::Enter))
}

// ===== The table =====

#[test]
fn the_binding_count_is_what_the_module_documents() {
    assert_eq!(default_keymap().len(), DEFAULT_BINDING_COUNT);
}

/// Every verb is reachable without touching a configuration file.
#[test]
fn every_verb_the_menu_answers_is_bound_by_the_default_keymap() {
    let keymap = default_keymap();
    let bound: BTreeSet<&str> = keymap
        .bindings()
        .iter()
        .filter_map(|binding| binding.command())
        .map(CommandId::as_str)
        .collect();
    for verb in Verb::ALL {
        assert!(
            bound.contains(verb.id().as_str()),
            "`{}` is a verb the menu answers and no default binding reaches it",
            verb.id()
        );
    }
}

/// And the other way round: the table binds nothing this menu cannot run.
#[test]
fn every_default_binding_names_a_verb_this_menu_answers() {
    for binding in default_keymap().bindings() {
        let id = binding
            .command()
            .expect("no default binding is a suppression");
        assert!(
            Verb::from_id(id).is_some(),
            "`{id}` is bound by default and the menu does not answer it"
        );
    }
}

/// Every binding is scoped to the menu's mode, which is what keeps these chords
/// out of the document.
#[test]
fn every_default_binding_is_scoped_to_the_menu() {
    for binding in default_keymap().bindings() {
        assert_eq!(
            binding.mode(),
            Some(&CONTEXT_MENU_MODE),
            "`{}` is not scoped to the context menu",
            binding.display_sequence()
        );
    }
}

/// ⛔ **The invisible regression.** The two boolean guards this table replaced
/// read `ctrl`, `alt` and `meta` and never looked at shift or `AltGraph`. A
/// pattern that does not spell them `Any` *requires them absent*, and would
/// silently drop chords that work today.
#[test]
fn every_default_binding_ignores_shift_exactly_as_the_guards_did() {
    for binding in default_keymap().bindings() {
        for stroke in binding.sequence() {
            assert_eq!(
                stroke.modifiers.shift,
                ModifierState::Any,
                "`{}` constrains shift; the guards it replaced never did",
                binding.display_sequence()
            );
            assert_eq!(
                stroke.modifiers.alt_graph,
                ModifierState::Any,
                "`{}` constrains AltGraph; the guards it replaced never did",
                binding.display_sequence()
            );
        }
    }
}

/// The same rule at the source: a pattern added to the table without `Any`
/// shift is caught here even before a binding uses it.
#[test]
fn every_modifier_pattern_the_table_offers_ignores_shift() {
    for pattern in EVERY_PATTERN {
        assert_eq!(pattern.shift, ModifierState::Any);
        assert_eq!(pattern.alt_graph, ModifierState::Any);
    }
}

/// ⭐ And the same rule *behaviourally*, which the two above cannot say.
///
/// A pattern-level assertion proves the table spells shift `Any`; this proves
/// **resolution honours it**, which is the property a user holding shift
/// actually depends on.
#[test]
fn holding_shift_changes_nothing_about_what_a_default_binding_resolves_to() {
    let editor = editor();
    let keymap = default_keymap();
    for binding in keymap.bindings() {
        let strokes = binding.sequence();
        assert_eq!(
            strokes.len(),
            1,
            "this ratchet presses one stroke; `{}` is a sequence",
            binding.display_sequence()
        );
        let stroke = &strokes[0];
        let id = binding
            .command()
            .expect("no default binding is a suppression");
        let expected = Verb::from_id(id).expect("every default binding names a verb");

        for shift in [false, true] {
            let mut menu = menu_with(&no_user_keys(), &editor);
            let event = KeyEvent {
                key: stroke.key,
                modifiers: concrete(stroke.modifiers, shift),
                is_repeat: false,
            };
            assert_eq!(
                menu.resolve_key(&event),
                Resolved::Verb(expected),
                "`{}` resolves differently with shift {}",
                binding.display_sequence(),
                if shift { "held" } else { "released" }
            );
        }
    }
}

/// One concrete modifier set a `pattern` accepts, with shift as asked.
const fn concrete(pattern: ModifierPattern, shift: bool) -> Modifiers {
    const fn held(state: ModifierState) -> bool {
        matches!(state, ModifierState::Required)
    }
    Modifiers {
        shift,
        ctrl: held(pattern.ctrl),
        alt: held(pattern.alt),
        meta: held(pattern.meta),
        alt_graph: held(pattern.alt_graph),
    }
}

/// ⚠️ The two bindings whose old `match` arms read `Char('p' | 'P')` and
/// `Char('n' | 'N')`.
///
/// `StrokePattern::matches` ASCII-lowercases a character keycode, so the single
/// binding on each letter must answer the shifted capital too. Written out
/// rather than left to the ratchet above, because that ratchet varies the
/// *modifier* and this is about the **keycode** the window system delivers.
#[test]
fn the_readline_chords_answer_capitals_as_well() {
    let editor = editor();
    for (key, expected) in [
        (KeyCode::Char('P'), Verb::SelectPrevious),
        (KeyCode::Char('N'), Verb::SelectNext),
    ] {
        let mut menu = menu_with(&no_user_keys(), &editor);
        let event = KeyEvent {
            key,
            modifiers: Modifiers {
                shift: true,
                ctrl: true,
                alt: false,
                meta: false,
                alt_graph: false,
            },
            is_repeat: false,
        };
        assert_eq!(menu.resolve_key(&event), Resolved::Verb(expected));
    }
}

/// The menu's ids are the kernel's ids, and the kernel knows all of them.
#[test]
fn the_default_bindings_validate_against_the_editors_registry() {
    let registry = default_registry().expect("the kernel tables are consistent");
    default_keymap()
        .validate(&registry)
        .expect("every command the menu binds must be a registered command");
}

/// Every command the kernel scopes to this menu is a verb it answers, and every
/// verb is scoped here. Catches the drift where a command is added to the
/// kernel's table and forgotten — a `[keys]` line that validates and does
/// nothing, the #110 defect class.
#[test]
fn the_kernels_vocabulary_and_this_menus_verbs_are_the_same_set() {
    let registered: BTreeSet<&str> = panel_command_metas()
        .filter(|meta| meta.mode() == Some(&CONTEXT_MENU_MODE))
        .map(|meta| meta.id().as_str())
        .collect();
    // Owned, because `Verb::id` returns by value and a borrow of it would not
    // outlive this expression.
    let ids: Vec<CommandId> = Verb::ALL.iter().map(|verb| verb.id()).collect();
    let answered: BTreeSet<&str> = ids.iter().map(CommandId::as_str).collect();
    assert_eq!(registered, answered);
}

// ===== Direction, pinned absolutely =====

/// ⭐ **The witness every seam test below is compared against.**
///
/// ⛔ Written as two *absolute* claims — `Down` reaches Copy, `Up` from there
/// reaches Cut — and deliberately **not** as a round trip. A test that presses a
/// key and then its opposite proves the two are inverses of each other; it never
/// says which one goes forward, and swapping both bindings leaves it green.
/// The ruled row order (Cut, Copy, Paste, …) is the outside fact that makes this
/// measurable, and the first assertion pins the menu's opening row so that
/// neither claim below is relative to the other.
#[test]
fn the_arrows_move_the_highlight_in_the_direction_the_rows_run() {
    let editor = editor();

    let mut opened = menu_with(&no_user_keys(), &editor);
    assert_eq!(
        selected(&mut opened),
        MenuOutcome::Run(CLIPBOARD_CUT),
        "the menu must open on its first runnable row, or nothing below is anchored"
    );

    let mut down = menu_with(&no_user_keys(), &editor);
    down.handle_key(&press(KeyCode::Down));
    assert_eq!(
        selected(&mut down),
        MenuOutcome::Run(CLIPBOARD_COPY),
        "`Down` must move towards the end of the ruled row order"
    );

    let mut back = menu_with(&no_user_keys(), &editor);
    back.handle_key(&press(KeyCode::Down));
    back.handle_key(&press(KeyCode::Up));
    assert_eq!(
        selected(&mut back),
        MenuOutcome::Run(CLIPBOARD_CUT),
        "`Up` must move towards the start of the ruled row order"
    );
}

// ===== The seam =====

/// ⭐ **The task, end to end.** A line a user could write moves a real key.
///
/// ⚠️ Compared against `Down`'s landing row, which the test above pinned
/// absolutely — not against where the menu started, which would hold just as
/// well if both keys were wrong.
#[test]
fn a_configuration_line_moves_a_menu_key() {
    let editor = editor();

    let mut menu = menu_with(&user_layer("ctrl+j", CONTEXT_MENU_SELECT_NEXT), &editor);
    menu.handle_key(&KeyEvent {
        key: KeyCode::Char('j'),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    });
    assert_eq!(
        selected(&mut menu),
        MenuOutcome::Run(CLIPBOARD_COPY),
        "`ctrl+j` must select what `Down` selects"
    );
}

/// ⭐ **The trap the mode exists to close.** A mode-free document binding
/// pushed into this menu whole would apply here too — so a binding on `Down`
/// would stop `Down` moving the highlight, having never mentioned the menu.
#[test]
fn a_user_binding_naming_a_document_command_never_reaches_this_menu() {
    let editor = editor();

    let mut menu = menu_with(&user_layer("down", EDIT_DELETE_TO_LINE_END), &editor);
    menu.handle_key(&press(KeyCode::Down));
    assert_eq!(
        selected(&mut menu),
        MenuOutcome::Run(CLIPBOARD_COPY),
        "a document binding must not take `Down` away from the context menu"
    );
}

/// ⚠️ An unbind carries no command, so it cannot be scoped — and is not
/// dropped. The honest reading of `"escape" = ""` is that the key does nothing,
/// here too. The menu is still closable: a click outside it closes it, and that
/// is not a key.
#[test]
fn an_unbind_suppresses_the_key_in_this_menu_as_well() {
    let editor = editor();
    let mut menu = menu_with(&unbind_layer("escape"), &editor);
    assert_eq!(
        menu.handle_key(&press(KeyCode::Escape)),
        MenuOutcome::Handled,
        "an unbound `Escape` must not close the menu"
    );
}

/// ⛔ **Why this menu's `Resolved` has no `Suppressed` variant.**
///
/// The palette and the file explorer both have a field, so an unclaimed
/// printable key becomes *text* — and a suppression reported as unclaimed would
/// type the very character the user unbound. This menu has no field, so both
/// cases are the same nothing. This test is what makes that claim measurable
/// rather than a comment: a printable key nobody bound changes no highlight and
/// does not close the menu.
#[test]
fn an_unclaimed_printable_key_is_swallowed_and_changes_nothing() {
    let editor = editor();

    let mut menu = menu_with(&no_user_keys(), &editor);
    assert_eq!(
        menu.handle_key(&press(KeyCode::Char('a'))),
        MenuOutcome::Handled,
        "the menu is modal: an unbound letter must not fall through"
    );
    assert_eq!(
        selected(&mut menu),
        MenuOutcome::Run(CLIPBOARD_CUT),
        "an unbound letter moved the highlight"
    );
}

/// ⚠️ A user's multi-stroke sequence is reachable, and the first stroke of one
/// is held rather than thrown away.
///
/// The menu has no field, so `Pending` and `Unclaimed` are the same
/// [`MenuOutcome`] from outside — which is why this asserts on
/// [`Resolved`] directly for the first stroke, and why the **second** stroke is
/// `ctrl+x`, which no default binding claims. Were the pending sequence dropped
/// after stroke one, `ctrl+x` alone would match nothing and the highlight would
/// not move; a second stroke that happened to be bound by default would have
/// made this test pass either way.
#[test]
fn a_multi_stroke_user_sequence_waits_for_its_second_stroke() {
    let editor = editor();
    let mut menu = menu_with(
        &user_layer("ctrl+k ctrl+x", CONTEXT_MENU_SELECT_LAST),
        &editor,
    );

    let ctrl = |key| KeyEvent {
        key,
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    };
    assert_eq!(
        menu.resolve_key(&ctrl(KeyCode::Char('k'))),
        Resolved::Pending,
        "the first stroke of a sequence must be held, not resolved"
    );
    menu.handle_key(&ctrl(KeyCode::Char('x')));
    assert_eq!(
        selected(&mut menu),
        MenuOutcome::Run(PALETTE_OPEN),
        "the completed sequence must reach the last runnable row"
    );
}
