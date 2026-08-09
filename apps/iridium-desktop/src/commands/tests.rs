//! Every assertion about this face's commands and the keys that reach them.
//!
//! Split out of [`commands`](super) on the file-size bar, which the single
//! file had passed. Nothing about what is asserted changed with the move.

use std::collections::BTreeSet;

use iridium_editor::commands::builtin::{
    AST_EXPAND_SELECTION, AST_SHRINK_SELECTION, CURSOR_DOCUMENT_END, CURSOR_DOCUMENT_END_SELECT,
    CURSOR_DOCUMENT_START, CURSOR_DOCUMENT_START_SELECT, CURSOR_LINE_END, CURSOR_LINE_END_SELECT,
    CURSOR_LINE_START, CURSOR_LINE_START_SELECT, CURSOR_WORD_LEFT, CURSOR_WORD_LEFT_SELECT,
    CURSOR_WORD_RIGHT, CURSOR_WORD_RIGHT_SELECT, EDIT_DELETE_TO_LINE_END,
    EDIT_DELETE_TO_LINE_START, EDIT_DELETE_WORD_BACKWARD, EDIT_DELETE_WORD_FORWARD,
};
use iridium_editor::commands::{default_keymap_stack, default_non_modal_keymap};
use iridium_editor::{
    CommandId, Editor, KeyCode, KeyHintIndex, KeyPress, Keymap, KeymapResolver, KeymapStack,
    Modifiers, StrokePattern,
};

use super::ids::{COMMAND_COUNT, COMMANDS, FILE_SAVE, FILE_SAVE_FORCE, command_metas};
use super::keymap::{BINDING_COUNT, BINDINGS, MAC_CHORDS, keymap};

/// The whole stack a session resolves against: the kernel's default layer
/// with this face's layer pushed on top, exactly as `DesktopApp::new`
/// builds it. A chord is only bound if it resolves *here* — a row in the
/// table proves nothing on its own, because the layer underneath binds the
/// same keys under looser patterns.
fn session_stack() -> KeymapStack {
    let mut stack = default_keymap_stack();
    stack.push(keymap());
    stack
}

/// Modifiers with only the named bits held.
const fn held(shift: bool, alt: bool, meta: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl: false,
        alt,
        meta,
        alt_graph: false,
    }
}

/// The `⌃⇧⌘` chord's modifiers — the one combination [`held`] cannot
/// spell, named rather than given a fourth `bool` parameter.
const fn ctrl_shift_meta() -> Modifiers {
    Modifiers {
        shift: true,
        ctrl: true,
        alt: false,
        meta: true,
        alt_graph: false,
    }
}

/// Asserts one keypress resolves to `expected` through the full stack.
fn resolves_to(stack: &KeymapStack, key: KeyCode, modifiers: Modifiers, expected: &CommandId) {
    let mut resolver = KeymapResolver::new();
    let outcome = resolver.resolve(stack, KeyPress::new(key, modifiers));
    assert_eq!(
        outcome.command().map(CommandId::as_str),
        Some(expected.as_str()),
        "{key:?} with {modifiers:?} did not resolve to {expected}"
    );
}

#[test]
fn the_mac_navigation_chords_resolve_through_the_whole_stack() {
    let stack = session_stack();
    let cases: &[(KeyCode, Modifiers, &CommandId)] = &[
        (KeyCode::Left, held(false, true, false), &CURSOR_WORD_LEFT),
        (KeyCode::Right, held(false, true, false), &CURSOR_WORD_RIGHT),
        (KeyCode::Left, held(false, false, true), &CURSOR_LINE_START),
        (KeyCode::Right, held(false, false, true), &CURSOR_LINE_END),
        (
            KeyCode::Left,
            held(true, false, true),
            &CURSOR_LINE_START_SELECT,
        ),
        (
            KeyCode::Right,
            held(true, false, true),
            &CURSOR_LINE_END_SELECT,
        ),
        (
            KeyCode::Up,
            held(false, false, true),
            &CURSOR_DOCUMENT_START,
        ),
        (
            KeyCode::Down,
            held(false, false, true),
            &CURSOR_DOCUMENT_END,
        ),
        (
            KeyCode::Up,
            held(true, false, true),
            &CURSOR_DOCUMENT_START_SELECT,
        ),
        (
            KeyCode::Down,
            held(true, false, true),
            &CURSOR_DOCUMENT_END_SELECT,
        ),
    ];
    for &(key, modifiers, expected) in cases {
        resolves_to(&stack, key, modifiers, expected);
    }
}

