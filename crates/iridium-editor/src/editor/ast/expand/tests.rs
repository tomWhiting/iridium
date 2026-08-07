//! Tests for structural selection and the stack that makes shrinking exact.
//!
//! Driven through the whole editor rather than the stack alone. The claims worth
//! holding — "shrink gives back exactly what expand took", "an edit invalidates
//! it", "expanding never fills the undo history" — are claims about the editor's
//! behaviour, and a test against the stack in isolation would prove them of a
//! component nobody uses.

use iridium_lang::Language;

use crate::commands::builtin::{
    AST_CURSOR_NODE_END, AST_CURSOR_NODE_START, AST_CURSOR_ON_EVERY_CHILD,
    AST_CURSOR_ON_EVERY_SIBLING, AST_EXPAND_SELECTION, AST_EXTEND_NEXT_SIBLING,
    AST_EXTEND_PREVIOUS_SIBLING, AST_SELECT_FIRST_CHILD, AST_SELECT_LAST_CHILD,
    AST_SELECT_NEXT_SIBLING, AST_SELECT_NODE, AST_SELECT_PREVIOUS_SIBLING, AST_SHRINK_SELECTION,
};
use crate::commands::{CommandArgs, CommandId};
use crate::document::{Position, Selection};
use crate::editor::{Editor, EditorConfig};
use crate::history::Command;
use crate::input::keyboard::AstRequest;
use crate::input::{KeyCode, KeyEvent, Modifiers};

const JSON: &str = "{\"items\": [{\"name\": \"first\"}, {\"name\": \"second\"}]}";
const RUST: &str = "fn main() {\n    let alpha = 1;\n    let beta = 2;\n    let gamma = 3;\n}\n";

/// An editor holding `source`, parsed as `language`, caret at the origin.
fn editor(language: Language, source: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(source);
    editor.set_language(language);
    editor
}

/// The same, with undo grouping disabled so every push gets its own node.
///
/// Only tests that read the *shape* of the history need this. The default 500ms
/// window swallows everything a unit test does, so a test asking "how many undo
/// steps did that make" gets the answer "one" no matter what.
fn editor_grouping_off(language: Language, source: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig {
        undo_group_timeout_ms: 0,
        ..EditorConfig::default()
    });
    editor.set_content(source);
    editor.set_language(language);
    editor
}

/// Places a single caret just before the first occurrence of `needle`.
fn caret_before(editor: &mut Editor, source: &str, needle: &str) {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("{needle:?} does not occur in the fixture"));
    let position = editor
        .state()
        .document
        .offset_to_position(offset)
        .expect("the offset is inside the document");
    editor.set_cursor(position);
}

/// The text each cursor currently covers, primary first.
fn selected(editor: &Editor, source: &str) -> Vec<String> {
    editor
        .state()
        .cursor
        .all_selections()
        .map(|selection| {
            let document = &editor.state().document;
            let start = document
                .position_to_offset(selection.start())
                .expect("a live selection resolves");
            let end = document
                .position_to_offset(selection.end())
                .expect("a live selection resolves");
            source[start..end].to_string()
        })
        .collect()
}

fn expand(editor: &mut Editor) {
    editor
        .run_command(AST_EXPAND_SELECTION.as_str(), CommandArgs::default())
        .expect("the kernel implements expansion");
}

fn shrink(editor: &mut Editor) {
    editor
        .run_command(AST_SHRINK_SELECTION.as_str(), CommandArgs::default())
        .expect("the kernel implements shrinking");
}

#[test]
fn expanding_from_a_caret_selects_the_value_under_it() {
    let mut editor = editor(Language::Json, JSON);
    caret_before(&mut editor, JSON, "first");

    expand(&mut editor);

    assert_eq!(selected(&editor, JSON), vec!["first"]);
}

#[test]
fn expanding_again_walks_out_one_enclosing_level_at_a_time() {
    let mut editor = editor(Language::Json, JSON);
    caret_before(&mut editor, JSON, "first");

    let mut seen = Vec::new();
    for _ in 0..4 {
        expand(&mut editor);
        seen.push(selected(&editor, JSON).join(""));
    }

    assert_eq!(
        seen,
        vec![
            "first",
            "\"first\"",
            "\"name\": \"first\"",
            "{\"name\": \"first\"}",
        ]
    );
}

