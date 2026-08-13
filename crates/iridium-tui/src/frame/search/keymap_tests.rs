//! What the terminal search overlay's keymap guarantees — #117.
//!
//! Two kinds of assertion live here. The first are about the *table*: every verb
//! reachable, every binding shift-blind **except the two the old code read shift
//! on**, the count documented. The second are about the *seam*: a line a user
//! could write in `config.toml` reaching the overlay and moving a real key.
//!
//! ⭐ The second kind is the point of the task, and is deliberately written
//! against [`KeyBinding::parse`] — the same parser `iridium_config` calls on the
//! text of the file — rather than against a hand-built binding.
//!
//! ⚠️ These live in their own file rather than in `tests.rs`, which is already
//! 1136 lines against a 1000-line hard limit and must not grow.

#![allow(clippy::expect_used)]

use std::collections::BTreeSet;

use iridium_editor::commands::builtin::{
    EDIT_DELETE_TO_LINE_END, SEARCH_MODE, SEARCH_TOGGLE_REGEX, default_registry,
    panel_command_metas,
};
use iridium_editor::{
    CommandId, Editor, KeyBinding, KeyCode, KeyEvent, Keymap, ModifierPattern, ModifierState,
    Modifiers,
};

use super::keymap::{DEFAULT_BINDING_COUNT, ENTER_PATTERNS, SHIFT_BLIND_PATTERNS, default_keymap};
use super::resolve::Resolved;
use super::verb::Verb;
use super::{SearchOutcome, SearchOverlay};

/// An empty layer: an overlay built with this answers only its own defaults.
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

/// A document with three matches for `ab`, and a search already running.
fn searching(overlay: &mut SearchOverlay) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content("ab\nxx\nab\nyy\nab\n");
    overlay.open(&mut editor);
    for character in "ab".chars() {
        overlay.handle_key(&press(KeyCode::Char(character)), &mut editor);
    }
    editor
}

/// An open overlay answering its defaults alone, and the kernel it drives.
fn opened() -> (SearchOverlay, Editor) {
    let mut overlay = SearchOverlay::new(&no_user_keys());
    let editor = searching(&mut overlay);
    (overlay, editor)
}

/// Where the caret is — how this suite observes "which match are we on".
fn caret(editor: &Editor) -> iridium_editor::Position {
    editor.state().cursor.primary.head
}

// ===== The table =====

#[test]
fn the_binding_count_is_what_the_module_documents() {
    assert_eq!(default_keymap().len(), DEFAULT_BINDING_COUNT);
}

/// Every verb is reachable without touching a configuration file.
#[test]
fn every_verb_the_overlay_answers_is_bound_by_the_default_keymap() {
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
            "`{}` is a verb the overlay answers and no default binding reaches it",
            verb.id()
        );
    }
}

/// And the other way round: the table binds nothing this overlay cannot run.
#[test]
fn every_default_binding_names_a_verb_this_overlay_answers() {
    for binding in default_keymap().bindings() {
        let id = binding
            .command()
            .expect("no default binding is a suppression");
        assert!(
            Verb::from_id(id).is_some(),
            "`{id}` is bound by default and the overlay does not answer it"
        );
    }
}

/// Every binding is scoped to the overlay's mode, which is what keeps these
/// chords out of the document.
///
/// ⚠️ This is about the **binding**, not the command. Two of the verbs are
/// mode-free editor commands (see [`Verb::BORROWED`]); the overlay's own
/// bindings on them are still scoped, which is what stops `Enter` in the
/// document meaning what `Enter` means here.
#[test]
fn every_default_binding_is_scoped_to_the_overlay() {
    for binding in default_keymap().bindings() {
        assert_eq!(
            binding.mode(),
            Some(&SEARCH_MODE),
            "`{}` is not scoped to the search overlay",
            binding.display_sequence()
        );
    }
}

