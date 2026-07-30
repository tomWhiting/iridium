//! Tests for undo-tree navigation at the [`Editor`] level.
//!
//! Every other undo-tree regression drives [`crate::UndoTree`] directly, with a
//! single cursor at the origin, and asserts only the document text. Production
//! never does that: it goes through [`Editor`], which also restores cursor
//! topology, invalidates the multi-cursor addition order and refreshes the
//! active search. These tests cover the branching story on that path.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::document::Selection;
use crate::input::{KeyCode, KeyEvent, Modifiers};
use crate::{EditorConfig, Position};

/// An editor with edit grouping disabled, so one keystroke is one undo step
/// and the tree shape under test is exactly the one written here.
fn editor_with_ungrouped_history() -> Editor {
    let mut editor = Editor::new(EditorConfig {
        undo_group_timeout_ms: 0,
        ..EditorConfig::default()
    });
    editor.set_content("");
    editor
}

/// Types one character through the keyboard path, so the recorded command
/// carries the trailing `SetSelection` that real edits carry.
fn type_char(editor: &mut Editor, ch: char) {
    editor.handle_key(&KeyEvent::new(KeyCode::Char(ch), Modifiers::default()));
}

fn cursor_heads(editor: &Editor) -> Vec<Position> {
    editor
        .state()
        .cursor
        .all_selections()
        .map(|sel| sel.head)
        .collect()
}

/// The scenario the undo tree exists for: undo a couple of things, make a
/// different change, then walk back down into the branch that was abandoned —
/// and back out again. No work is lost either way.
#[test]
fn editor_can_walk_both_branches_of_a_fork() {
    let mut editor = editor_with_ungrouped_history();

    // First branch: "ab".
    type_char(&mut editor, 'a');
    type_char(&mut editor, 'b');
    assert_eq!(editor.content(), "ab");

    // Retreat to "a" and take a different turn: "aZ".
    assert!(editor.undo());
    assert_eq!(editor.content(), "a");
    type_char(&mut editor, 'Z');
    assert_eq!(editor.content(), "aZ");

    // Back to the fork point. Two branches now leave this node.
    assert!(editor.undo());
    assert_eq!(editor.content(), "a");
    let branches = editor.history_branches();
    assert_eq!(branches.len(), 2, "the abandoned branch was not preserved");
    assert_eq!(
        branches[1].preferred_child_id, None,
        "a freshly created leaf has no active child"
    );

    // A plain redo takes the branch just travelled — the new work, not the
    // abandoned one.
    assert!(editor.redo());
    assert_eq!(editor.content(), "aZ");

    // Explicitly descend the older branch instead.
    assert!(editor.undo());
    assert!(editor.redo_branch(0));
    assert_eq!(editor.content(), "ab", "branch 0 was not reachable");

    // That choice becomes the active path, so an undo/redo round-trip stays
    // on it rather than snapping back to the newer branch.
    assert!(editor.undo());
    assert!(editor.redo());
    assert_eq!(editor.content(), "ab");

    // And the other branch is still there to go back to.
    assert!(editor.undo());
    assert!(editor.redo_branch(1));
    assert_eq!(editor.content(), "aZ");
}

