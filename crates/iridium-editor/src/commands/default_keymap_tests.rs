//! Default-keymap tests.
//!
//! The large table below is the fidelity proof for the migration: every entry is
//! a `(key, modifiers)` pair whose expected outcome was read off the existing
//! dispatch in `crate::input::keyboard`, including the cases where that dispatch
//! deliberately ignores a modifier.

// `fn_params_excessive_bools` is allowed for the `mods` helper only: its five
// parameters mirror the five hardware modifier bits of `Modifiers` one for one,
// exactly as that struct's own documented allow does.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::fn_params_excessive_bools
)]

use std::collections::HashSet;

use super::builtin::{
    BUILTIN_COMMAND_COUNT, COMMAND_NO_OP, EDIT_DELETE_TO_LINE_END, EDIT_DELETE_TO_LINE_START,
    EDIT_INSERT_CHARACTER, HISTORY_REDO_BRANCH, HISTORY_TOGGLE_PANEL, HOST_COMMAND_COUNT,
    MULTI_CURSOR_SKIP_LAST_OCCURRENCE, PALETTE_OPEN, TRANSFORM_CAMEL_CASE, TRANSFORM_DEDUPE_LINES,
    TRANSFORM_KEBAB_CASE, TRANSFORM_LOWER_CASE, TRANSFORM_PASCAL_CASE, TRANSFORM_REVERSE_LINES,
    TRANSFORM_SCREAMING_SNAKE_CASE, TRANSFORM_SNAKE_CASE, TRANSFORM_SORT_LINES,
    TRANSFORM_SORT_LINES_REVERSE, TRANSFORM_SWAP_CASE, TRANSFORM_TITLE_CASE, TRANSFORM_TOGGLE_CASE,
    TRANSFORM_TRIM_TRAILING_WHITESPACE, TRANSFORM_UPPER_CASE, builtin_registry, default_registry,
    host_command_metas,
};
use super::{
    DEFAULT_KEYMAP_BINDING_COUNT, KeyBinding, KeyPress, Keymap, KeymapError, KeymapResolver,
    KeymapStack, ModifierPattern, ModifierState, Resolution, StrokePattern, default_keymap_stack,
    default_non_modal_keymap,
};
use crate::input::{KeyCode, Modifiers};

const fn mods(shift: bool, ctrl: bool, alt: bool, meta: bool, alt_graph: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        meta,
        alt_graph,
    }
}

const NONE: Modifiers = mods(false, false, false, false, false);
const SHIFT: Modifiers = mods(true, false, false, false, false);
const CTRL: Modifiers = mods(false, true, false, false, false);
const CTRL_SHIFT: Modifiers = mods(true, true, false, false, false);
const ALT: Modifiers = mods(false, false, true, false, false);
const SHIFT_ALT: Modifiers = mods(true, false, true, false, false);
const CTRL_ALT: Modifiers = mods(false, true, true, false, false);
const CTRL_ALT_SHIFT: Modifiers = mods(true, true, true, false, false);
const CTRL_ALT_GRAPH: Modifiers = mods(false, true, true, false, true);
const META: Modifiers = mods(false, false, false, true, false);
const CTRL_META: Modifiers = mods(false, true, false, true, false);

/// A `Ctrl`+letter chord requiring `Shift` to be absent, as the default keymap
/// spells one.
fn ctrl_no_shift(key: KeyCode) -> StrokePattern {
    use ModifierState::{Any, Forbidden, Required};
    StrokePattern::new(
        key,
        ModifierPattern::new(Forbidden, Required, Forbidden, Forbidden, Any),
    )
}

/// The leader these tests hang their chord on.
///
/// Any key the default keymap leaves free will do, and that is the point: the
/// behaviour under test belongs to the resolver, not to this key. It was `Ctrl+K`
/// until `Ctrl+K` became the command palette — which is exactly the coupling
/// naming it once, here, is meant to stop.
const CHORD_LEADER: KeyCode = KeyCode::Char('b');

