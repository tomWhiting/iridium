//! What the explorer's keymap guarantees — #91.
//!
//! Two kinds of assertion live here. The first are about the *table*: every verb
//! reachable, every binding shift-blind, the count documented. The second are
//! about the *seam*: a line a user could write in `config.toml` reaching the
//! panel and moving a real key.
//!
//! ⭐ The second kind is the point of the task, and is deliberately written
//! against `iridium_editor::KeyBinding::parse` — the same parser
//! `iridium_config` calls on the text of the file — rather than against a
//! hand-built binding. A test that constructed the binding itself would prove
//! the panel honours bindings without proving a *configuration file* can produce
//! one.

use std::collections::BTreeSet;

use iridium_editor::commands::builtin::{
    EXPLORER_CONFIRM_MODE, EXPLORER_EDIT_MODE, EXPLORER_MODE, EXPLORER_MOVE_DOWN,
    EXPLORER_REFUSED_MODE, EXPLORER_TOGGLE_PANEL, default_registry, panel_command_metas,
};
use iridium_editor::{CommandId, KeyBinding, KeyCode, Keymap, Modifiers};

use super::keymap::{DEFAULT_BINDING_COUNT, EVERY_PATTERN, default_keymap};
use super::panel::ExplorerOutcome;
use super::tests::support::{chord, opened, press, project};
use super::verb::Verb;

/// A user layer holding one line as `config.toml` would have produced it.
fn user_layer(sequence: &str, command: CommandId) -> Keymap {
    let mut layer = Keymap::new("user");
    layer.push(KeyBinding::parse(sequence, command).expect("the fixture is a key sequence"));
    layer
}

// ===== The table =====

#[test]
fn the_binding_count_is_what_the_module_documents() {
    assert_eq!(default_keymap().len(), DEFAULT_BINDING_COUNT);
}

/// ⭐ Every verb the panel can perform has a default key. The kernel's
/// `every_registered_command_is_bound_except_the_typing_fall_through` skips
/// mode-scoped commands precisely because *this* is where they are guarded, so
/// a verb that lost its binding would otherwise be caught by nothing at all.
#[test]
fn every_verb_is_bound_by_the_default_keymap() {
    let keymap = default_keymap();
    let bound: BTreeSet<String> = keymap
        .bindings()
        .iter()
        .filter_map(|binding| binding.command())
        .map(|id| id.as_str().to_owned())
        .collect();

    for verb in Verb::ALL {
        assert!(
            bound.contains(verb.id().as_str()),
            "`{}` is a verb the panel can perform and no default key runs it",
            verb.id()
        );
    }
}

/// The other direction: nothing in the table names a command that does not
/// exist. A binding to a misspelled id would resolve and then be swallowed by
/// the dispatch's catch-all, which is a key that silently does nothing.
#[test]
fn every_default_binding_names_a_verb_or_one_of_the_two_toggles() {
    for binding in default_keymap().bindings() {
        let Some(id) = binding.command() else {
            panic!("the default table holds a suppression, which it has no reason to");
        };
        let known = Verb::from_id(id).is_some() || id.as_str().starts_with("explorer.toggle");
        assert!(known, "`{id}` is bound by default and nothing answers it");
    }
}

