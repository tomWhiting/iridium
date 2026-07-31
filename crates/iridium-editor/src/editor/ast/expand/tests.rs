//! Tests for structural selection and the stack that makes shrinking exact.
//!
//! Driven through the whole editor rather than the stack alone. The claims worth
//! holding — "shrink gives back exactly what expand took", "an edit invalidates
//! it", "expanding never fills the undo history" — are claims about the editor's
//! behaviour, and a test against the stack in isolation would prove them of a
//! component nobody uses.

use iridium_syntax::Language;

use crate::commands::CommandArgs;
use crate::commands::builtin::{AST_EXPAND_SELECTION, AST_SELECT_NODE, AST_SHRINK_SELECTION};
use crate::document::{Position, Selection};
use crate::editor::{Editor, EditorConfig};
use crate::history::Command;
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