#[test]
fn the_mac_deletion_chords_resolve_through_the_whole_stack() {
    let stack = session_stack();
    let cases: &[(KeyCode, Modifiers, &CommandId)] = &[
        (
            KeyCode::Backspace,
            held(false, true, false),
            &EDIT_DELETE_WORD_BACKWARD,
        ),
        (
            KeyCode::Delete,
            held(false, true, false),
            &EDIT_DELETE_WORD_FORWARD,
        ),
        (
            KeyCode::Backspace,
            held(false, false, true),
            &EDIT_DELETE_TO_LINE_START,
        ),
        (
            KeyCode::Delete,
            held(false, false, true),
            &EDIT_DELETE_TO_LINE_END,
        ),
    ];
    for &(key, modifiers, expected) in cases {
        resolves_to(&stack, key, modifiers, expected);
    }
}

#[test]
fn the_word_select_chords_belong_to_word_select() {
    // The contested pair, ruled. ⌥⇧← / ⌥⇧→ were left to the default
    // keymap's syntax verbs while the collision was unresolved; Tom asked
    // for word-by-word selection on the chord every other mac editor puts
    // it on, so this face takes it back. The predecessor of this test
    // pinned the opposite resolution under the name
    // `the_word_select_chords_are_left_to_the_syntax_verbs`, and the
    // rename is the record that a deliberate decision replaced a
    // deliberate decision rather than drifting into one.
    let stack = session_stack();
    resolves_to(
        &stack,
        KeyCode::Left,
        held(true, true, false),
        &CURSOR_WORD_LEFT_SELECT,
    );
    resolves_to(
        &stack,
        KeyCode::Right,
        held(true, true, false),
        &CURSOR_WORD_RIGHT_SELECT,
    );
}

#[test]
fn the_syntax_verbs_keep_a_home_of_their_own() {
    // Expand/shrink did not lose the chord, it moved: ⌃⇧⌘← / ⌃⇧⌘→, where
    // VS Code puts them on mac. A verb displaced by a rebinding and left
    // unbound would be the rebinding quietly deleting a feature.
    let stack = session_stack();
    resolves_to(
        &stack,
        KeyCode::Left,
        ctrl_shift_meta(),
        &AST_SHRINK_SELECTION,
    );
    resolves_to(
        &stack,
        KeyCode::Right,
        ctrl_shift_meta(),
        &AST_EXPAND_SELECTION,
    );
}

/// Verbs the kernel binds under `Ctrl` that deliberately have no
/// Ctrl-free chord on this face.
///
/// A list with a reason against each, so "Ctrl only" is a decision written
/// down rather than a row nobody got to. Empty is the goal.
const CTRL_ONLY: &[(&str, &str)] = &[];

/// Whether any key that reaches `id` needs no `Ctrl` held.
///
/// ⚠️ **Not "has a ⌘ chord", and the difference is the whole test.** The
/// mac spelling of a `Ctrl` chord is sometimes ⌘ and sometimes ⌥ — word
/// motion is `⌥←` on every mac editor, not `⌘←`, and `⌥⌫` deletes a word
/// — so demanding ⌘ specifically would fail on four verbs this face binds
/// correctly. And demanding "⌘ *or* ⌥" would *pass* `⌃⌥E`, which holds
/// `Alt` and is exactly the chord that sent Tom looking for a ⌘ one.
///
/// What a mac hand actually objects to is `Ctrl`. So that is what is
/// asked.
fn reachable_without_ctrl(hints: &KeyHintIndex, id: &str) -> bool {
    hints.hints_for(id).iter().any(|hint| {
        hint.sequence()
            .first()
            .is_some_and(|stroke| !stroke.modifiers.required_modifiers().ctrl)
    })
}