/// ⛔ **The invisible regression, with this panel's exception.** The `chord`
/// function this table replaced read `ctrl`, `alt` and `meta` and never looked
/// at shift or `AltGraph` — *except* on `Enter`, where the arm re-read shift to
/// choose between next and previous. So every binding must spell shift `Any`
/// **but the two `Enter` ones**, and this test names them rather than skipping
/// anything that happens to constrain shift.
#[test]
fn every_default_binding_ignores_shift_except_the_two_enter_spellings() {
    for binding in default_keymap().bindings() {
        for stroke in binding.sequence() {
            assert_eq!(
                stroke.modifiers.alt_graph,
                ModifierState::Any,
                "`{}` constrains AltGraph; the table it replaced never did",
                binding.display_sequence()
            );
            if stroke.key == KeyCode::Enter {
                assert_ne!(
                    stroke.modifiers.shift,
                    ModifierState::Any,
                    "`{}` must decide shift: it is what picks next from previous",
                    binding.display_sequence()
                );
                continue;
            }
            assert_eq!(
                stroke.modifiers.shift,
                ModifierState::Any,
                "`{}` constrains shift; the table it replaced never did",
                binding.display_sequence()
            );
        }
    }
}

/// The same rule at the source, in both directions: the shift-blind patterns
/// really are blind, and the two `Enter` ones between them **partition** shift
/// rather than leaving a state that reaches nothing.
#[test]
fn the_patterns_the_table_offers_are_blind_or_partition_shift() {
    for pattern in SHIFT_BLIND_PATTERNS {
        assert_eq!(pattern.shift, ModifierState::Any);
        assert_eq!(pattern.alt_graph, ModifierState::Any);
    }
    let states: BTreeSet<&str> = ENTER_PATTERNS
        .iter()
        .map(|pattern| match pattern.shift {
            ModifierState::Required => "required",
            ModifierState::Forbidden => "forbidden",
            ModifierState::Any => "any",
        })
        .collect();
    assert_eq!(
        states,
        ["forbidden", "required"].into_iter().collect(),
        "the two `Enter` patterns must cover shift held and shift released"
    );
    for pattern in ENTER_PATTERNS {
        assert_eq!(pattern.alt_graph, ModifierState::Any);
    }
}