#[test]
fn shrinking_restores_the_exact_selection_expansion_started_from() {
    let mut editor = editor(Language::Json, JSON);
    caret_before(&mut editor, JSON, "first");
    let before = editor.state().cursor.clone();

    expand(&mut editor);
    expand(&mut editor);
    expand(&mut editor);
    shrink(&mut editor);
    shrink(&mut editor);
    shrink(&mut editor);

    assert_eq!(
        editor.state().cursor,
        before,
        "shrinking three times did not retrace the three expansions"
    );
}

#[test]
fn shrinking_restores_every_cursor_that_merged_on_the_way_out() {
    // The case a per-cursor stack cannot survive, and the reason the stack holds
    // whole cursor states: three carets inside one statement expand onto the
    // same node, merge into one, and must come back as three.
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "alpha");
    let mut state = editor.state().cursor.clone();
    for needle in ["beta", "gamma"] {
        let offset = RUST.find(needle).expect("the fixture has it");
        let position = editor
            .state()
            .document
            .offset_to_position(offset)
            .expect("inside the document");
        state.add_cursor(Selection::collapsed(position));
    }
    editor.state_mut().cursor = state.clone();
    assert_eq!(editor.state().cursor.cursor_count(), 3);

    // Expand until they collapse into a single selection.
    let mut expansions = 0;
    while editor.state().cursor.cursor_count() > 1 {
        expand(&mut editor);
        expansions += 1;
        assert!(expansions < 16, "the cursors never merged");
    }

    for _ in 0..expansions {
        shrink(&mut editor);
    }

    assert_eq!(
        editor.state().cursor,
        state,
        "shrinking gave back {} cursor(s) after three merged into one",
        editor.state().cursor.cursor_count()
    );
}

#[test]
fn an_edit_discards_the_expansion_stack() {
    // The frames describe ranges in a document that no longer exists, and
    // restoring one would put the selection somewhere the person never was.
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "alpha");
    expand(&mut editor);
    expand(&mut editor);
    assert!(editor.state().syntax.expansion_depth() > 0);

    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: "// a comment\n".to_string(),
    });

    assert_eq!(
        editor.state().syntax.expansion_depth(),
        0,
        "the stack survived an edit, so a shrink would restore a range measured \
         against a document that no longer exists"
    );
}

#[test]
fn moving_the_cursors_by_another_route_discards_the_stack() {
    // No other path has to know this stack exists: they invalidate it simply by
    // leaving the cursors somewhere expansion did not put them.
    let mut editor = editor(Language::Rust, RUST);
    caret_before(&mut editor, RUST, "alpha");
    expand(&mut editor);
    expand(&mut editor);
    let expanded = editor.state().cursor.clone();

    caret_before(&mut editor, RUST, "gamma");
    expand(&mut editor);

    assert_ne!(
        editor.state().cursor,
        expanded,
        "an expansion after a cursor jump reused frames from before it"
    );
    assert_eq!(
        editor.state().syntax.expansion_depth(),
        1,
        "the stale frames were kept and only added to"
    );
}

#[test]
fn shrinking_with_no_stack_walks_down_the_tree() {
    let mut editor = editor(Language::Json, JSON);
    let document_end = editor
        .state()
        .document
        .offset_to_position(JSON.len())
        .expect("the end of the document");
    editor.state_mut().cursor =
        crate::document::CursorState::new(Selection::new(Position::new(0, 0), document_end));

    shrink(&mut editor);

    assert_ne!(
        selected(&editor, JSON),
        vec![JSON.to_string()],
        "a hand-made selection must still narrow, with no expansion to retrace"
    );
}

#[test]
fn expanding_at_the_outermost_node_changes_nothing() {
    let mut editor = editor(Language::Json, "[1]");
    caret_before(&mut editor, "[1]", "1");

    // Walk all the way out, then keep pressing.
    for _ in 0..8 {
        expand(&mut editor);
    }
    let settled = editor.state().cursor.clone();
    let depth = editor.state().syntax.expansion_depth();

    expand(&mut editor);

    assert_eq!(editor.state().cursor, settled);
    assert_eq!(
        editor.state().syntax.expansion_depth(),
        depth,
        "a press that changed nothing still recorded a frame, so the next \
         shrink would appear to do nothing"
    );
}