/// A branch switch must restore the full cursor topology of the state it lands
/// on, not merely the text. Multi-cursor state is where this bites: entering a
/// branch recorded with three carets while two are live would leave the editor
/// showing a document it cannot correctly type into.
#[test]
fn redo_branch_restores_multi_cursor_topology() {
    let mut editor = editor_with_ungrouped_history();
    editor.set_content("....\n....\n....");

    // Three carets, one per line, at column 2.
    editor.set_cursor(Position::new(0, 2));
    editor
        .state_mut()
        .cursor
        .add_cursor(Selection::collapsed(Position::new(1, 2)));
    editor
        .state_mut()
        .cursor
        .add_cursor(Selection::collapsed(Position::new(2, 2)));

    // Branch A: typed with all three carets live.
    type_char(&mut editor, 'A');
    assert_eq!(editor.content(), "..A..\n..A..\n..A..");
    assert_eq!(
        cursor_heads(&editor),
        vec![
            Position::new(0, 3),
            Position::new(1, 3),
            Position::new(2, 3)
        ]
    );

    // Retreat, collapse to a single caret, and take a different turn.
    assert!(editor.undo());
    assert_eq!(cursor_heads(&editor).len(), 3, "undo lost the caret block");
    editor.set_cursor(Position::new(0, 0));
    type_char(&mut editor, 'B');
    assert_eq!(editor.content(), "B....\n....\n....");
    assert!(editor.undo());

    // Descend branch A again. Its three carets must come back with it.
    assert_eq!(editor.history_branches().len(), 2);
    assert!(editor.redo_branch(0));
    assert_eq!(editor.content(), "..A..\n..A..\n..A..");
    assert_eq!(
        cursor_heads(&editor),
        vec![
            Position::new(0, 3),
            Position::new(1, 3),
            Position::new(2, 3)
        ],
        "entering a branch restored its text but not its carets"
    );

    // And branch B's single caret must come back with it.
    assert!(editor.undo());
    assert!(editor.redo_branch(1));
    assert_eq!(editor.content(), "B....\n....\n....");
    assert_eq!(cursor_heads(&editor), vec![Position::new(0, 1)]);
}

/// A jump reaches a state no undo/redo sequence could reach from here without
/// abandoning the current branch, and it must leave the document, the cursor
/// and the tree pointer mutually consistent.
#[test]
fn jump_across_branches_replays_document_and_cursor() {
    let mut editor = editor_with_ungrouped_history();

    type_char(&mut editor, 'a');
    type_char(&mut editor, 'b');
    let deep_in_first_branch = editor.current_history_node();
    let cursor_in_first_branch = cursor_heads(&editor);

    assert!(editor.undo());
    assert!(editor.undo());
    type_char(&mut editor, 'X');
    type_char(&mut editor, 'Y');
    let deep_in_second_branch = editor.current_history_node();
    assert_eq!(editor.content(), "XY");

    // Jump sideways: up two levels to the root, then down two into the other
    // branch. Nothing in undo/redo can do this in one step.
    assert!(editor.jump_to_history_node(deep_in_first_branch));
    assert_eq!(editor.content(), "ab");
    assert_eq!(cursor_heads(&editor), cursor_in_first_branch);
    assert_eq!(editor.current_history_node(), deep_in_first_branch);
    assert!(
        editor
            .history_node(deep_in_first_branch)
            .expect("target node exists")
            .is_current
    );

    // The tree pointer and the document agree, so an undo from here undoes
    // *this* branch's last edit rather than replaying against a state that
    // never existed.
    assert!(editor.undo());
    assert_eq!(editor.content(), "a");

    // And the branch jumped away from is intact.
    assert!(editor.jump_to_history_node(deep_in_second_branch));
    assert_eq!(editor.content(), "XY");
}