/// The default stack plus a host layer binding a `Ctrl+B Ctrl+D` chord.
///
/// The default keymap binds no chord at all — a bare `Ctrl+K` forecloses every
/// chord beneath it — so the multi-stroke machinery would otherwise have no
/// subject to be tested against, and dropping one binding would silently drop the
/// coverage of a whole feature.
///
/// Adding one in a layer is *also* the escape hatch the `default_keymap` module
/// documentation promises to anyone who wants a chord, so every test built on
/// this doubles as proof that the promise holds.
fn stack_with_host_chord() -> KeymapStack {
    let mut stack = default_keymap_stack();
    let mut layer = Keymap::new("host-chord");
    layer.push(KeyBinding::new(
        ctrl_no_shift(CHORD_LEADER),
        &[ctrl_no_shift(KeyCode::Char('d'))],
        MULTI_CURSOR_SKIP_LAST_OCCURRENCE,
    ));
    stack.push(layer);
    stack
}

/// Resolves one keypress against the default keymap with a fresh resolver.
fn resolve(stack: &KeymapStack, key: KeyCode, modifiers: Modifiers) -> Resolution {
    let mut resolver = KeymapResolver::new();
    resolver.resolve(stack, KeyPress::new(key, modifiers))
}

/// Asserts the keypress resolves to `expected`, or to nothing when `None`.
fn expect(stack: &KeymapStack, key: KeyCode, modifiers: Modifiers, expected: Option<&str>) {
    let outcome = resolve(stack, key, modifiers);
    match (expected, &outcome) {
        (Some(id), Resolution::Matched(matched)) => {
            assert_eq!(
                matched.command().as_str(),
                id,
                "wrong command for {key:?} + {modifiers:?}"
            );
        },
        (None, Resolution::NoMatch) => {},
        _ => panic!("{key:?} + {modifiers:?}: expected {expected:?}, got {outcome:?}"),
    }
}

#[test]
fn binding_count_matches_the_documented_total() {
    assert_eq!(
        default_non_modal_keymap().len(),
        DEFAULT_KEYMAP_BINDING_COUNT
    );
}

#[test]
fn the_default_keymap_validates_against_the_default_registry() {
    let registry = default_registry().unwrap();
    default_non_modal_keymap()
        .validate(&registry)
        .expect("the default keymap must reference only registered commands and shadow nothing");
}

#[test]
fn the_default_keymap_does_not_validate_against_the_builtins_alone() {
    // Not a quirk to work around — the statement that the host commands are real
    // registered commands rather than strings the keymap invented. A registry
    // without the host table genuinely does not know them, and says so.
    let error = default_non_modal_keymap()
        .validate(&builtin_registry().unwrap())
        .expect_err("the built-in table alone cannot know the host commands");
    let host_ids: HashSet<&str> = host_command_metas()
        .iter()
        .map(|meta| meta.id().as_str())
        .collect();
    assert!(
        matches!(error, KeymapError::UnknownCommand { ref id, .. } if host_ids.contains(id.as_str())),
        "expected a host command to be the unknown one, got {error:?}"
    );

    // Every host command, not merely the first one validation happens to reach:
    // a binding to an id no registry knows is the failure this guards against,
    // and it must be caught for all of them.
    let registry = builtin_registry().unwrap();
    for id in &host_ids {
        assert!(
            registry.get(id).is_none(),
            "`{id}` is a host command and must not be in the built-in table"
        );
    }
    let with_hosts = default_registry().unwrap();
    for id in &host_ids {
        assert!(
            with_hosts.get(id).is_some(),
            "`{id}` is bound by the default keymap but no registry names it"
        );
    }
}

#[test]
fn every_default_binding_is_allocation_free() {
    for binding in default_non_modal_keymap().bindings() {
        let id = binding
            .command()
            .expect("no suppressions in the default keymap");
        assert!(id.is_static(), "`{id}` would allocate on every match");
        assert!(binding.mode().is_none(), "the default keymap is mode-free");
    }
}