#[test]
fn expansion_never_enters_the_undo_history() {
    // The approved plan expected the opposite — "every selection change goes out
    // as a `Command::SetSelection`, so AST navigation is undoable with no new
    // history machinery". It is not: `apply_command_internal` pushes to the undo
    // tree only when a command modifies *content*, so a selection-only command
    // is applied and never recorded.
    //
    // That is the right behaviour and this test pins it. Shrink is the inverse
    // of expand, not `Ctrl+Z`; every editor that has this verb works that way.
    // If expansions did become undo steps, holding the key would bury the last
    // real edit under a stack of selection changes — which is the failure this
    // guards against.
    //
    // Grouping is off deliberately. `UndoTree::push` merges everything pushed
    // inside `undo_group_timeout_ms` into one node, and a unit test runs well
    // inside 500ms — so with grouping on, expansions that *did* reach the
    // history would land in the same node as the insert and one undo would
    // still revert the text. The test would pass while the bug it names was
    // live. Zero puts every push on its own node, which is the only way the
    // assertion below can tell the two apart.
    let mut editor = editor_grouping_off(Language::Json, JSON);
    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: "  ".to_string(),
    });
    caret_before(&mut editor, JSON, "first");
    expand(&mut editor);
    expand(&mut editor);
    expand(&mut editor);

    assert!(editor.undo(), "the insert is still there to undo");
    assert_eq!(
        editor.state().document.text(),
        JSON,
        "the first undo undid an expansion instead of the edit, so three key \
         presses now stand between a person and their last real change"
    );
}

#[test]
fn a_document_with_no_language_ignores_every_structural_verb() {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_content(JSON);
    caret_before(&mut editor, JSON, "first");
    let before = editor.state().cursor.clone();

    expand(&mut editor);
    shrink(&mut editor);
    editor
        .run_command(AST_SELECT_NODE.as_str(), CommandArgs::default())
        .expect("the kernel implements it whether or not a language is set");

    assert_eq!(
        editor.state().cursor,
        before,
        "a file with no grammar has no structure, and the verbs must be quiet \
         rather than wrong"
    );
}

#[test]
fn select_node_snaps_a_partial_selection_to_the_node_holding_it() {
    let mut editor = editor(Language::Json, JSON);
    let start = JSON.find("irs").expect("a fragment inside \"first\"");
    let document = &editor.state().document;
    let from = document
        .offset_to_position(start)
        .expect("inside the document");
    let to = document
        .offset_to_position(start + 3)
        .expect("inside the document");
    editor.state_mut().cursor = crate::document::CursorState::new(Selection::new(from, to));

    editor
        .run_command(AST_SELECT_NODE.as_str(), CommandArgs::default())
        .expect("the kernel implements it");

    assert_eq!(
        selected(&editor, JSON),
        vec!["first"],
        "a selection cutting across a token must snap out to the whole token"
    );
}

#[test]
fn a_backwards_selection_stays_backwards() {
    // Direction is not decoration: it decides which end `Shift`+arrow grows
    // from, so an expansion that silently flips it moves the wrong edge next.
    let mut editor = editor(Language::Json, JSON);
    let start = JSON.find("first").expect("the fixture has it");
    let document = &editor.state().document;
    let head = document.offset_to_position(start).expect("inside");
    let anchor = document.offset_to_position(start + 5).expect("inside");
    editor.state_mut().cursor = crate::document::CursorState::new(Selection::new(anchor, head));
    assert!(editor.state().cursor.primary.is_backward());

    expand(&mut editor);

    assert!(
        editor.state().cursor.primary.is_backward(),
        "expansion turned a backwards selection forwards"
    );
    assert_eq!(selected(&editor, JSON), vec!["\"first\""]);
}

#[test]
fn shrinking_twice_with_no_stack_keeps_narrowing() {
    // The downward walk must not record a frame on the way down. If it did, the
    // second press would pop what the first pushed and widen the selection back
    // out — a key that alternates instead of narrowing.
    let mut editor = editor(Language::Json, JSON);
    let end = editor
        .state()
        .document
        .offset_to_position(JSON.len())
        .expect("the end of the document");
    editor.state_mut().cursor =
        crate::document::CursorState::new(Selection::new(Position::new(0, 0), end));

    shrink(&mut editor);
    let once = selected(&editor, JSON);
    shrink(&mut editor);
    let twice = selected(&editor, JSON);

    assert_ne!(once, twice, "the second shrink undid the first");
    assert!(
        twice[0].len() < once[0].len(),
        "shrinking twice widened the selection: {once:?} then {twice:?}"
    );
}