#[test]
fn every_ctrl_chord_a_mac_hand_reaches_for_has_a_meta_spelling() {
    // ⚠️ **This test exists because reading the table by eye failed.**
    // The walkthrough written on 9 Aug 2026 asserted `⌘⌥E` opened the file
    // explorer. It did not — the kernel binds `Ctrl+Alt+E` and this face's
    // table had `⌘⌥` rows for H, T and R and no E — and Tom found that by
    // pressing the key, which is the worst possible place to find it.
    //
    // The question is asked **per verb, not per stroke**, and that
    // distinction is the whole reason this passes for the arrows.
    // `cursor.documentStart` is bound to `Ctrl+Home`, and there is no
    // `⌘Home` row; a stroke-level check would demand one. But a mac hand
    // does not reach for `⌘Home`, it reaches for `⌘↑`, and that IS bound —
    // so the verb has a ⌘ spelling and the stroke never needs one.
    //
    // Scoped to the non-modal defaults, which is the surface this face's
    // layer can reach, and to bindings that *require* `Ctrl`: a plain
    // arrow needs no mac spelling because it already is one.
    let hints = KeyHintIndex::build(&session_stack());
    let excused: BTreeSet<&str> = CTRL_ONLY.iter().map(|(id, _)| *id).collect();

    let mut stranded: Vec<String> = Vec::new();
    for binding in default_non_modal_keymap().bindings() {
        let Some(command) = binding.command() else {
            continue;
        };
        let requires_ctrl = binding
            .sequence()
            .first()
            .is_some_and(|stroke| stroke.modifiers.required_modifiers().ctrl);
        if !requires_ctrl || excused.contains(command.as_str()) {
            continue;
        }
        if !reachable_without_ctrl(&hints, command.as_str()) {
            stranded.push(command.to_string());
        }
    }

    stranded.sort_unstable();
    stranded.dedup();
    assert!(
        stranded.is_empty(),
        "these verbs answer only to a chord holding Ctrl, on a face whose users \
         reach for ⌘ and ⌥: {stranded:?} — add the row, or put the id on \
         CTRL_ONLY with the reason"
    );
}

#[test]
fn nothing_on_the_ctrl_only_list_has_a_meta_spelling_after_all() {
    // The same staleness guard `nothing_on_the_palette_only_list_is_also_bound`
    // applies to its list: an excuse that outlived its reason is worse than
    // no excuse, because the next verb added inherits it.
    let hints = KeyHintIndex::build(&session_stack());
    for (id, reason) in CTRL_ONLY {
        assert!(
            !reason.is_empty(),
            "{id} is excused from a Ctrl-free chord with no reason given"
        );
        assert!(
            !reachable_without_ctrl(&hints, id),
            "{id} has a Ctrl-free chord after all — take it off CTRL_ONLY"
        );
    }
}

#[test]
fn no_verb_the_kernel_could_reach_is_stranded_by_this_layer() {
    // The general form of the test above it. That one names the two verbs
    // whose displacement someone noticed; this one asks the question of
    // every verb, so the next row added to `MAC_CHORDS` cannot strand one
    // quietly.
    //
    // The claim being checked is the module doc's: this face takes chords
    // *away* from the default keymap, and every verb so displaced keeps a
    // home. Most rows displace nothing that matters — `⌥←` outranks the
    // loose pattern behind `cursor.left`, which still answers to a bare
    // `←` — and the distinction between that and stranding a verb outright
    // is exactly what a stroke-level overlap check cannot see. So the
    // comparison is at the level of *reachability*, which is what
    // `KeyHintIndex` already computes: an empty hint list means there is
    // no key for this command, suppression, shadowing and rank loss all
    // accounted for.
    //
    // A command already unreachable under the defaults alone is skipped
    // rather than asserted about: this layer did not do that, and holding
    // it responsible would fail the moment the kernel gained a
    // palette-only verb.
    //
    // Scoped to the non-modal defaults, which is the surface this face's
    // layer can reach — every row it pushes is a `CHORD`-mode binding.
    let before = KeyHintIndex::build(&default_keymap_stack());
    let after = KeyHintIndex::build(&session_stack());

    for binding in default_non_modal_keymap().bindings() {
        let Some(command) = binding.command() else {
            continue;
        };
        let id = command.as_str();
        if before.hints_for(id).is_empty() {
            continue;
        }
        assert!(
            !after.hints_for(id).is_empty(),
            "{id} answered to a key under the default keymap and answers to none \
             with this face's layer pushed — rehouse it, as the syntax verbs were, \
             or say in the table that it is deliberately gone"
        );
    }
}