#[test]
fn every_registered_command_is_bound_except_the_typing_fall_through() {
    let bound: HashSet<String> = default_non_modal_keymap()
        .bindings()
        .iter()
        .filter_map(|binding| binding.command())
        .map(|id| id.as_str().to_owned())
        .collect();

    let registry = default_registry().unwrap();
    let unbound: Vec<&str> = registry
        .commands()
        .map(|meta| meta.id().as_str())
        .filter(|id| !bound.contains(*id))
        .collect();

    // Twenty-one commands are intentionally unbound by the *non-modal* default,
    // and a twenty-second entry here would mean a feature silently lost:
    //
    // - `edit.insertCharacter` is the typing fall-through; no key sequence can
    //   stand for "whatever the user typed".
    // - `command.noOp` exists for modal keymaps that must swallow a key rather
    //   than unbind it, and swallowing keys is precisely what a non-modal keymap
    //   must never do.
    // - `multiCursor.skipLastOccurrence` held `Ctrl+K Ctrl+D` until `Ctrl+K`
    //   became the command palette. A bare binding forecloses every chord sharing
    //   its prefix, so the two cannot coexist. This one is *palette-only* rather
    //   than lost: still registered, still implemented, still runnable by id, and
    //   a host may bind it in its own layer.
    // - `edit.deleteToLineStart` / `edit.deleteToLineEnd` are the macOS
    //   `Cmd+Backspace` / `Cmd+Delete` verbs. The default keymap is
    //   platform-neutral and already spends `Ctrl+Backspace` / `Ctrl+Delete` on
    //   word-wise delete, so there is no neutral chord left to give them; the web
    //   face reaches them by id from its own `Cmd` handling, and the palette
    //   reaches them everywhere. Binding them is a keymap decision, not a
    //   prerequisite for the verbs existing.
    // - `history.redoBranch` takes a *count* — "enter the second branch" — and a
    //   bare chord cannot carry one. Its siblings `history.nextBranch` and
    //   `history.previousBranch` are bound (`Ctrl+Alt+Z` / `Ctrl+Alt+Y`) because
    //   they are the browsing verbs; this one is what a host keymap binds to a
    //   counted sequence, and what the panel calls by id when a branch is
    //   clicked.
    // - The fifteen `transform.*` verbs are palette-only *by design*, which is
    //   the approved plan: fifteen new chords that must collide with nothing
    //   would be a poor trade for verbs most people run by name, and the
    //   palette is precisely what makes binding them optional. Which three or
    //   four earn real keys is a question about daily use, not a technical one,
    //   and it is Tom's to answer; adding a binding later is one line here and
    //   one line in the keymap.
    //
    // This list is *ordered* and matches `registry.commands()` iteration order,
    // so a new entry goes where its id is registered, not at the end.
    assert_eq!(
        unbound,
        vec![
            EDIT_INSERT_CHARACTER.as_str(),
            EDIT_DELETE_TO_LINE_START.as_str(),
            EDIT_DELETE_TO_LINE_END.as_str(),
            TRANSFORM_UPPER_CASE.as_str(),
            TRANSFORM_LOWER_CASE.as_str(),
            TRANSFORM_TITLE_CASE.as_str(),
            TRANSFORM_TOGGLE_CASE.as_str(),
            TRANSFORM_SWAP_CASE.as_str(),
            TRANSFORM_CAMEL_CASE.as_str(),
            TRANSFORM_PASCAL_CASE.as_str(),
            TRANSFORM_SNAKE_CASE.as_str(),
            TRANSFORM_SCREAMING_SNAKE_CASE.as_str(),
            TRANSFORM_KEBAB_CASE.as_str(),
            TRANSFORM_SORT_LINES.as_str(),
            TRANSFORM_SORT_LINES_REVERSE.as_str(),
            TRANSFORM_REVERSE_LINES.as_str(),
            TRANSFORM_DEDUPE_LINES.as_str(),
            TRANSFORM_TRIM_TRAILING_WHITESPACE.as_str(),
            HISTORY_REDO_BRANCH.as_str(),
            MULTI_CURSOR_SKIP_LAST_OCCURRENCE.as_str(),
            COMMAND_NO_OP.as_str()
        ]
    );
    // Every host command is bound too — an id the kernel names but no face can
    // discover by key is a feature nobody finds.
    assert_eq!(bound.len(), BUILTIN_COMMAND_COUNT + HOST_COMMAND_COUNT - 21);
    assert!(bound.contains(PALETTE_OPEN.as_str()));
}