#[test]
fn expanding_forgets_the_sticky_preferred_column() {
    // Vertical motion remembers the column a caret came from, so it can return
    // to it across a short line. An expansion moves the caret somewhere that
    // memory knows nothing about, so the memory has to go — otherwise the next
    // `Down` lands under where the caret used to be, not where it now is.
    const LINES: &str = "fn f() {\n    let aaaaaaaaaaaaaaaaaaaaaaaaaaaa = 1;\n    let bb = 2;\n\
                             let cccccccccccccccccccccccccccc = 3;\n}\n";
    let mut editor = editor(Language::Rust, LINES);

    // Sanity: the sticky column is real. From column 30 on a long line, down
    // onto a short one and down again returns to 30.
    editor.set_cursor(Position::new(1, 30));
    editor.handle_key(&KeyEvent::simple(KeyCode::Down));
    assert_eq!(editor.state().cursor.primary.head, Position::new(2, 15));
    editor.handle_key(&KeyEvent::simple(KeyCode::Down));
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(3, 30),
        "the sticky column is not doing what this test assumes"
    );

    // The same trip again, with an expansion in the middle of it.
    editor.set_cursor(Position::new(1, 30));
    editor.handle_key(&KeyEvent::simple(KeyCode::Down));

    editor.handle_key(&KeyEvent::new(
        KeyCode::Right,
        Modifiers {
            shift: true,
            alt: true,
            ..Modifiers::default()
        },
    ));
    assert_eq!(
        selected(&editor, LINES),
        vec!["let bb = 2;"],
        "the Shift+Alt+Right binding did not reach expansion"
    );

    editor.handle_key(&KeyEvent::simple(KeyCode::Down));
    assert_eq!(
        editor.state().cursor.primary.head,
        Position::new(3, 15),
        "`Down` after an expansion returned to column 30, which is where the \
         caret was before the expansion moved it"
    );
}

// ===== Step 7: siblings, children, caret motions, multi-cursor =====

/// The JSON these tests walk along.
///
/// Three array elements of different widths, so a walk landing on the wrong one
/// is visible in the assertion rather than coincidentally right — and a second
/// pair after the array, so that climbing out of its last element has somewhere
/// to arrive. Without the tail, "the last sibling climbs" would be untestable:
/// the array would be the only thing in the object and the honest answer would
/// be that there is nowhere to go.
const ARRAY: &str = "{\"values\": [111, 22, 3333], \"tail\": 9}";

/// Runs a structural verb by id, failing loudly if the kernel does not have it.
fn ast(editor: &mut Editor, id: &CommandId) {
    editor
        .run_command(id.as_str(), CommandArgs::default())
        .unwrap_or_else(|error| panic!("the kernel implements `{}`: {error}", id.as_str()));
}

/// Where each caret sits, as byte offsets, primary first.
fn carets(editor: &Editor) -> Vec<usize> {
    editor
        .state()
        .cursor
        .all_selections()
        .map(|selection| {
            editor
                .state()
                .document
                .position_to_offset(selection.cursor_position())
                .expect("a live caret resolves")
        })
        .collect()
}

#[test]
fn walking_to_the_next_sibling_crosses_the_array_one_element_at_a_time() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    ast(&mut editor, &AST_SELECT_NODE);
    assert_eq!(selected(&editor, ARRAY), vec!["111"]);

    ast(&mut editor, &AST_SELECT_NEXT_SIBLING);
    assert_eq!(selected(&editor, ARRAY), vec!["22"]);

    ast(&mut editor, &AST_SELECT_NEXT_SIBLING);
    assert_eq!(selected(&editor, ARRAY), vec!["3333"]);
}

#[test]
fn the_last_sibling_climbs_rather_than_stopping_dead() {
    // A walk that refuses to leave the array is a walk that strands the person
    // at its end. Climbing means the key keeps meaning "onward".
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "3333");
    ast(&mut editor, &AST_SELECT_NODE);
    assert_eq!(selected(&editor, ARRAY), vec!["3333"]);

    ast(&mut editor, &AST_SELECT_NEXT_SIBLING);
    assert_ne!(
        selected(&editor, ARRAY),
        vec!["3333"],
        "the last element of an array must still have somewhere to go"
    );
}

