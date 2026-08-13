//! What the palette's keymap guarantees — #117.
//!
//! Two kinds of assertion live here. The first are about the *table*: every verb
//! reachable, every binding shift-blind, the count documented. The second are
//! about the *seam*: a line a user could write in `config.toml` reaching the
//! panel and moving a real key.
//!
//! ⭐ The second kind is the point of the task, and is deliberately written
//! against [`KeyBinding::parse`] — the same parser `iridium_config` calls on the
//! text of the file — rather than against a hand-built binding. A test that
//! constructed the binding itself would prove the panel honours bindings without
//! proving a *configuration file* can produce one.

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
use super::panel::{CommandPalette, PaletteOutcome};
use super::resolve::Resolved;
use super::verb::Verb;

/// A user layer holding one line as `config.toml` would have produced it.
fn user_layer(sequence: &str, command: CommandId) -> Keymap {
    let mut layer = Keymap::new("user");
    layer.push(KeyBinding::parse(sequence, command).expect("the fixture is a key sequence"));
    layer
}

/// A user layer holding one *unbind* — a sequence bound to nothing.
///
/// Built the way `iridium_config::keys` builds one from `"escape" = ""`, so
/// this is the shape a configuration file really produces.
fn unbind_layer(sequence: &str) -> Keymap {
    let (first, rest) =
        KeyBinding::parse_sequence(sequence).expect("the fixture is a key sequence");
    let mut layer = Keymap::new("user");
    layer.push(KeyBinding::unbound(first, &rest));
    layer
}

/// An open palette over a kernel carrying this face's commands and keys.
fn open_palette() -> (CommandPalette, Editor, CommandMru) {
    let mut panel = CommandPalette::new();
    panel.open();
    let mut editor = Editor::with_defaults();
    for meta in crate::commands::command_metas() {
        editor
            .register_command(meta)
            .expect("the kernel accepted the command");
    }
    editor
        .push_keymap(crate::commands::keymap())
        .expect("the keymap validates");
    (panel, editor, CommandMru::default())
}

// ===== The table =====

#[test]
fn the_binding_count_is_what_the_module_documents() {
    assert_eq!(default_keymap().len(), DEFAULT_BINDING_COUNT);
}

/// Every verb is reachable without touching a configuration file.
///
/// A verb the kernel names, the panel answers and nothing binds would be a
/// command that exists only for somebody who already knows it exists.
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
            "`{}` is a verb the palette answers and no default binding reaches it",
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
            "`{id}` is bound by default and the palette does not answer it"
        );
    }
}