#[test]
fn horizontal_navigation_matches_the_current_dispatch() {
    let stack = default_keymap_stack();
    let cases: &[(KeyCode, Modifiers, Option<&str>)] = &[
        (KeyCode::Left, NONE, Some("cursor.charLeft")),
        (KeyCode::Left, SHIFT, Some("cursor.charLeftSelect")),
        (KeyCode::Left, CTRL, Some("cursor.wordLeft")),
        (KeyCode::Left, CTRL_SHIFT, Some("cursor.wordLeftSelect")),
        // Dispatch reads only Ctrl and Shift for horizontal motion, so Alt and
        // Meta must not change the outcome.
        (KeyCode::Left, ALT, Some("cursor.charLeft")),
        (KeyCode::Left, META, Some("cursor.charLeft")),
        (KeyCode::Left, CTRL_META, Some("cursor.wordLeft")),
        (KeyCode::Right, NONE, Some("cursor.charRight")),
        (KeyCode::Right, SHIFT, Some("cursor.charRightSelect")),
        (KeyCode::Right, CTRL, Some("cursor.wordRight")),
        (KeyCode::Right, CTRL_SHIFT, Some("cursor.wordRightSelect")),
        (KeyCode::Right, ALT, Some("cursor.charRight")),
        (KeyCode::Home, NONE, Some("cursor.lineStart")),
        (KeyCode::Home, SHIFT, Some("cursor.lineStartSelect")),
        (KeyCode::Home, CTRL, Some("cursor.documentStart")),
        (
            KeyCode::Home,
            CTRL_SHIFT,
            Some("cursor.documentStartSelect"),
        ),
        (KeyCode::Home, ALT, Some("cursor.lineStart")),
        (KeyCode::End, NONE, Some("cursor.lineEnd")),
        (KeyCode::End, SHIFT, Some("cursor.lineEndSelect")),
        (KeyCode::End, CTRL, Some("cursor.documentEnd")),
        (KeyCode::End, CTRL_SHIFT, Some("cursor.documentEndSelect")),
        (KeyCode::End, META, Some("cursor.lineEnd")),
    ];
    for &(key, modifiers, expected) in cases {
        expect(&stack, key, modifiers, expected);
    }
}