#[test]
fn the_sibling_walks_are_inverses_of_each_other() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "22");
    ast(&mut editor, &AST_SELECT_NODE);
    let start = selected(&editor, ARRAY);

    ast(&mut editor, &AST_SELECT_NEXT_SIBLING);
    ast(&mut editor, &AST_SELECT_PREVIOUS_SIBLING);

    assert_eq!(selected(&editor, ARRAY), start);
}

#[test]
fn descending_to_a_child_and_back_out_reaches_the_same_place() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    // Two, not three: from a caret the ladder is `111`, then the array. A third
    // press would already be out at the enclosing `"values": [...]` pair.
    for _ in 0..2 {
        expand(&mut editor);
    }
    assert_eq!(selected(&editor, ARRAY), vec!["[111, 22, 3333]"]);

    ast(&mut editor, &AST_SELECT_FIRST_CHILD);
    assert_eq!(selected(&editor, ARRAY), vec!["111"]);
}

#[test]
fn the_last_child_is_the_last_named_one_not_the_closing_brace() {
    // Anonymous nodes are the grammar's punctuation. Landing on `]` would be a
    // selection nobody can do anything with.
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    // Two, not three: from a caret the ladder is `111`, then the array. A third
    // press would already be out at the enclosing `"values": [...]` pair.
    for _ in 0..2 {
        expand(&mut editor);
    }

    ast(&mut editor, &AST_SELECT_LAST_CHILD);
    assert_eq!(selected(&editor, ARRAY), vec!["3333"]);
}

#[test]
fn a_node_with_no_children_leaves_the_selection_alone() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    ast(&mut editor, &AST_SELECT_NODE);
    let before = editor.state().cursor.clone();

    ast(&mut editor, &AST_SELECT_FIRST_CHILD);

    assert_eq!(
        editor.state().cursor,
        before,
        "a number has no parts, and a key that cannot move must not pretend to"
    );
}

#[test]
fn extending_picks_up_the_commas_between_the_elements() {
    // The distinction that makes extend worth having separately from select:
    // the result must be a range that can be cut and pasted, which means the
    // punctuation holding the elements apart comes with it.
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    ast(&mut editor, &AST_SELECT_NODE);

    ast(&mut editor, &AST_EXTEND_NEXT_SIBLING);
    assert_eq!(selected(&editor, ARRAY), vec!["111, 22"]);

    ast(&mut editor, &AST_EXTEND_NEXT_SIBLING);
    assert_eq!(selected(&editor, ARRAY), vec!["111, 22, 3333"]);
}

#[test]
fn extending_backwards_grows_from_the_other_end() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "3333");
    ast(&mut editor, &AST_SELECT_NODE);

    ast(&mut editor, &AST_EXTEND_PREVIOUS_SIBLING);

    assert_eq!(selected(&editor, ARRAY), vec!["22, 3333"]);
}

#[test]
fn extending_never_shrinks_what_is_already_selected() {
    // The property a held key rests on. `union` is what guarantees it, and a
    // sibling step that returned something narrower would silently drop text
    // from a selection the person was building up.
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "22");
    ast(&mut editor, &AST_SELECT_NODE);

    let mut widest = 0;
    for _ in 0..6 {
        ast(&mut editor, &AST_EXTEND_NEXT_SIBLING);
        let length = selected(&editor, ARRAY)[0].len();
        assert!(
            length >= widest,
            "an extend shrank the selection from {widest} to {length} characters"
        );
        widest = length;
    }
}

#[test]
fn the_caret_motions_walk_outward_on_repeated_presses() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "22");

    // Pressed until it stops rather than a fixed count: where the ladder ends is
    // the thing being measured, so a hard-coded number of presses would be
    // asserting the answer it was given.
    let mut seen = Vec::new();
    loop {
        let before = carets(&editor);
        ast(&mut editor, &AST_CURSOR_NODE_START);
        let after = carets(&editor);
        if after == before {
            break;
        }
        seen.push(after[0]);
        assert!(seen.len() < 32, "the walk is not converging: {seen:?}");
    }

    assert!(
        seen.windows(2).all(|pair| pair[0] > pair[1]),
        "each press must move further left, but the walk went {seen:?}"
    );
    assert_eq!(
        seen.last().copied(),
        Some(0),
        "walking outward far enough must reach the start of the document"
    );
}

