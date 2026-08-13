//! What the terminal undo-tree panel's keymap guarantees — #117.
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
    EDIT_DELETE_TO_LINE_END, HISTORY_MODE, HISTORY_SELECT_PREVIOUS, default_registry,
    panel_command_metas,
};
use iridium_editor::{
    CommandId, Editor, KeyBinding, KeyCode, KeyEvent, Keymap, ModifierPattern, ModifierState,
    Modifiers,
};

use super::keymap::{DEFAULT_BINDING_COUNT, EVERY_PATTERN, default_keymap};
use super::resolve::Resolved;
use super::verb::Verb;
use super::{HistoryOutcome, HistoryPanel};

/// An empty layer: a panel built with this answers only its own defaults.
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

/// A kernel whose history is a straight line of three states, so a selection
/// has somewhere to travel in both directions.
fn linear_history() -> Editor {
    let mut editor = Editor::with_defaults();
    // Without this the two pastes coalesce into one undo step and the tree has
    // one node, which would make every movement assertion below vacuous.
    editor.state_mut().history.set_group_timeout_ms(0);
    editor.paste("a");
    editor.paste("b");
    editor
}

/// An open panel answering its defaults alone.
fn opened() -> HistoryPanel {
    let mut panel = HistoryPanel::new(&no_user_keys());
    panel.open();
    panel
}

/// Which state `Enter` would jump to — the panel's selection, observed as the
/// host observes it.
fn selected(panel: &mut HistoryPanel, editor: &Editor) -> HistoryOutcome {
    panel.handle_key(&press(KeyCode::Enter), editor)
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

/// Every binding is scoped to the panel's mode, which is what keeps these
/// chords out of the document.
#[test]
fn every_default_binding_is_scoped_to_the_panel() {
    for binding in default_keymap().bindings() {
        assert_eq!(
            binding.mode(),
            Some(&HISTORY_MODE),
            "`{}` is not scoped to the undo tree",
            binding.display_sequence()
        );
    }
}

/// ⛔ **The invisible regression.** The `chord` function this table replaced
/// read `ctrl`, `alt` and `meta` and never looked at shift or `AltGraph`. A
/// pattern that does not spell them `Any` *requires them absent*, and would
/// silently drop chords that work today.
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
            let mut panel = HistoryPanel::new(&no_user_keys());
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

/// ⚠️ The one binding whose old `match` arm read `Char('h' | 'H')`.
///
/// `StrokePattern::matches` ASCII-lowercases a character keycode, so the single
/// binding on `'h'` must answer a shifted `H` too. Written out rather than left
/// to the ratchet above, because that ratchet varies the *modifier* and this is
/// about the **keycode** the terminal delivers.
#[test]
fn the_close_chord_answers_a_capital_h_as_well() {
    let mut panel = HistoryPanel::new(&no_user_keys());
    let event = KeyEvent {
        key: KeyCode::Char('H'),
        modifiers: Modifiers {
            shift: true,
            ctrl: true,
            alt: true,
            meta: false,
            alt_graph: false,
        },
        is_repeat: false,
    };
    assert_eq!(panel.resolve_key(&event), Resolved::Verb(Verb::Dismiss));
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
        .filter(|meta| meta.mode() == Some(&HISTORY_MODE))
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
/// ⚠️ The panel opens on the *current* state, which in this fixture is the
/// newest — so `Up` genuinely travels, and the assertion below proves it did
/// before comparing anything to it. A fixture that began where the key lands
/// would make every assertion here vacuous.
#[test]
fn a_configuration_line_moves_a_panel_key() {
    let editor = linear_history();

    let current = selected(&mut opened(), &editor);
    let mut walked = opened();
    walked.handle_key(&press(KeyCode::Up), &editor);
    let previous = selected(&mut walked, &editor);
    assert!(
        matches!(previous, HistoryOutcome::Jump(_)),
        "the fixture must have a state to land on, got {previous:?}"
    );
    assert_ne!(
        previous, current,
        "the fixture must have somewhere for `Up` to go, or this proves nothing"
    );

    let mut panel = HistoryPanel::new(&user_layer("ctrl+j", HISTORY_SELECT_PREVIOUS));
    panel.open();
    panel.handle_key(
        &KeyEvent {
            key: KeyCode::Char('j'),
            modifiers: Modifiers::ctrl(),
            is_repeat: false,
        },
        &editor,
    );
    assert_eq!(
        selected(&mut panel, &editor),
        previous,
        "`ctrl+j` must select what `Up` selects"
    );
}

/// ⭐ **The trap the mode exists to close.** A mode-free document binding
/// pushed into this panel whole would apply here too — so a binding on `Up`
/// would stop `Up` moving the selection, having never mentioned the undo tree.
#[test]
fn a_user_binding_naming_a_document_command_never_reaches_this_panel() {
    let editor = linear_history();

    let current = selected(&mut opened(), &editor);
    let mut walked = opened();
    walked.handle_key(&press(KeyCode::Up), &editor);
    let previous = selected(&mut walked, &editor);
    assert_ne!(
        previous, current,
        "`Up` must actually travel, or a key that stopped working looks the same"
    );

    let mut panel = HistoryPanel::new(&user_layer("up", EDIT_DELETE_TO_LINE_END));
    panel.open();
    panel.handle_key(&press(KeyCode::Up), &editor);
    assert_eq!(
        selected(&mut panel, &editor),
        previous,
        "a document binding must not take `Up` away from the undo tree"
    );
}

/// ⚠️ An unbind carries no command, so it cannot be scoped — and is not
/// dropped. The honest reading of `"escape" = ""` is that the key does nothing,
/// here too.
#[test]
fn an_unbind_suppresses_the_key_in_this_panel_as_well() {
    let editor = linear_history();
    let mut panel = HistoryPanel::new(&unbind_layer("escape"));
    panel.open();
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor),
        HistoryOutcome::Handled,
        "an unbound `Escape` must not close the panel"
    );
}

/// ⛔ **Why this panel's `Resolved` has no `Suppressed` variant.**
///
/// The palette and the file explorer both have a field, so an unclaimed
/// printable key becomes *text* — and a suppression reported as unclaimed would
/// type the very character the user unbound. This panel has no field, so both
/// cases are the same nothing. This test is what makes that claim measurable
/// rather than a comment: a printable key nobody bound changes no selection and
/// does not close the panel.
#[test]
fn an_unclaimed_printable_key_is_swallowed_and_changes_nothing() {
    let editor = linear_history();
    let before = selected(&mut opened(), &editor);

    let mut panel = opened();
    assert_eq!(
        panel.handle_key(&press(KeyCode::Char('a')), &editor),
        HistoryOutcome::Handled,
        "the panel is modal: an unbound letter must not fall through"
    );
    assert_eq!(
        selected(&mut panel, &editor),
        before,
        "an unbound letter moved the selection"
    );
}