#[test]
fn vertical_navigation_and_its_chords_match_the_current_dispatch() {
    let stack = default_keymap_stack();
    let cases: &[(KeyCode, Modifiers, Option<&str>)] = &[
        (KeyCode::Up, NONE, Some("cursor.lineUp")),
        (KeyCode::Up, SHIFT, Some("cursor.lineUpSelect")),
        // The plain vertical arm reads only Shift, so Ctrl and Meta fall through
        // to caret movement exactly as they do today.
        (KeyCode::Up, CTRL, Some("cursor.lineUp")),
        (KeyCode::Up, META, Some("cursor.lineUp")),
        (KeyCode::Up, ALT, Some("lines.moveUp")),
        (KeyCode::Up, SHIFT_ALT, Some("lines.duplicateUp")),
        (KeyCode::Up, CTRL_ALT, Some("multiCursor.addCursorAbove")),
        // Ctrl+Alt+Shift is neither a line op (Ctrl) nor an add-cursor (Shift),
        // so it falls through to the selecting vertical motion.
        (KeyCode::Up, CTRL_ALT_SHIFT, Some("cursor.lineUpSelect")),
        (KeyCode::Down, NONE, Some("cursor.lineDown")),
        (KeyCode::Down, SHIFT, Some("cursor.lineDownSelect")),
        (KeyCode::Down, CTRL, Some("cursor.lineDown")),
        (KeyCode::Down, ALT, Some("lines.moveDown")),
        (KeyCode::Down, SHIFT_ALT, Some("lines.duplicateDown")),
        (KeyCode::Down, CTRL_ALT, Some("multiCursor.addCursorBelow")),
        (KeyCode::Down, CTRL_ALT_SHIFT, Some("cursor.lineDownSelect")),
        // Alt+Meta is not a line op (Meta excluded), so the caret moves.
        (
            KeyCode::Up,
            mods(false, false, true, true, false),
            Some("cursor.lineUp"),
        ),
    ];
    for &(key, modifiers, expected) in cases {
        expect(&stack, key, modifiers, expected);
    }
}

#[test]
fn alt_graph_arrows_never_add_cursors() {
    let stack = default_keymap_stack();
    // AltGr is reported as Ctrl+Alt plus the AltGraph bit. The add-cursor chord
    // must decline it and the caret must simply move, as it does today.
    expect(&stack, KeyCode::Up, CTRL_ALT_GRAPH, Some("cursor.lineUp"));
    expect(
        &stack,
        KeyCode::Down,
        CTRL_ALT_GRAPH,
        Some("cursor.lineDown"),
    );
    expect(
        &stack,
        KeyCode::Up,
        mods(true, true, true, false, true),
        Some("cursor.lineUpSelect"),
    );
}

#[test]
fn control_letter_chords_match_the_current_dispatch() {
    let stack = default_keymap_stack();
    let cases: &[(KeyCode, Modifiers, Option<&str>)] = &[
        (KeyCode::Char('c'), CTRL, Some("clipboard.copy")),
        (KeyCode::Char('x'), CTRL, Some("clipboard.cut")),
        (KeyCode::Char('v'), CTRL, Some("clipboard.paste")),
        (KeyCode::Char('z'), CTRL, Some("history.undo")),
        (KeyCode::Char('y'), CTRL, Some("history.redo")),
        (KeyCode::Char('a'), CTRL, Some("selection.selectAll")),
        (
            KeyCode::Char('d'),
            CTRL,
            Some("multiCursor.addSelectionToNextMatch"),
        ),
        (KeyCode::Char('f'), CTRL, Some("search.open")),
        (KeyCode::Char('/'), CTRL, Some("comment.toggleLine")),
        (
            KeyCode::Char('u'),
            CTRL,
            Some("multiCursor.removeLastCursor"),
        ),
        (KeyCode::Char('j'), CTRL, Some("lines.join")),
        // The shifted forms of the guarded arms.
        (
            KeyCode::Char('L'),
            CTRL_SHIFT,
            Some("multiCursor.selectAllOccurrences"),
        ),
        (KeyCode::Char('K'), CTRL_SHIFT, Some("lines.delete")),
        // Guards the other way: these arms require Shift to be absent.
        (KeyCode::Char('U'), CTRL_SHIFT, None),
        (KeyCode::Char('J'), CTRL_SHIFT, None),
        // Ctrl+Shift+L/K only; the unshifted L is not a binding of its own.
        (KeyCode::Char('l'), CTRL, None),
        // The command palette: `Ctrl+K` with Shift forbidden so it cannot swallow
        // Ctrl+Shift+K above, and `Ctrl+P` either way.
        (KeyCode::Char('k'), CTRL, Some("palette.open")),
        (KeyCode::Char('p'), CTRL, Some("palette.open")),
        (KeyCode::Char('P'), CTRL_SHIFT, Some("palette.open")),
        // Unbound Ctrl chords pass through, never inserting text.
        (KeyCode::Char('b'), CTRL, None),
        // Ctrl+Alt and Ctrl+Meta compounds are reserved and must pass through.
        (KeyCode::Char('c'), CTRL_ALT, None),
        (KeyCode::Char('c'), CTRL_META, None),
    ];
    for &(key, modifiers, expected) in cases {
        expect(&stack, key, modifiers, expected);
    }
}