#[test]
fn the_end_motion_reaches_the_end_of_the_document() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "22");

    for _ in 0..6 {
        ast(&mut editor, &AST_CURSOR_NODE_END);
    }

    assert_eq!(carets(&editor), vec![ARRAY.len()]);
}

#[test]
fn a_caret_motion_collapses_the_selection_it_started_from() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    // Two, not three: from a caret the ladder is `111`, then the array. A third
    // press would already be out at the enclosing `"values": [...]` pair.
    for _ in 0..2 {
        expand(&mut editor);
    }
    assert_eq!(selected(&editor, ARRAY), vec!["[111, 22, 3333]"]);

    ast(&mut editor, &AST_CURSOR_NODE_END);

    assert_eq!(
        selected(&editor, ARRAY),
        vec![""],
        "a go-to-position verb must leave a caret, not a selection"
    );
}

#[test]
fn a_cursor_lands_on_every_element_of_the_array() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "22");
    ast(&mut editor, &AST_SELECT_NODE);

    ast(&mut editor, &AST_CURSOR_ON_EVERY_SIBLING);

    let mut texts = selected(&editor, ARRAY);
    texts.sort_unstable();
    assert_eq!(texts, vec!["111", "22", "3333"]);
    assert_eq!(
        editor.state().cursor.cursor_count(),
        3,
        "siblings are disjoint, so none of them may merge away"
    );
}

#[test]
fn the_primary_cursor_stays_on_the_node_it_started_from() {
    // Without this the primary silently becomes the first element, and the next
    // `Escape` collapses to somewhere the person was not.
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "22");
    ast(&mut editor, &AST_SELECT_NODE);

    ast(&mut editor, &AST_CURSOR_ON_EVERY_SIBLING);

    assert_eq!(
        selected(&editor, ARRAY)[0],
        "22",
        "the spread moved the primary off the node it was launched from"
    );
}

#[test]
fn a_cursor_lands_on_every_child_of_the_selected_node() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    // Two, not three: from a caret the ladder is `111`, then the array. A third
    // press would already be out at the enclosing `"values": [...]` pair.
    for _ in 0..2 {
        expand(&mut editor);
    }
    assert_eq!(selected(&editor, ARRAY), vec!["[111, 22, 3333]"]);

    ast(&mut editor, &AST_CURSOR_ON_EVERY_CHILD);

    let mut texts = selected(&editor, ARRAY);
    texts.sort_unstable();
    assert_eq!(texts, vec!["111", "22", "3333"]);
}

#[test]
fn spreading_onto_a_node_that_is_already_the_only_cursor_changes_nothing() {
    let mut editor = editor(Language::Json, "[1]");
    caret_before(&mut editor, "[1]", "1");
    ast(&mut editor, &AST_SELECT_NODE);
    let before = editor.state().cursor.clone();

    ast(&mut editor, &AST_CURSOR_ON_EVERY_CHILD);

    assert_eq!(
        editor.state().cursor,
        before,
        "a number has no children, and a spread with nothing to spread must not \
         report a change"
    );
}

#[test]
fn a_spread_that_lands_where_it_started_reports_that_nothing_happened() {
    // Comparing cursor states is not enough to catch this: a spread that returns
    // the state it was given still *applies* it, and the cursors look identical
    // either way. What differs is the claim — and the cost of a false claim is
    // the expansion stack, which every moving verb clears. So a lone element
    // whose only sibling is itself would silently destroy the ability to shrink
    // back, from a key that visibly did nothing.
    let mut editor = editor(Language::Json, "[1]");
    caret_before(&mut editor, "[1]", "1");
    expand(&mut editor);
    expand(&mut editor);
    let depth = editor.state().syntax.expansion_depth();
    assert!(
        depth > 0,
        "there must be frames for this test to protect them"
    );
    let before = editor.state().cursor.clone();

    let moved = editor.perform_ast_request(AstRequest::CursorOnEverySibling);

    assert!(
        !moved,
        "the spread reported a change it did not make; every caller that acts on \
         that answer is now acting on a lie"
    );
    assert_eq!(editor.state().cursor, before);
    assert_eq!(
        editor.state().syntax.expansion_depth(),
        depth,
        "a verb that did nothing threw away the frames a shrink needs"
    );
}

