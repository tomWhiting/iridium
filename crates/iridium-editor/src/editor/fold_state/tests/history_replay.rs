//! Folds after an undo, a redo, or a jump across the undo tree.
//!
//! Undo and redo mutate the document without going through
//! `apply_command_internal`, so they are the one editing path that can leave the
//! parse tree — and therefore the folds — describing a document that no longer
//! exists. These run in every feature configuration, because the defect is in
//! the editor's control flow rather than in either fold detector: the oracle is
//! a second editor loaded with the same text, which is by construction whatever
//! a full recompute would produce in whichever configuration is being built.

use crate::editor::{Editor, EditorConfig};
use crate::{Command, Language, Position};

/// An editor holding `content`, with folds already built for it.
///
/// `set_language` refreshes the folds, so the returned editor's regions are
/// exactly the ones a from-scratch detection of `content` produces. That is
/// what makes a second instance a usable oracle.
///
/// Undo grouping is switched off. Left on, two edits made microseconds apart
/// merge into one history node, and a test that means to cross two nodes
/// silently crosses one.
fn editor_for(content: &str) -> Editor {
    let mut editor = Editor::new(EditorConfig::default());
    editor.set_undo_group_timeout_ms(0);
    editor.set_content(content);
    editor.set_language(Language::Rust);
    editor
}

/// Asserts `editor`'s folds are the ones its current text deserves.
fn assert_folds_match_a_fresh_load(editor: &Editor, step: &str) {
    let oracle = editor_for(&editor.content());
    assert_eq!(
        editor.state().fold_state.regions(),
        oracle.state().fold_state.regions(),
        "{step}: the editor's folds describe a document it no longer holds"
    );
}

/// A whole function, so undoing it changes the fold set rather than just
/// moving it.
const INSERTED_BLOCK: &str = "fn inserted() {\n    one();\n    two();\n}\n\n";

/// The bug, stated as a test.
///
/// `finish_history_replay` refreshed the search and emitted content-changed
/// but never refreshed the syntax tree, so every fold region after an undo
/// described the document as it was before the undo — until some later
/// content command happened to refresh them.
#[test]
fn folds_are_current_after_an_undo() {
    let mut editor = editor_for("fn kept() {\n    zero();\n}\n");
    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: INSERTED_BLOCK.to_owned(),
    });
    assert_folds_match_a_fresh_load(&editor, "after the insert");

    assert!(editor.undo(), "the insert is undoable");
    assert_folds_match_a_fresh_load(&editor, "after the undo");
}

/// Redo is the same path and fails the same way.
///
/// Two inserts and two undos before the redo, deliberately. A single
/// insert-undo-redo lands back on the document the stale regions were built
/// from, so it passes against the defect and proves nothing; landing on an
/// intermediate state is what makes the staleness observable.
#[test]
fn folds_are_current_after_a_redo() {
    let mut editor = editor_for("fn kept() {\n    zero();\n}\n");
    for text in ["fn a() {\n    a();\n}\n", "fn b() {\n    b();\n}\n"] {
        editor.apply_command(Command::Insert {
            position: Position::new(0, 0),
            text: text.to_owned(),
        });
    }
    assert!(editor.undo(), "the second insert is undoable");
    assert!(editor.undo(), "the first insert is undoable");

    assert!(editor.redo(), "the first insert is redoable");
    assert_folds_match_a_fresh_load(&editor, "after the redo");
}

/// A jump across the undo tree replays several commands before the shared
/// post-replay step runs, so it is the path where an unreported edit would
/// survive longest.
#[test]
fn folds_are_current_after_a_history_jump() {
    let mut editor = editor_for("fn kept() {\n    zero();\n}\n");
    let root = editor.current_history_node();

    for text in ["fn a() {\n    a();\n}\n", "fn b() {\n    b();\n}\n"] {
        editor.apply_command(Command::Insert {
            position: Position::new(0, 0),
            text: text.to_owned(),
        });
    }

    assert!(editor.jump_to_history_node(root), "the root is reachable");
    assert_folds_match_a_fresh_load(&editor, "after the jump");
}

/// Bringing the tree back in step must not cost a whole-document parse.
///
/// A replayed command is an ordinary edit against the document it is applied
/// to, so its span can be measured before it lands and reported exactly as a
/// typed edit's is. If that ever stops happening the tree still ends up
/// correct — the revision comparison inside `sync` sees to that — but it
/// costs a full parse per undo, which is precisely the O(document) keystroke
/// cost this whole line of work removed. Counting parses is how that stays
/// visible.
#[test]
fn an_undo_reparses_incrementally_rather_than_wholly() {
    let mut editor = editor_for("fn kept() {\n    zero();\n}\n");
    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: INSERTED_BLOCK.to_owned(),
    });
    let full_before = editor.state().syntax.full_parses();
    let incremental_before = editor.state().syntax.incremental_parses();

    assert!(editor.undo(), "the insert is undoable");

    assert_eq!(
        editor.state().syntax.full_parses(),
        full_before,
        "an undo parsed the whole document; the replayed edit's span was \
         not reported to the tree"
    );
    assert_eq!(
        editor.state().syntax.incremental_parses(),
        incremental_before + 1,
        "an undo did not reparse at all, so the tree still describes the \
         document as it was before the undo"
    );
}

/// Undo must not leave the folds correct only by accident of nothing having
/// changed.
///
/// The insert adds a fold region; if the assertions above passed with the
/// region set never moving, they would be proving nothing.
#[test]
fn the_replayed_edit_really_does_move_the_fold_set() {
    let mut editor = editor_for("fn kept() {\n    zero();\n}\n");
    let before = editor.state().fold_state.regions().to_vec();
    editor.apply_command(Command::Insert {
        position: Position::new(0, 0),
        text: INSERTED_BLOCK.to_owned(),
    });
    assert_ne!(
        editor.state().fold_state.regions(),
        before.as_slice(),
        "the fixture must change the fold set, or the undo tests are vacuous"
    );
}