#[test]
fn shifted_control_chords_are_shift_agnostic_exactly_where_dispatch_is() {
    let stack = default_keymap_stack();
    // `handle_ctrl_char` lower-cases the character and only guards Shift for
    // l/u/k/j, so every other Ctrl chord fires with Shift held too. Hosts report
    // the shifted letter, so both cases are checked.
    for (lower, upper, id) in [
        ('c', 'C', "clipboard.copy"),
        ('x', 'X', "clipboard.cut"),
        ('v', 'V', "clipboard.paste"),
        ('a', 'A', "selection.selectAll"),
        ('d', 'D', "multiCursor.addSelectionToNextMatch"),
        ('f', 'F', "search.open"),
        ('y', 'Y', "history.redo"),
    ] {
        expect(&stack, KeyCode::Char(lower), CTRL_SHIFT, Some(id));
        expect(&stack, KeyCode::Char(upper), CTRL_SHIFT, Some(id));
    }
}

#[test]
fn ctrl_shift_z_redoes_and_plain_ctrl_z_undoes() {
    let stack = default_keymap_stack();
    // The registry migration transcribed the pre-registry dispatch, which
    // matched 'z' regardless of Shift, so Ctrl+Shift+Z used to undo. Two
    // separate bindings now, because one pattern cannot mean two commands.
    expect(&stack, KeyCode::Char('z'), CTRL, Some("history.undo"));
    expect(&stack, KeyCode::Char('z'), CTRL_SHIFT, Some("history.redo"));
    // Hosts forward the base letter for modifier chords, in either case.
    expect(&stack, KeyCode::Char('Z'), CTRL_SHIFT, Some("history.redo"));
    expect(&stack, KeyCode::Char('y'), CTRL, Some("history.redo"));
    expect(&stack, KeyCode::Char('y'), CTRL_SHIFT, Some("history.redo"));
}

#[test]
fn block_comment_requires_shift_alt_without_ctrl_or_meta() {
    let stack = default_keymap_stack();
    expect(
        &stack,
        KeyCode::Char('a'),
        SHIFT_ALT,
        Some("comment.toggleBlock"),
    );
    // Hosts forward the base letter for modifier chords, in either case.
    expect(
        &stack,
        KeyCode::Char('A'),
        SHIFT_ALT,
        Some("comment.toggleBlock"),
    );
    // Alt alone, and Ctrl+Alt (which is how AltGr composes), must pass through.
    expect(&stack, KeyCode::Char('a'), ALT, None);
    expect(&stack, KeyCode::Char('a'), CTRL_ALT, None);
    expect(
        &stack,
        KeyCode::Char('a'),
        mods(true, false, true, true, false),
        None,
    );
}