#[test]
fn no_host_command_this_face_binds_carries_arguments() {
    // The face half of the kernel's
    // `no_host_command_is_bound_to_a_sequence_that_carries_arguments`.
    // Every verb this face contributes is a host command by construction —
    // the test above asserts the kernel implements none of them — and it
    // reaches `dispatch_host_command`, which takes an id and nothing else.
    //
    // `EditorKeyResult::HostCommand` carries `args` beside the id, and this
    // face drops them. Harmless while no binding declares a count prefix
    // or a capturing stroke; a silent wrong answer the moment one does,
    // since the browser face forwards both.
    //
    // ⚠️ This covers this face's own layer only. A **user keymap** can
    // carry a capturing stroke — `{char}` parses to
    // `StrokePattern::any_char` — and nothing checks that layer. It costs
    // nothing today because no host command anywhere reads its arguments,
    // so the character dropped here is one the browser hands to a host
    // that ignores it too.
    //
    // **When this fails, thread `args` through `run_host_command` rather
    // than relaxing it.**
    //
    // ⚠️ **This note used to say `implements_command` admits the five
    // `workspace.*` ids. It does not** — it is
    // `actions::action_for(id).is_some()`, the keyboard action table, and
    // no `workspace.*` id is in it. Proven on 9 Aug 2026 by binding `⌘⇧]`
    // to `workspace.nextTab`, which made
    // `every_borrowed_kernel_verb_is_still_a_kernel_verb` fail with
    // "the kernel neither implements nor names it".
    //
    // The consequence for *this* test is that a `workspace.*` id falls
    // through the `continue` above and gets asserted about, which is
    // correct and was correct before: those ids reach
    // `Workspace::run_command`, which takes an id and nothing else, so
    // they discard arguments exactly as host commands do.
    for binding in keymap().bindings() {
        let Some(command) = binding.command() else {
            continue;
        };
        if Editor::implements_command(command.as_str()) {
            continue;
        }
        assert!(
            !binding.accepts_count() && !binding.has_capture_stroke(),
            "{command} is a command the kernel does not implement, bound to \
             a sequence that carries arguments, and this face discards them"
        );
    }
}

#[test]
fn every_command_this_face_adds_is_one_the_kernel_does_not_implement() {
    // A face command that shadows a kernel one would be the exact mistake
    // this crate is written to avoid: two implementations of one id, and
    // whichever the dispatch reaches first wins.
    for meta in COMMANDS {
        assert!(
            !Editor::implements_command(meta.id().as_str()),
            "{} is already implemented by the kernel",
            meta.id()
        );
    }
}

#[test]
fn every_borrowed_kernel_verb_is_still_a_kernel_verb() {
    // The ⌘ layer binds only verbs the kernel implements or ids the kernel
    // *names* for someone else to run; a row here for an id the kernel
    // dropped would be a mac chord that consumes the key and does nothing.
    //
    // **Three families, not two.** `host_command_metas` was the only one
    // this test knew about until `⌘⇧]` was bound to `workspace.nextTab` on
    // 9 Aug 2026 and it failed — the `workspace.*` ids are named by
    // `workspace_command_metas` and dispatched by `Workspace::run_command`,
    // which is a third destination and was always a legitimate one for this
    // face to bind. The test was narrower than the code it guarded.
    let ours: BTreeSet<&str> = COMMANDS.iter().map(|meta| meta.id().as_str()).collect();
    let named: BTreeSet<&str> = iridium_editor::commands::builtin::host_command_metas()
        .iter()
        .chain(iridium_editor::commands::builtin::workspace_command_metas())
        .map(|meta| meta.id().as_str())
        .collect();
    for (_, command) in BINDINGS.iter().chain(MAC_CHORDS) {
        if ours.contains(command.as_str()) {
            continue;
        }
        assert!(
            Editor::implements_command(command.as_str()) || named.contains(command.as_str()),
            "{command} is bound here but the kernel neither implements it, nor names \
             it as a host command, nor names it as a workspace command"
        );
    }
}

#[test]
fn no_two_commands_share_an_id() {
    let ids: BTreeSet<&str> = COMMANDS.iter().map(|meta| meta.id().as_str()).collect();
    assert_eq!(ids.len(), COMMAND_COUNT, "a command id is duplicated");
}

#[test]
fn every_command_carries_a_title_and_a_description() {
    for meta in COMMANDS {
        assert!(!meta.title().is_empty(), "{} has no title", meta.id());
        assert!(
            meta.description().is_some_and(|text| !text.is_empty()),
            "{} has no description",
            meta.id()
        );
    }
}

/// Commands this face contributes and deliberately leaves unbound.
///
/// A list rather than a relaxed assertion, so leaving a verb unbound is a
/// decision written down with its reason next to it — and an *accidentally*
/// unbound verb still fails, which is the case this test exists for.
///
/// - `commands.list` opens a reference read once while writing a
///   configuration file and then not again for months. Every chord spent
///   is one the user cannot have, and the palette is exactly the right
///   surface for something wanted by name and rarely.
/// - `config.edit` opens that same configuration file, at the same
///   cadence, and is found the same way. `⌘,` is the mac chord for it and
///   is deliberately **not** taken: this face has no preferences window,
///   and claiming the key that everyone's hand expects one from — to open
///   a text file instead — is a promise it cannot keep.
const PALETTE_ONLY: &[&str] = &["commands.list", "config.edit"];