/// ⛔ **D-4, the invisible regression.** The `chord` function these bindings
/// replaced read `ctrl`, `alt` and `meta` and never looked at shift — so
/// `Shift+Down` moved the selection and `Ctrl+Shift+D` struck a row through.
/// A `ModifierPattern` that does not spell shift `Any` *requires it absent*, so
/// a binding that forgot would silently drop those chords while passing every
/// test written against the unshifted spelling.
#[test]
fn every_default_binding_ignores_shift_exactly_as_the_chord_table_did() {
    for binding in default_keymap().bindings() {
        for stroke in binding.sequence() {
            assert_eq!(
                stroke.modifiers.shift,
                iridium_editor::ModifierState::Any,
                "`{}` constrains shift; the table it replaced never did",
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
        assert_eq!(pattern.shift, iridium_editor::ModifierState::Any);
    }
}

/// The panel's ids are the kernel's ids, and the kernel knows all of them —
/// which is what makes a `[keys]` line naming one validate instead of being
/// reported as a typo.
#[test]
fn the_default_bindings_validate_against_the_editors_registry() {
    let registry = default_registry().expect("the kernel tables are consistent");
    default_keymap()
        .validate(&registry)
        .expect("every command the explorer binds must be a registered command");
}

/// Every command the kernel scopes to one of *this* panel's modes is a verb it
/// answers. Catches the drift where a command is added to the kernel table and
/// forgotten here, which would be a `[keys]` line that validates and then does
/// nothing.
///
/// ⚠️ **Filtered by mode, not taken whole.** `panel_command_metas` enumerates
/// every panel's vocabulary — the command palette's among them since #117 — and
/// asking the explorer to answer another panel's verbs would be asking it to be
/// two panels. The mode is what says which vocabulary a command belongs to, and
/// it is the kernel's own answer rather than a list restated here.
#[test]
fn every_registered_explorer_command_is_a_verb_the_panel_answers() {
    let mine = [
        EXPLORER_MODE,
        EXPLORER_EDIT_MODE,
        EXPLORER_CONFIRM_MODE,
        EXPLORER_REFUSED_MODE,
    ];
    let mut seen = 0_usize;
    for meta in
        panel_command_metas().filter(|meta| meta.mode().is_some_and(|mode| mine.contains(mode)))
    {
        seen += 1;
        assert!(
            Verb::from_id(meta.id()).is_some(),
            "`{}` is registered under one of the explorer's modes and the panel does not answer it",
            meta.id()
        );
    }
    // ⛔ A filter that matched nothing would make the assertion above vacuous,
    // and a mode renamed in the kernel is exactly how that happens.
    assert_eq!(
        seen,
        Verb::ALL.len(),
        "the explorer's modes must account for every verb it answers"
    );
}

/// The four modes are the four screens, and each is used.
#[test]
fn every_mode_has_at_least_one_binding() {
    let keymap = default_keymap();
    for mode in [
        EXPLORER_MODE,
        EXPLORER_EDIT_MODE,
        EXPLORER_CONFIRM_MODE,
        EXPLORER_REFUSED_MODE,
    ] {
        assert!(
            keymap
                .bindings()
                .iter()
                .any(|binding| binding.mode() == Some(&mode)),
            "no binding is scoped to `{mode}`"
        );
    }
}

/// A user layer holding one *unbind*, built the way `iridium_config` builds one.
fn unbind_layer(sequence: &str) -> Keymap {
    let (first, rest) =
        KeyBinding::parse_sequence(sequence).expect("the fixture is a key sequence");
    let mut layer = Keymap::new("user");
    layer.push(KeyBinding::unbound(first, &rest));
    layer
}

/// ⛔ **A defect this panel shipped with, found while converting the terminal's
/// palette.**
///
/// The browsing mode has a *filter query*, so a key nothing claimed becomes
/// text. A suppression — `"a" = ""` — resolved to `Unclaimed` along with it, so
/// pressing `a` filtered the tree by the very character the user had asked the
/// editor to stop reacting to.
///
/// Red first: this failed against the code as shipped in `111d72fa`.
#[test]
fn a_suppressed_printable_key_does_not_type_itself_into_the_filter() {
    let directory = project();

    let mut typing = opened(&directory);
    typing.handle_key(&press(KeyCode::Char('a')));
    assert_eq!(
        typing.query.text(),
        "a",
        "an unclaimed printable key must filter, or the assertion below proves nothing"
    );

    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&unbind_layer("a"));
    assert_eq!(
        explorer.handle_key(&press(KeyCode::Char('a'))),
        ExplorerOutcome::Handled
    );
    assert_eq!(
        explorer.query.text(),
        "",
        "a suppressed key typed itself into the filter"
    );
}

// ===== The seam =====

/// ⭐ **The task, end to end.** A line a user could write moves a real key.
#[test]
fn a_configuration_line_moves_a_panel_key() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&user_layer("ctrl+j", EXPLORER_MOVE_DOWN));

    let before = explorer.selected_row();
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('j'), Modifiers::ctrl())),
        ExplorerOutcome::Handled
    );
    assert_ne!(
        explorer.selected_row(),
        before,
        "`\"ctrl+j\" = \"explorer.moveDown\"` must move the selection"
    );
}

/// ⚠️ And the default it did not mention still works. A user layer is an
/// addition, not a replacement: somebody who binds one key has not asked to lose
/// the other twenty-one.
#[test]
fn a_configuration_line_does_not_disturb_the_keys_it_did_not_name() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&user_layer("ctrl+j", EXPLORER_MOVE_DOWN));

    let before = explorer.selected_row();
    explorer.handle_key(&press(KeyCode::Down));
    assert_ne!(explorer.selected_row(), before, "`Down` still moves");
}