/// ⭐ And the same rule *behaviourally*, which the two above cannot say.
///
/// A pattern-level assertion proves the table spells shift `Any`; this proves
/// **resolution honours it**, which is the property a user holding shift
/// actually depends on. `Enter` is excluded and gets its own test below.
#[test]
fn holding_shift_changes_nothing_about_what_a_non_enter_binding_resolves_to() {
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
        if stroke.key == KeyCode::Enter {
            continue;
        }
        let id = binding
            .command()
            .expect("no default binding is a suppression");
        let expected = Verb::from_id(id).expect("every default binding names a verb");

        for shift in [false, true] {
            let mut overlay = SearchOverlay::new(&no_user_keys());
            let event = KeyEvent {
                key: stroke.key,
                modifiers: concrete(stroke.modifiers, shift),
                is_repeat: false,
            };
            assert_eq!(
                overlay.resolve_key(&event),
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

/// ⭐ **The exception, proven behaviourally.** `Enter` goes forward and
/// `Shift+Enter` goes back — the one place in seven panels where shift changes
/// which verb runs.
///
/// ⛔ **This test was originally written as "press `Enter`, then `Shift+Enter`,
/// and assert the caret came back", and a mutation swapping the two bindings
/// did not fail it.** That shape only proves the pair are inverses of each
/// other; it never says which one goes forward, so inverting both keeps it
/// green. It is now written against an *independent oracle*: `Down` and `Up` are
/// separately bound to the same two verbs — the whole reason they exist, since
/// the legacy encoding cannot express `Shift+Enter` — so what they select is a
/// witness `Enter` can be compared against without comparing it to itself.
///
/// A test that compares an artefact against itself agrees with the writer by
/// sharing the writer's mistake.
#[test]
fn enter_goes_where_down_goes_and_shift_enter_goes_where_up_goes() {
    /// Where the caret lands after one key, from a freshly-opened overlay.
    fn after(event: &KeyEvent) -> iridium_editor::Position {
        let mut overlay = SearchOverlay::new(&no_user_keys());
        let mut editor = searching(&mut overlay);
        overlay.handle_key(event, &mut editor);
        caret(&editor)
    }

    let start = {
        let mut overlay = SearchOverlay::new(&no_user_keys());
        caret(&searching(&mut overlay))
    };
    let forward = after(&press(KeyCode::Down));
    let backward = after(&press(KeyCode::Up));

    // The oracle itself is pinned first, absolutely rather than relatively:
    // `Down` must move to a *later* line and `Up` must not, or the two
    // comparisons below would hold just as well with the oracle inverted.
    assert!(
        forward.line > start.line,
        "`Down` must move forward from the first match: {start:?} -> {forward:?}"
    );
    assert!(
        backward.line != forward.line,
        "`Up` and `Down` must reach different matches, or this proves nothing"
    );

    let shift_enter = KeyEvent {
        key: KeyCode::Enter,
        modifiers: Modifiers {
            shift: true,
            ..Modifiers::none()
        },
        is_repeat: false,
    };
    assert_eq!(
        after(&press(KeyCode::Enter)),
        forward,
        "`Enter` must select what `Down` selects"
    );
    assert_eq!(
        after(&shift_enter),
        backward,
        "`Shift+Enter` must select what `Up` selects"
    );
}

/// ⚠️ One binding covers `r` and `R` both, where the old arms read
/// `Char('r' | 'R')`. `StrokePattern::matches` ASCII-lowercases a character
/// keycode, so the second spelling was never the extra coverage it looked like.
#[test]
fn the_option_chords_answer_a_capital_letter_as_well() {
    let mut overlay = SearchOverlay::new(&no_user_keys());
    let event = KeyEvent {
        key: KeyCode::Char('R'),
        modifiers: Modifiers {
            shift: true,
            alt: true,
            ..Modifiers::none()
        },
        is_repeat: false,
    };
    assert_eq!(
        overlay.resolve_key(&event),
        Resolved::Verb(Verb::ToggleRegex)
    );
}

/// The overlay's ids are the kernel's ids, and the kernel knows all of them.
#[test]
fn the_default_bindings_validate_against_the_editors_registry() {
    let registry = default_registry().expect("the kernel tables are consistent");
    default_keymap()
        .validate(&registry)
        .expect("every command the overlay binds must be a registered command");
}

/// ⭐ **The vocabulary, in two halves, because there are two.**
///
/// Every `SEARCH_MODE`-scoped command in the kernel is a verb this overlay
/// answers, and every verb *except the borrowed ones* is `SEARCH_MODE`-scoped.
/// Catches the drift where a command is added to the kernel's table and
/// forgotten — a `[keys]` line that validates and does nothing, the #110 defect
/// class — without the test having to pretend there is only one source.
#[test]
fn the_kernels_search_table_and_this_overlays_own_verbs_are_the_same_set() {
    let registered: BTreeSet<&str> = panel_command_metas()
        .filter(|meta| meta.mode() == Some(&SEARCH_MODE))
        .map(|meta| meta.id().as_str())
        .collect();
    // Owned, because `Verb::id` returns by value and a borrow of it would not
    // outlive this expression.
    let ids: Vec<CommandId> = Verb::ALL
        .iter()
        .filter(|verb| !Verb::BORROWED.contains(verb))
        .map(|verb| verb.id())
        .collect();
    let answered: BTreeSet<&str> = ids.iter().map(CommandId::as_str).collect();
    assert_eq!(registered, answered);
}

/// ⭐ **The other half.** Each borrowed id is a real, registered command, and is
/// **mode-free** — which is the whole reason it could be borrowed. If one ever
/// gained a mode, scoping it here would stop being harmless and this fails.
#[test]
fn every_borrowed_verb_names_a_registered_mode_free_command() {
    let registry = default_registry().expect("the kernel tables are consistent");
    for verb in Verb::BORROWED {
        let id = verb.id();
        let meta = registry
            .get(id.as_str())
            .expect("a borrowed verb must name a registered command");
        assert_eq!(
            meta.mode(),
            None,
            "`{id}` is borrowed from the editor and must stay mode-free"
        );
    }
}

// ===== The seam =====

/// ⭐ **The task, end to end.** A line a user could write moves a real key.
#[test]
fn a_configuration_line_moves_an_overlay_key() {
    let (mut plain, mut editor) = opened();
    let before = plain.options().regex;
    plain.handle_key(
        &KeyEvent {
            key: KeyCode::Char('r'),
            modifiers: Modifiers {
                alt: true,
                ..Modifiers::none()
            },
            is_repeat: false,
        },
        &mut editor,
    );
    assert_ne!(
        plain.options().regex,
        before,
        "`Alt+R` must actually toggle, or this proves nothing"
    );

    let mut overlay = SearchOverlay::new(&user_layer("ctrl+alt+g", SEARCH_TOGGLE_REGEX));
    let mut editor = searching(&mut overlay);
    let before = overlay.options().regex;
    overlay.handle_key(
        &KeyEvent {
            key: KeyCode::Char('g'),
            modifiers: Modifiers {
                ctrl: true,
                alt: true,
                ..Modifiers::none()
            },
            is_repeat: false,
        },
        &mut editor,
    );
    assert_ne!(
        overlay.options().regex,
        before,
        "`ctrl+alt+g` must toggle the regex option"
    );
}

/// ⭐ **The trap the mode exists to close.** A mode-free document binding pushed
/// into this stack whole would apply here too — so a binding on `End` would stop
/// `End` reaching the end of the query, having never mentioned the overlay.
#[test]
fn a_user_binding_naming_a_document_command_never_reaches_this_overlay() {
    let mut overlay = SearchOverlay::new(&user_layer("end", EDIT_DELETE_TO_LINE_END));
    let mut editor = searching(&mut overlay);
    overlay.handle_key(&press(KeyCode::Home), &mut editor);
    overlay.handle_key(&press(KeyCode::End), &mut editor);
    // Typing now appends, which it could not do if `End` had been taken away.
    overlay.handle_key(&press(KeyCode::Char('c')), &mut editor);
    assert_eq!(
        overlay.query(),
        "abc",
        "a document binding must not take `End` away from the overlay"
    );
}

/// ⚠️ An unbind carries no command, so it cannot be scoped — and is not
/// dropped. The honest reading of `"escape" = ""` is that the key does nothing,
/// here too.
#[test]
fn an_unbind_suppresses_the_key_in_this_overlay_as_well() {
    let mut overlay = SearchOverlay::new(&unbind_layer("escape"));
    let mut editor = searching(&mut overlay);
    assert_eq!(
        overlay.handle_key(&press(KeyCode::Escape), &mut editor),
        SearchOutcome::Handled,
        "an unbound `Escape` must not close the overlay"
    );
}

/// ⛔ **The assertion that puts this overlay in the dangerous class.**
///
/// It has two text fields, so a key nothing claimed becomes *text*. That makes a
/// suppression dangerous in a way it is not on the undo tree: if `"a" = ""` were
/// reported as unclaimed, pressing `a` would type the very character the user
/// asked the editor to stop reacting to. It is why `Resolved` carries a
/// `Suppressed` variant rather than folding it into `Unclaimed`.
#[test]
fn a_suppressed_printable_key_does_not_type_itself_into_the_query() {
    let (mut typing, mut editor) = opened();
    typing.handle_key(&press(KeyCode::Char('c')), &mut editor);
    assert_eq!(
        typing.query(),
        "abc",
        "an unclaimed printable key must type, or the test below proves nothing"
    );

    let mut overlay = SearchOverlay::new(&unbind_layer("c"));
    let mut editor = searching(&mut overlay);
    assert_eq!(
        overlay.handle_key(&press(KeyCode::Char('c')), &mut editor),
        SearchOutcome::Handled
    );
    assert_eq!(
        overlay.query(),
        "ab",
        "a suppressed key typed itself into the query"
    );
}

/// ⛔ **And into the replacement, which is worse.** That field's text is what
/// `Ctrl+Alt+R` writes into the document, so a suppressed key typing itself
/// there does not just look wrong — it edits the file with a character the user
/// unbound.
#[test]
fn a_suppressed_printable_key_does_not_type_itself_into_the_replacement() {
    let mut overlay = SearchOverlay::new(&unbind_layer("z"));
    let mut editor = searching(&mut overlay);
    overlay.handle_key(&press(KeyCode::Tab), &mut editor);
    overlay.handle_key(&press(KeyCode::Char('z')), &mut editor);
    assert_eq!(
        overlay.replacement(),
        "",
        "a suppressed key typed itself into the replacement text"
    );
}

/// ⭐ **What being non-modal means, and the thing no other converted panel
/// asserts.** A key nothing claimed and that is not text stays the *host's*, so
/// a binding such as save is not dead while the overlay is open.
#[test]
fn an_unclaimed_chord_is_left_to_the_host() {
    let (mut overlay, mut editor) = opened();
    let save = KeyEvent {
        key: KeyCode::Char('s'),
        modifiers: Modifiers::ctrl(),
        is_repeat: false,
    };
    assert_eq!(
        overlay.handle_key(&save, &mut editor),
        SearchOutcome::Ignored,
        "the overlay is not modal: an unbound chord must reach the host"
    );
}