#[test]
fn every_command_this_face_adds_is_bound_or_deliberately_is_not() {
    let bound: BTreeSet<&str> = BINDINGS
        .iter()
        .chain(MAC_CHORDS)
        .map(|(_, command)| command.as_str())
        .collect();
    for meta in COMMANDS {
        let id = meta.id().as_str();
        assert!(
            bound.contains(id) || PALETTE_ONLY.contains(&id),
            "{id} has no key and is not on the palette-only list, and this \
             face's own verbs otherwise all deserve one"
        );
    }
}

#[test]
fn nothing_on_the_palette_only_list_is_also_bound() {
    // The list is a record of a decision, and a stale record is worse than
    // none: a verb that later gained a chord would keep its excuse and the
    // next unbound one would inherit it.
    let bound: BTreeSet<&str> = BINDINGS
        .iter()
        .chain(MAC_CHORDS)
        .map(|(_, command)| command.as_str())
        .collect();
    for id in PALETTE_ONLY {
        assert!(
            !bound.contains(id),
            "{id} is bound after all — take it off the palette-only list"
        );
        assert!(
            COMMANDS.iter().any(|meta| meta.id().as_str() == *id),
            "{id} is on the palette-only list but this face does not add it"
        );
    }
}

#[test]
fn the_save_ids_are_the_terminal_faces_ids() {
    // One id, one meaning, across every face. The strings are asserted
    // here because the terminal face declares its own constants and
    // nothing else would notice the two crates drifting apart.
    assert_eq!(FILE_SAVE.as_str(), "file.save");
    assert_eq!(FILE_SAVE_FORCE.as_str(), "file.saveForce");
}

/// Every single-stroke binding of the default keymap that `stroke` could
/// fire instead of, named by its command.
fn defaults_overlapped(defaults: &Keymap, stroke: &StrokePattern) -> Vec<String> {
    defaults
        .bindings()
        .iter()
        .filter(|binding| {
            let Some((first, rest)) = binding.sequence().split_first() else {
                return false;
            };
            rest.is_empty() && first.overlaps(stroke)
        })
        .filter_map(|binding| binding.command().map(ToString::to_string))
        .collect()
}

#[test]
fn no_binding_collides_with_the_default_keymap_except_the_mac_chords() {
    // Two layers may legitimately bind one stroke — that is what a layer
    // is for — but the letter rows of this face are adding verbs and mac
    // spellings, not rebinding the kernel's `Ctrl` chords, so an overlap
    // there means a key silently stopped doing what it did.
    let defaults = default_non_modal_keymap();
    for (stroke, command) in BINDINGS {
        let shadowed = defaults_overlapped(&defaults, stroke);
        assert!(
            shadowed.is_empty(),
            "{command} shadows the default bindings for {shadowed:?}"
        );
    }

    // `MAC_CHORDS` is the allowlist, and it overlaps by construction: the
    // default keymap's arrow and delete patterns declare `Alt` and `Meta`
    // `Any`, so the only way to give `⌥←` or `⌘⌫` a verb of its own is a
    // `Required` mac spelling that matches inside the loose pattern and
    // outranks it. A row that overrides *nothing* is the mistake this half
    // catches: it would mean the chord was already bound elsewhere, or that
    // the pattern is not the one the default keymap actually uses.
    for (stroke, command) in MAC_CHORDS {
        assert!(
            !defaults_overlapped(&defaults, stroke).is_empty(),
            "{command} is filed as a deliberate override but overrides nothing"
        );
    }
}

#[test]
fn the_keymap_binds_every_row_of_the_table() {
    let keymap = keymap();
    assert_eq!(keymap.len(), BINDING_COUNT);
    let bound: BTreeSet<String> = keymap
        .bindings()
        .iter()
        .filter_map(|binding| binding.command().map(ToString::to_string))
        .collect();
    for (_, command) in BINDINGS.iter().chain(MAC_CHORDS) {
        assert!(bound.contains(command.as_str()), "{command} is not bound");
    }
}

#[test]
fn the_keymap_validates_against_a_registry_holding_these_commands() {
    // `push_keymap` runs this check for real at start-up; running it here
    // means a mistyped id fails in a unit test rather than at launch.
    let mut editor = Editor::with_defaults();
    for meta in command_metas() {
        editor
            .register_command(meta)
            .expect("the kernel accepted the command");
    }
    editor.push_keymap(keymap()).expect("the keymap validates");
}
