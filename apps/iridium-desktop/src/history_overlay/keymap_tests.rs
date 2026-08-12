//! What the undo tree's keymap guarantees — #117.
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
    EDIT_DELETE_TO_LINE_END, HISTORY_MODE, HISTORY_SELECT_NEXT, default_registry,
    panel_command_metas,
};
use iridium_editor::{
    CommandId, Editor, KeyBinding, KeyCode, KeyEvent, Keymap, ModifierPattern, ModifierState,
    Modifiers,
};

use super::keymap::{DEFAULT_BINDING_COUNT, EVERY_PATTERN, default_keymap};
use super::panel::{HistoryOutcome, HistoryPanel};
use super::resolve::Resolved;
use super::verb::Verb;

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

/// An editor with a history worth walking: root → "a" → "ab" → "abc".
///
/// ⚠️ **Pasted, not `set_content`.** `set_content` replaces the document rather
/// than editing it, so a tree built that way has one node — and every
/// "this key travels" assertion below would then compare a node against itself.
/// The group timeout is zeroed so the three pastes do not coalesce into one.
fn edited_editor() -> Editor {
    let mut editor = Editor::with_defaults();
    editor.state_mut().history.set_group_timeout_ms(0);
    for text in ["a", "b", "c"] {
        editor.paste(text);
    }
    editor
}

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
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

/// ⛔ **D-4, the invisible regression.** The `chord` function this table
/// replaced read `ctrl`, `alt` and `meta` and never looked at shift, so
/// `Shift+Down` moved the selection. A pattern that does not spell shift `Any`
/// *requires it absent* and would silently drop chords that worked.
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
            let mut panel = HistoryPanel::new();
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

/// ⛔ **Every seam test below starts by pressing `Home`, and that is not
/// incidental.**
///
/// A freshly opened panel already has the *newest* state selected, so `Down`,
/// `End` and `PageDown` all move nothing from there — and a test asserting
/// "this key lands where that key lands" would hold for two keys that both did
/// nothing at all. Measured: the D-2 test written without this step **passed
/// with the filtering removed**, which is a test that cannot fail.
///
/// Pressing `Home` first moves the selection to the oldest state, so a key that
/// works and a key that has been taken away land in different places.
fn at_the_oldest(editor: &Editor) -> HistoryPanel {
    let mut panel = HistoryPanel::new();
    panel.open();
    panel.handle_key(&press(KeyCode::Home), editor);
    panel
}

/// Where a panel's selection currently is, as the node `Enter` would jump to.
fn selected(panel: &mut HistoryPanel, editor: &Editor) -> HistoryOutcome {
    panel.handle_key(&press(KeyCode::Enter), editor)
}

/// ⭐ **The task, end to end.** A line a user could write moves a real key.
#[test]
fn a_configuration_line_moves_a_panel_key() {
    let editor = edited_editor();

    // What `Down` from the oldest state reaches, and proof it is somewhere else.
    let oldest = selected(&mut at_the_oldest(&editor), &editor);
    let mut walked = at_the_oldest(&editor);
    walked.handle_key(&press(KeyCode::Down), &editor);
    let expected = selected(&mut walked, &editor);
    assert!(
        matches!(expected, HistoryOutcome::Jump(_)),
        "the fixture must have a node to land on, got {expected:?}"
    );
    assert_ne!(
        expected, oldest,
        "the fixture must have somewhere for `Down` to go, or this proves nothing"
    );

    let mut panel = at_the_oldest(&editor);
    panel.set_user_keymap(&user_layer("ctrl+j", HISTORY_SELECT_NEXT));
    panel.handle_key(&press(KeyCode::Home), &editor);
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
        expected,
        "`ctrl+j` must select what `Down` selects"
    );
}

/// ⭐ **The trap D-2 exists to close.** A mode-free document binding pushed into
/// this panel whole would apply here too — so a binding on `End` would stop
/// `End` reaching the newest state, having never mentioned the undo tree.
#[test]
fn a_user_binding_naming_a_document_command_never_reaches_this_panel() {
    let editor = edited_editor();

    let oldest = selected(&mut at_the_oldest(&editor), &editor);
    let mut walked = at_the_oldest(&editor);
    walked.handle_key(&press(KeyCode::End), &editor);
    let newest = selected(&mut walked, &editor);
    assert_ne!(
        newest, oldest,
        "`End` must actually travel, or a key that stopped working looks the same"
    );

    let mut panel = HistoryPanel::new();
    panel.set_user_keymap(&user_layer("end", EDIT_DELETE_TO_LINE_END));
    panel.open();
    panel.handle_key(&press(KeyCode::Home), &editor);
    panel.handle_key(&press(KeyCode::End), &editor);
    assert_eq!(
        selected(&mut panel, &editor),
        newest,
        "a document binding must not take `End` away from the undo tree"
    );
}

/// ⚠️ An unbind carries no command, so it cannot be scoped — and is not
/// dropped. The honest reading of `"escape" = ""` is that the key does nothing,
/// here too.
#[test]
fn an_unbind_suppresses_the_key_in_this_panel_as_well() {
    let editor = edited_editor();
    let mut panel = HistoryPanel::new();
    panel.open();
    panel.set_user_keymap(&unbind_layer("escape"));
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor),
        HistoryOutcome::Handled,
        "an unbound `Escape` must not close the panel"
    );
}