/// Every binding is scoped to the palette's mode, which is what keeps these
/// chords out of the document.
#[test]
fn every_default_binding_is_scoped_to_the_palette() {
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
/// replaced read `ctrl`, `alt` and `meta` and never looked at shift, so
/// `Shift+Down` moved the selection. A pattern that does not spell shift `Any`
/// *requires it absent* and would silently drop chords that worked.
///
/// #91 measured that nothing else catches this — its whole workspace suite
/// passed with the shift state tightened — so this panel carries its own.
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
/// Presses every default binding a second time with shift held and asserts the
/// same verb comes back. A pattern-level assertion proves the table spells shift
/// `Any`; this proves **resolution honours it**, which is the property a user
/// holding shift actually depends on.
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
///
/// `Any` on anything other than shift is answered *absent*, because the chord
/// table these bindings replaced held no `AltGraph` — pressing it would be
/// asking a different question than the one this ratchet is about.
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

/// The panel's ids are the kernel's ids, and the kernel knows all of them —
/// which is what makes a `[keys]` line naming one validate instead of being
/// reported as a typo.
#[test]
fn the_default_bindings_validate_against_the_editors_registry() {
    let registry = default_registry().expect("the kernel tables are consistent");
    default_keymap()
        .validate(&registry)
        .expect("every command the palette binds must be a registered command");
}

/// Every command the kernel scopes to the palette is a verb this panel answers.
///
/// Catches the drift where a command is added to the kernel's table and
/// forgotten here, which would be a `[keys]` line that validates and then does
/// nothing — the #110 defect class.
#[test]
fn every_command_the_kernel_scopes_to_this_panel_is_a_verb_it_answers() {
    for meta in panel_command_metas().filter(|meta| meta.mode() == Some(&PALETTE_MODE)) {
        assert!(
            Verb::from_id(meta.id()).is_some(),
            "`{}` is scoped to the palette and the panel does not answer it",
            meta.id()
        );
    }
}

/// And the other way round, which is what keeps [`PALETTE_MODE`] honest: the
/// mode this panel resolves in is the mode the kernel scoped its verbs to.
#[test]
fn every_verb_is_scoped_to_the_palettes_mode_by_the_kernel() {
    for verb in Verb::ALL {
        let meta = panel_command_metas()
            .find(|meta| meta.id() == &verb.id())
            .unwrap_or_else(|| panic!("`{}` is not registered by the kernel", verb.id()));
        assert_eq!(
            meta.mode(),
            Some(&PALETTE_MODE),
            "`{}` is answered by this panel and scoped elsewhere",
            verb.id()
        );
    }
}

// ===== The seam =====

/// ⭐ **The task, end to end.** A line a user could write moves a real key.
#[test]
fn a_configuration_line_moves_a_palette_key() {
    let (mut panel, editor, mru) = open_palette();
    panel.set_user_keymap(&user_layer("ctrl+j", PALETTE_SELECT_NEXT));

    // The bound chord now moves the selection, which `Down` also does — so the
    // proof is that the two land on the same entry rather than that either did
    // something.
    let mut baseline = CommandPalette::new();
    baseline.open();
    baseline.handle_key(&press(KeyCode::Down), &editor, &mru);
    let expected = baseline.handle_key(&press(KeyCode::Enter), &editor, &mru);

    panel.handle_key(&ctrl('j'), &editor, &mru);
    assert_eq!(
        panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
        expected,
        "`ctrl+j` must select what `Down` selects"
    );
}

/// ⭐ **The trap D-2 exists to close.** A user's layer holds all their bindings,
/// most of them mode-free editor ones. Pushed into this panel whole, a mode-free
/// binding would apply in every mode — so a document binding on `Backspace`
/// would stop `Backspace` deleting in the query, having never mentioned the
/// palette.
#[test]
fn a_user_binding_naming_a_document_command_never_reaches_this_panel() {
    let (mut panel, editor, mru) = open_palette();
    panel.set_user_keymap(&user_layer("backspace", EDIT_DELETE_TO_LINE_END));

    panel.handle_key(&press(KeyCode::Char('a')), &editor, &mru);
    panel.handle_key(&press(KeyCode::Char('b')), &editor, &mru);
    assert_eq!(panel.query(), "ab");
    panel.handle_key(&press(KeyCode::Backspace), &editor, &mru);
    assert_eq!(
        panel.query(),
        "a",
        "a document binding must not take `Backspace` away from the query"
    );
}

/// ⚠️ An unbind carries no command, so it cannot be scoped — and is not
/// dropped. The honest reading of `"escape" = ""` is that the key does nothing,
/// here too.
#[test]
fn an_unbind_suppresses_the_key_in_this_panel_as_well() {
    let (mut panel, editor, mru) = open_palette();
    panel.set_user_keymap(&unbind_layer("escape"));
    assert_eq!(
        panel.handle_key(&press(KeyCode::Escape), &editor, &mru),
        PaletteOutcome::Handled,
        "an unbound `Escape` must not close the panel"
    );
}

/// Re-reading a configuration file must not stack two copies of the user's
/// bindings — the reason `set_user_keymap` rebuilds the stack rather than
/// pushing onto it.
#[test]
fn setting_the_user_keymap_twice_leaves_one_layer() {
    let (mut panel, editor, mru) = open_palette();
    let layer = user_layer("ctrl+j", PALETTE_SELECT_NEXT);
    panel.set_user_keymap(&layer);
    panel.set_user_keymap(&layer);

    let mut once = CommandPalette::new();
    once.open();
    once.set_user_keymap(&layer);
    once.handle_key(&ctrl('j'), &editor, &mru);
    let expected = once.handle_key(&press(KeyCode::Enter), &editor, &mru);

    panel.handle_key(&ctrl('j'), &editor, &mru);
    assert_eq!(
        panel.handle_key(&press(KeyCode::Enter), &editor, &mru),
        expected,
        "a second reload must move the selection once, not twice"
    );
}

/// ⛔ **A defect this panel shipped with, found while converting the terminal's.**
///
/// This panel has a query field, so a key nothing claimed becomes *text*. A
/// suppression — `"a" = ""` — resolved to `Unclaimed` along with it, so pressing
/// `a` typed the very character the user had asked the editor to stop reacting
/// to. `resolve_key`'s own comment said "both end the sequence; only one reaches
/// the query field", and the code did not do that.
///
/// Red first: this failed against the code as shipped in `56bd0e2f`.
#[test]
fn a_suppressed_printable_key_does_not_type_itself_into_the_query() {
    let editor = Editor::with_defaults();
    let mru = CommandMru::default();

    let mut typing = CommandPalette::new();
    typing.open();
    typing.handle_key(&press(KeyCode::Char('a')), &editor, &mru);
    assert_eq!(
        typing.query(),
        "a",
        "an unclaimed printable key must type, or the assertion below proves nothing"
    );

    let mut panel = CommandPalette::new();
    panel.open();
    panel.set_user_keymap(&unbind_layer("a"));
    panel.handle_key(&press(KeyCode::Char('a')), &editor, &mru);
    assert_eq!(
        panel.query(),
        "",
        "a suppressed key typed itself into the query"
    );
}

/// A key press with no modifiers.
fn press(key: KeyCode) -> KeyEvent {
    KeyEvent {
        key,
        modifiers: Modifiers::none(),
        is_repeat: false,
    }
}

/// A bare `Ctrl` chord.
fn ctrl(character: char) -> KeyEvent {
    KeyEvent {
        key: KeyCode::Char(character),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    }
}
