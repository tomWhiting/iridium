//! What the terminal palette's keymap guarantees — #117.
//!
//! Two kinds of assertion live here. The first are about the *table*: every verb
//! reachable, every binding shift-blind, the count documented. The second are
//! about the *seam*: a line a user could write in `config.toml` reaching the
//! panel and moving a real key.
//!
//! ⭐ The second kind is the point of the task, and is deliberately written
//! against [`KeyBinding::parse`] — the same parser `iridium_config` calls on the
//! text of the file — rather than against a hand-built binding.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use iridium_editor::commands::builtin::{
    EDIT_DELETE_TO_LINE_END, PALETTE_MODE, PALETTE_SELECT_NEXT, default_registry,
    panel_command_metas,
};
use iridium_editor::commands::palette::CommandMru;
use iridium_editor::{
    CommandId, Editor, KeyBinding, KeyCode, KeyEvent, Keymap, ModifierPattern, ModifierState,
    Modifiers,
};

use super::keymap::{DEFAULT_BINDING_COUNT, EVERY_PATTERN, default_keymap};
use super::resolve::Resolved;
use super::verb::Verb;
use super::{CommandPalette, PaletteOutcome};

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

/// An open palette with an empty query, so every command is a result.
fn opened() -> CommandPalette {
    let mut panel = CommandPalette::new();
    panel.open();
    panel
}

/// Which command `Enter` would run — the panel's selection, observed as the
/// caller observes it.
fn selected(panel: &mut CommandPalette, editor: &Editor, mru: &CommandMru) -> PaletteOutcome {
    panel.handle_key(&press(KeyCode::Enter), editor, mru)
}

// ===== The table =====

#[test]
fn the_binding_count_is_what_the_module_documents() {
    assert_eq!(default_keymap().len(), DEFAULT_BINDING_COUNT);
}

/// Every verb is reachable without touching a configuration file.
#[test]
fn every_verb_the_panel_answers_is_bound_by_the_default_keymap() {
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
            "`{}` is a verb the panel answers and no default binding reaches it",
            verb.id()
        );
    }
}

/// And the other way round: the table binds nothing this panel cannot run.
#[test]
fn every_default_binding_names_a_verb_this_panel_answers() {
    for binding in default_keymap().bindings() {
        let id = binding
            .command()
            .expect("no default binding is a suppression");
        assert!(
            Verb::from_id(id).is_some(),
            "`{id}` is bound by default and the panel does not answer it"
        );
    }
}

/// Every binding is scoped to the palette's mode, which is what keeps these
/// chords out of the document.
#[test]
fn every_default_binding_is_scoped_to_the_panel() {
    for binding in default_keymap().bindings() {
        assert_eq!(
            binding.mode(),
            Some(&PALETTE_MODE),
            "`{}` is not scoped to the palette",
            binding.display_sequence()
        );
    }
}