#[test]
fn a_sideways_step_throws_away_the_expansion_stack() {
    // The rule that keeps shrink honest. After walking to a sibling, the state
    // expansion started from is no longer where "back" leads — and a shrink that
    // jumped there would land on a range the person never looked at.
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    expand(&mut editor);
    expand(&mut editor);
    assert!(
        editor.state().syntax.expansion_depth() > 0,
        "the expansions must have been recorded for this test to mean anything"
    );

    ast(&mut editor, &AST_SELECT_NEXT_SIBLING);

    assert_eq!(
        editor.state().syntax.expansion_depth(),
        0,
        "walking sideways left frames behind that a later shrink would retrace"
    );
}

#[test]
fn shrinking_after_a_sideways_step_narrows_rather_than_jumping_back() {
    let mut editor = editor(Language::Json, ARRAY);
    caret_before(&mut editor, ARRAY, "111");
    // Two, not three: from a caret the ladder is `111`, then the array. A third
    // press would already be out at the enclosing `"values": [...]` pair.
    for _ in 0..2 {
        expand(&mut editor);
    }
    assert_eq!(selected(&editor, ARRAY), vec!["[111, 22, 3333]"]);

    ast(&mut editor, &AST_SELECT_NEXT_SIBLING);
    let after_step = selected(&editor, ARRAY);
    shrink(&mut editor);

    assert_ne!(
        selected(&editor, ARRAY),
        after_step,
        "the shrink did nothing at all"
    );
    assert_ne!(
        selected(&editor, ARRAY),
        vec!["111"],
        "the shrink retraced an expansion that the sideways step should have \
         invalidated"
    );
}

#[test]
fn every_structural_verb_is_silent_on_a_document_with_no_language() {
    // The whole set, not a sample: a verb that panics or misbehaves without a
    // tree is a verb that breaks the editor on any file whose language has no
    // grammar, which is most of them.
    for id in [
        &AST_SELECT_NEXT_SIBLING,
        &AST_SELECT_PREVIOUS_SIBLING,
        &AST_SELECT_FIRST_CHILD,
        &AST_SELECT_LAST_CHILD,
        &AST_EXTEND_NEXT_SIBLING,
        &AST_EXTEND_PREVIOUS_SIBLING,
        &AST_CURSOR_NODE_START,
        &AST_CURSOR_NODE_END,
        &AST_CURSOR_ON_EVERY_SIBLING,
        &AST_CURSOR_ON_EVERY_CHILD,
    ] {
        let mut editor = Editor::new(EditorConfig::default());
        editor.set_content(ARRAY);
        caret_before(&mut editor, ARRAY, "22");
        let before = editor.state().cursor.clone();

        ast(&mut editor, id);

        assert_eq!(
            editor.state().cursor,
            before,
            "`{}` moved the cursor in a document with no parse tree",
            id.as_str()
        );
    }
}

#[test]
fn every_structural_verb_leaves_the_document_untouched() {
    // None of these is `.mutating()`, which is what keeps them available in a
    // read-only buffer. If one of them ever edited text that claim would be a
    // lie, and the lie would only surface in a read-only file.
    for id in [
        &AST_SELECT_NEXT_SIBLING,
        &AST_SELECT_PREVIOUS_SIBLING,
        &AST_SELECT_FIRST_CHILD,
        &AST_SELECT_LAST_CHILD,
        &AST_EXTEND_NEXT_SIBLING,
        &AST_EXTEND_PREVIOUS_SIBLING,
        &AST_CURSOR_NODE_START,
        &AST_CURSOR_NODE_END,
        &AST_CURSOR_ON_EVERY_SIBLING,
        &AST_CURSOR_ON_EVERY_CHILD,
    ] {
        let mut editor = editor(Language::Json, ARRAY);
        caret_before(&mut editor, ARRAY, "22");

        ast(&mut editor, id);

        assert_eq!(
            editor.state().document.text(),
            ARRAY,
            "`{}` changed the document",
            id.as_str()
        );
    }
}