/// Branch navigation must notify the host exactly once per destination.
///
/// This is what separates these methods from driving the tree directly through
/// [`Editor::state_mut`]: that path moves the pointer and applies commands but
/// runs none of the post-replay work, so a host would render a stale document.
/// A jump crossing four edges is still one destination, so it must emit one
/// content-changed event, not four.
#[test]
fn branch_navigation_notifies_the_host_once_per_destination() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let mut editor = editor_with_ungrouped_history();

    type_char(&mut editor, 'a');
    type_char(&mut editor, 'b');
    let deep_in_first_branch = editor.current_history_node();
    assert!(editor.undo());
    assert!(editor.undo());
    type_char(&mut editor, 'X');
    type_char(&mut editor, 'Y');
    assert!(editor.undo());
    assert!(editor.undo());

    let content_events = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&content_events);
    editor.add_listener(move |event| {
        if matches!(event, EditorEvent::ContentChanged { .. }) {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    });

    assert!(editor.redo_branch(0));
    assert_eq!(
        content_events.load(Ordering::SeqCst),
        1,
        "entering a branch did not tell the host the document changed"
    );

    // Four edges travelled — down one, up one, down two — and one destination.
    assert!(editor.undo());
    content_events.store(0, Ordering::SeqCst);
    assert!(editor.jump_to_history_node(deep_in_first_branch));
    assert_eq!(editor.content(), "ab");
    assert_eq!(
        content_events.load(Ordering::SeqCst),
        1,
        "a jump leaked its intermediate states to the host"
    );

    // A refused navigation must not claim anything changed.
    content_events.store(0, Ordering::SeqCst);
    assert!(!editor.jump_to_history_node(UndoNodeId::from_u64(u64::MAX)));
    assert!(!editor.redo_branch(usize::MAX));
    assert_eq!(content_events.load(Ordering::SeqCst), 0);
}

/// A jump to the node already occupied changes nothing and reports success.
#[test]
fn jump_to_current_node_is_a_no_op() {
    let mut editor = editor_with_ungrouped_history();
    type_char(&mut editor, 'a');

    let here = editor.current_history_node();
    let cursors = cursor_heads(&editor);

    assert!(editor.jump_to_history_node(here));
    assert_eq!(editor.content(), "a");
    assert_eq!(cursor_heads(&editor), cursors);
    assert_eq!(editor.current_history_node(), here);
}

/// Navigation to somewhere that does not exist must be refused outright, with
/// the document and the tree pointer untouched — never a partial replay.
#[test]
fn navigation_to_a_missing_target_changes_nothing() {
    let mut editor = editor_with_ungrouped_history();
    type_char(&mut editor, 'a');

    let here = editor.current_history_node();
    let unknown = UndoNodeId::from_u64(u64::MAX);

    assert!(!editor.jump_to_history_node(unknown));
    assert!(!editor.redo_branch(0), "a leaf has no branch 0");
    assert!(!editor.redo_branch(usize::MAX));

    assert_eq!(editor.content(), "a");
    assert_eq!(editor.current_history_node(), here);
    assert!(editor.history_node(unknown).is_none());
}

/// The reported node metadata describes the tree that was actually built.
#[test]
fn node_info_describes_the_tree_as_built() {
    let mut editor = editor_with_ungrouped_history();
    type_char(&mut editor, 'a');
    let first = editor.current_history_node();

    assert!(editor.undo());
    let root = editor.current_history_node();

    let root_info = editor.history_node(root).expect("root exists");
    assert_eq!(root_info.parent_id, None, "the root has no parent");
    assert!(root_info.is_current);
    assert_eq!(root_info.child_ids.len(), 1);
    assert_eq!(
        root_info.preferred_child_id.as_deref(),
        Some(first.as_u64().to_string().as_str()),
        "the departed child is the active path"
    );

    let first_info = editor.history_node(first).expect("first edit exists");
    assert_eq!(first_info.parent_id.as_deref(), Some(root_info.id.as_str()));
    assert!(first_info.child_ids.is_empty());
    assert!(!first_info.is_current);
    assert!(first_info.description.is_none());
    assert!(
        first_info.elapsed_ms >= root_info.elapsed_ms,
        "an edit cannot predate the tree it belongs to"
    );

    // Descriptions are what an undo-tree view shows in place of a node id.
    assert!(
        editor
            .state_mut()
            .history
            .set_description(first, Some("typed a".to_string()))
    );
    assert_eq!(
        editor
            .history_node(first)
            .expect("first edit exists")
            .description
            .as_deref(),
        Some("typed a")
    );
    assert!(
        !editor
            .state_mut()
            .history
            .set_description(UndoNodeId::from_u64(u64::MAX), Some("nowhere".to_string()))
    );
}