/// ⛔ **D-4, the invisible regression.** The `chord` function this table
/// replaced read `ctrl`, `alt` and `meta` and never looked at shift or
/// `AltGraph`. A pattern that does not spell them `Any` *requires them absent*,
/// and would silently drop chords that work today.
#[test]
fn every_default_binding_ignores_shift_exactly_as_the_chord_table_did() {
    for binding in default_keymap().bindings() {
        for stroke in binding.sequence() {
            assert_eq!(
                stroke.modifiers.shift,
                ModifierState::Any,
                "`{}` constrains shift; the table it replaced never did",
                binding.display_sequence()
            );
            assert_eq!(
                stroke.modifiers.alt_graph,
                ModifierState::Any,
                "`{}` constrains AltGraph; the table it replaced never did",
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
            let mut panel = CommandPalette::new();
            let event = KeyEvent {
                key: stroke.key,
                modifiers: concrete(stroke.modifiers, shift),
                is_repeat: false,
            };
            assert_eq!(
                panel.resolve_key(&event),
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

/// The panel's ids are the kernel's ids, and the kernel knows all of them.
#[test]
fn the_default_bindings_validate_against_the_editors_registry() {
    let registry = default_registry().expect("the kernel tables are consistent");
    default_keymap()
        .validate(&registry)
        .expect("every command the panel binds must be a registered command");
}

/// Every command the kernel scopes to this panel is a verb it answers, and
/// every verb is scoped here. Catches the drift where a command is added to the
/// kernel's table and forgotten — a `[keys]` line that validates and does
/// nothing, the #110 defect class.
#[test]
fn the_kernels_vocabulary_and_this_panels_verbs_are_the_same_set() {
    let registered: BTreeSet<&str> = panel_command_metas()
        .filter(|meta| meta.mode() == Some(&PALETTE_MODE))
        .map(|meta| meta.id().as_str())
        .collect();
    // Owned, because `Verb::id` returns by value and a borrow of it would not
    // outlive this expression.
    let ids: Vec<CommandId> = Verb::ALL.iter().map(|verb| verb.id()).collect();
    let answered: BTreeSet<&str> = ids.iter().map(CommandId::as_str).collect();
    assert_eq!(registered, answered);
}

// ===== The seam =====

/// ⭐ **The task, end to end.** A line a user could write moves a real key.
///
/// ⚠️ The panel opens with the *first* result selected, so `Down` genuinely
/// travels from there — and the assertion below proves it did before comparing
/// anything to it. A fixture that began where the key lands would make every
/// assertion here vacuous.
#[test]
fn a_configuration_line_moves_a_panel_key() {
    let editor = Editor::with_defaults();
    let mru = CommandMru::default();

    let first = selected(&mut opened(), &editor, &mru);
    let mut walked = opened();
    walked.handle_key(&press(KeyCode::Down), &editor, &mru);
    let second = selected(&mut walked, &editor, &mru);
    assert!(
        matches!(second, PaletteOutcome::Run(_)),
        "the fixture must have a command to land on, got {second:?}"
    );
    assert_ne!(
        second, first,
        "the fixture must have somewhere for `Down` to go, or this proves nothing"
    );

    let mut panel = opened();
    panel.set_user_keymap(&user_layer("ctrl+j", PALETTE_SELECT_NEXT));
    panel.handle_key(
        &KeyEvent {
            key: KeyCode::Char('j'),
            modifiers: Modifiers::ctrl(),
            is_repeat: false,
        },
        &editor,
        &mru,
    );
    assert_eq!(
        selected(&mut panel, &editor, &mru),
        second,
        "`ctrl+j` must select what `Down` selects"
    );
}

/// ⭐ **The trap D-2 exists to close.** A mode-free document binding pushed into
/// this panel whole would apply here too — so a binding on `Down` would stop
/// `Down` moving the selection, having never mentioned the palette.
#[test]
fn a_user_binding_naming_a_document_command_never_reaches_this_panel() {
    let editor = Editor::with_defaults();
    let mru = CommandMru::default();

    let first = selected(&mut opened(), &editor, &mru);
    let mut walked = opened();
    walked.handle_key(&press(KeyCode::Down), &editor, &mru);
    let second = selected(&mut walked, &editor, &mru);
    assert_ne!(
        second, first,
        "`Down` must actually travel, or a key that stopped working looks the same"
    );

    let mut panel = opened();
    panel.set_user_keymap(&user_layer("down", EDIT_DELETE_TO_LINE_END));
    panel.handle_key(&press(KeyCode::Down), &editor, &mru);
    assert_eq!(
        selected(&mut panel, &editor, &mru),
        second,
        "a document binding must not take `Down` away from the palette"
    );
}

/// ⚠️ An unbind carries no command, so it cannot be scoped — and is not
/// dropped. The honest reading of `"escape" = ""` is that the key does nothing,
/// here too.
#[test]
fn an_unbind_suppresses_the_key_in_this_panel_as_well() {
    let editor = Editor::with_defaults();
    let mru = CommandMru::default();
    let mut panel = opened();
    panel.set_user_keymap(&unbind_layer("escape"));
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor, &mru),
        PaletteOutcome::Handled,
        "an unbound `Escape` must not close the palette"
    );
}

/// ⛔ **The assertion this panel needs and the undo tree did not.**
///
/// This panel has a query field, so a key nothing claimed becomes *text*. That
/// makes a suppression dangerous in a way it is not elsewhere: if `"a" = ""`
/// were reported as unclaimed, pressing `a` would type the very character the
/// user asked the editor to stop reacting to. It is why `Resolved` carries a
/// `Suppressed` variant rather than folding it into `Unclaimed`.
#[test]
fn a_suppressed_printable_key_does_not_type_itself_into_the_query() {
    let editor = Editor::with_defaults();
    let mru = CommandMru::default();

    let mut typing = opened();
    typing.handle_key(&press(KeyCode::Char('a')), &editor, &mru);
    assert_eq!(
        typing.query(),
        "a",
        "an unclaimed printable key must type, or the test below proves nothing"
    );

    let mut panel = opened();
    panel.set_user_keymap(&unbind_layer("a"));
    assert_eq!(
        panel.handle_key(&press(KeyCode::Char('a')), &editor, &mru),
        PaletteOutcome::Handled
    );
    assert_eq!(
        panel.query(),
        "",
        "a suppressed key typed itself into the query"
    );
}