/// ⛔ **The trap `set_user_keymap`'s filter exists for.** A mode-free binding
/// applies in every mode, so a user who bound `Tab` for the *document* would
/// have found it had stopped opening the oil buffer — having never mentioned the
/// explorer at all.
#[test]
fn an_editor_binding_in_the_users_layer_does_not_shadow_a_panel_key() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&user_layer(
        "tab",
        CommandId::from_static("edit.toggleLineComment"),
    ));

    explorer.handle_key(&press(KeyCode::Tab));
    assert!(
        explorer.mode.is_editing(),
        "`Tab` must still open the oil buffer when the user bound it for the document"
    );
}

/// A multi-stroke sequence is reachable. Nothing in the default table is a
/// chord, so without the pending state this binding would be accepted by the
/// configuration file and then never fire.
#[test]
fn a_multi_stroke_configuration_binding_fires() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&user_layer("ctrl+k ctrl+j", EXPLORER_MOVE_DOWN));

    let before = explorer.selected_row();
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('k'), Modifiers::ctrl())),
        ExplorerOutcome::Handled
    );
    assert!(
        explorer.has_pending_keys(),
        "the leader must be held, not discarded"
    );
    assert_eq!(
        explorer.selected_row(),
        before,
        "the leader alone must not move anything"
    );

    explorer.handle_key(&chord(KeyCode::Char('j'), Modifiers::ctrl()));
    assert!(!explorer.has_pending_keys(), "the sequence completed");
    assert_ne!(explorer.selected_row(), before);
}

/// A leader followed by a key that cannot continue it abandons the sequence —
/// and the abandoning key does **not** land in the query. It was typed as part
/// of a chord, not as text.
#[test]
fn an_abandoned_sequence_does_not_type_into_the_query() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&user_layer("ctrl+k ctrl+j", EXPLORER_MOVE_DOWN));

    explorer.handle_key(&chord(KeyCode::Char('k'), Modifiers::ctrl()));
    explorer.handle_key(&press(KeyCode::Char('z')));
    assert!(!explorer.has_pending_keys());
    assert!(
        !explorer.is_filtering(),
        "the key that abandoned the chord must not become a filter"
    );
}

/// A held chord is still a chord after the panel changes screen: the pending
/// strokes are dropped when the bindings under them are replaced.
#[test]
fn replacing_the_user_layer_drops_a_half_typed_sequence() {
    let directory = project();
    let mut explorer = opened(&directory);
    explorer.set_user_keymap(&user_layer("ctrl+k ctrl+j", EXPLORER_MOVE_DOWN));
    explorer.handle_key(&chord(KeyCode::Char('k'), Modifiers::ctrl()));
    assert!(explorer.has_pending_keys());

    explorer.set_user_keymap(&user_layer("ctrl+j", EXPLORER_MOVE_DOWN));
    assert!(
        !explorer.has_pending_keys(),
        "a sequence half-typed against the old bindings means nothing against the new"
    );
}

/// D-6: an unbind reaches the panel, and the panel stays closable anyway
/// because the toggle is the editor's command and the face answers it.
#[test]
fn an_unbound_sequence_stops_working_in_the_panel() {
    let directory = project();
    let mut explorer = opened(&directory);

    let mut layer = Keymap::new("user");
    let (first, rest) = KeyBinding::parse_sequence("tab").expect("the fixture is a key sequence");
    layer.push(KeyBinding::unbound(first, &rest));
    explorer.set_user_keymap(&layer);

    explorer.handle_key(&press(KeyCode::Tab));
    assert!(
        !explorer.mode.is_editing(),
        "`\"tab\" = \"\"` must stop Tab opening the oil buffer"
    );
}

/// The toggle chord still closes the panel from the browse screen, and it is
/// answered as the editor's command rather than as one of the panel's verbs.
#[test]
fn the_toggle_chord_still_closes_the_panel() {
    let directory = project();
    let mut explorer = opened(&directory);
    assert!(Verb::from_id(&EXPLORER_TOGGLE_PANEL).is_none());

    let ctrl_alt = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::none()
    };
    assert_eq!(
        explorer.handle_key(&chord(KeyCode::Char('e'), ctrl_alt)),
        ExplorerOutcome::Closed
    );
}