#[test]
fn editing_keys_match_the_current_dispatch() {
    let stack = default_keymap_stack();
    let cases: &[(KeyCode, Modifiers, Option<&str>)] = &[
        // Enter and Escape consult no modifier at all today.
        (KeyCode::Enter, NONE, Some("edit.insertNewline")),
        (KeyCode::Enter, SHIFT, Some("edit.insertNewline")),
        (KeyCode::Enter, CTRL, Some("edit.insertNewline")),
        (
            KeyCode::Enter,
            mods(true, true, true, true, true),
            Some("edit.insertNewline"),
        ),
        (KeyCode::Escape, NONE, Some("selection.collapseToPrimary")),
        (
            KeyCode::Escape,
            CTRL_SHIFT,
            Some("selection.collapseToPrimary"),
        ),
        // Tab reads only Shift.
        (KeyCode::Tab, NONE, Some("edit.tab")),
        (KeyCode::Tab, SHIFT, Some("edit.outdent")),
        (KeyCode::Tab, CTRL, Some("edit.tab")),
        (KeyCode::Tab, CTRL_SHIFT, Some("edit.outdent")),
        // Backspace and Delete read only Ctrl.
        (KeyCode::Backspace, NONE, Some("edit.deleteBackward")),
        (KeyCode::Backspace, SHIFT, Some("edit.deleteBackward")),
        (KeyCode::Backspace, ALT, Some("edit.deleteBackward")),
        (KeyCode::Backspace, META, Some("edit.deleteBackward")),
        (KeyCode::Backspace, CTRL, Some("edit.deleteWordBackward")),
        (
            KeyCode::Backspace,
            CTRL_SHIFT,
            Some("edit.deleteWordBackward"),
        ),
        (KeyCode::Delete, NONE, Some("edit.deleteForward")),
        (KeyCode::Delete, SHIFT, Some("edit.deleteForward")),
        (KeyCode::Delete, CTRL, Some("edit.deleteWordForward")),
        (KeyCode::Delete, CTRL_ALT, Some("edit.deleteWordForward")),
    ];
    for &(key, modifiers, expected) in cases {
        expect(&stack, key, modifiers, expected);
    }
}

#[test]
fn search_keys_match_the_current_dispatch() {
    let stack = default_keymap_stack();
    expect(&stack, KeyCode::F3, NONE, Some("search.nextMatch"));
    expect(&stack, KeyCode::F3, SHIFT, Some("search.previousMatch"));
    // F3's arms read only Shift.
    expect(&stack, KeyCode::F3, CTRL, Some("search.nextMatch"));
    expect(
        &stack,
        KeyCode::F3,
        CTRL_SHIFT,
        Some("search.previousMatch"),
    );
}

#[test]
fn keys_the_dispatch_ignores_stay_unbound() {
    let stack = default_keymap_stack();
    let cases: &[(KeyCode, Modifiers)] = &[
        // Page keys are the viewport's business, not the keyboard handler's.
        (KeyCode::PageUp, NONE),
        (KeyCode::PageDown, NONE),
        (KeyCode::PageUp, CTRL),
        // Function keys other than F3.
        (KeyCode::F1, NONE),
        (KeyCode::F2, NONE),
        (KeyCode::F4, NONE),
        (KeyCode::F12, NONE),
        // Bare modifier presses.
        (KeyCode::Shift, NONE),
        (KeyCode::Control, NONE),
        (KeyCode::Alt, NONE),
        (KeyCode::Meta, NONE),
    ];
    for &(key, modifiers) in cases {
        expect(&stack, key, modifiers, None);
    }
}

#[test]
fn plain_characters_fall_through_to_the_typing_path() {
    let stack = default_keymap_stack();
    // Nothing in the keymap may claim a bare character: those are text, and the
    // fall-through inserts them from the original event.
    for c in ['a', 'A', 'z', '/', ' ', '1', 'é', 'Ж'] {
        expect(&stack, KeyCode::Char(c), NONE, None);
        expect(&stack, KeyCode::Char(c), SHIFT, None);
    }
    // Alt- and Meta-modified characters are the host's, as they are today.
    expect(&stack, KeyCode::Char('q'), ALT, None);
    expect(&stack, KeyCode::Char('q'), META, None);
}

#[test]
fn three_chords_open_the_command_palette() {
    // `Ctrl+K` is the key the skip-occurrence chord was given up for, so what it
    // does now is worth pinning. `Ctrl+P` and `Ctrl+Shift+P` come from the same
    // binding, which is why `Shift` is `Any` there and `Forbidden` on `Ctrl+K`.
    let stack = default_keymap_stack();
    let palette_open = PALETTE_OPEN;
    let palette = Some(palette_open.as_str());

    expect(&stack, KeyCode::Char('k'), CTRL, palette);
    expect(&stack, KeyCode::Char('p'), CTRL, palette);
    expect(&stack, KeyCode::Char('P'), CTRL_SHIFT, palette);

    // Ctrl+Shift+K still deletes lines: the palette's `Ctrl+K` forbids `Shift`,
    // so it does not swallow the shifted chord.
    expect(&stack, KeyCode::Char('K'), CTRL_SHIFT, Some("lines.delete"));
}

/// The undo tree's three keys are one family on `Ctrl+Alt`, and none of them
/// collides with the plain undo and redo they sit beside.
#[test]
fn the_undo_tree_keys_are_one_chord_family() {
    let stack = default_keymap_stack();

    expect(
        &stack,
        KeyCode::Char('h'),
        CTRL_ALT,
        Some(HISTORY_TOGGLE_PANEL.as_str()),
    );
    expect(
        &stack,
        KeyCode::Char('z'),
        CTRL_ALT,
        Some("history.previousBranch"),
    );
    expect(
        &stack,
        KeyCode::Char('y'),
        CTRL_ALT,
        Some("history.nextBranch"),
    );

    // Adding `Alt` is what distinguishes them: without it the same letters are
    // undo and redo, and those must not have moved.
    expect(&stack, KeyCode::Char('z'), CTRL, Some("history.undo"));
    expect(&stack, KeyCode::Char('y'), CTRL, Some("history.redo"));

    // And `Ctrl+H` alone is not the panel — it is nothing at all, left free
    // rather than quietly claimed.
    expect(&stack, KeyCode::Char('h'), CTRL, None);
}

#[test]
fn a_host_layer_can_bind_a_chord_of_its_own() {
    // The escape hatch the module documentation promises: a host layer may bind a
    // chord on any leader the default leaves free, and it resolves.
    let stack = stack_with_host_chord();
    let mut resolver = KeymapResolver::new();

    assert_eq!(
        resolver.resolve(&stack, KeyPress::new(CHORD_LEADER, CTRL)),
        Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&stack, KeyPress::new(KeyCode::Char('d'), CTRL)),
        Resolution::matched(super::builtin::MULTI_CURSOR_SKIP_LAST_OCCURRENCE)
    );
}

#[test]
fn a_dead_ended_skip_chord_replays_the_character_instead_of_eating_it() {
    // REGRESSION: a dead-ended sequence used to be consumed, so pressing a chord
    // leader and then typing silently discarded one character of the document.
    // The leader is still consumed, but the character that kills the chord is
    // replayed and reaches the typing fall-through.
    //
    // Exercised through a host-bound chord because the default keymap no longer
    // has one; the defect lives in the resolver, not in any particular binding,
    // so any live chord proves it.
    let stack = stack_with_host_chord();
    let mut resolver = KeymapResolver::new();
    assert_eq!(
        resolver.resolve(&stack, KeyPress::new(CHORD_LEADER, CTRL)),
        Resolution::Pending
    );
    let outcome = resolver.resolve(&stack, KeyPress::new(KeyCode::Char('q'), NONE));
    assert_eq!(outcome, Resolution::NoMatch);
    assert!(
        !outcome.consumed(),
        "the character that killed the chord must still be typed"
    );

    // The same replay makes a mistyped second stroke run its own binding rather
    // than disappearing: the leader then `Ctrl+Shift+D` adds a selection.
    assert_eq!(
        resolver.resolve(&stack, KeyPress::new(CHORD_LEADER, CTRL)),
        Resolution::Pending
    );
    assert_eq!(
        resolver.resolve(&stack, KeyPress::new(KeyCode::Char('d'), CTRL_SHIFT)),
        Resolution::matched(super::builtin::MULTI_CURSOR_ADD_SELECTION_TO_NEXT_MATCH)
    );
}
