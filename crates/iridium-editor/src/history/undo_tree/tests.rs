//! Tests for the undo tree.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::document::{CursorState, Document, Position};

fn insert_cmd(text: &str) -> Command {
    Command::Insert {
        position: Position::new(0, 0),
        text: text.to_string(),
    }
}

/// Builds an insert command at the given column on line 0.
fn insert_at(column: usize, text: &str) -> Command {
    Command::Insert {
        position: Position::new(0, column),
        text: text.to_string(),
    }
}

#[test]
fn new_tree() {
    let tree = UndoTree::new();
    assert!(!tree.can_undo());
    assert!(!tree.can_redo());
    assert_eq!(tree.branch_count(), 0);
}

#[test]
fn push_and_undo() {
    let mut tree = UndoTree::with_timeout(0); // Disable grouping
    tree.push(insert_cmd("Hello"));

    assert!(tree.can_undo());
    assert!(!tree.can_redo());

    let undo_cmd = tree.undo();
    assert!(undo_cmd.is_some());
    assert!(!tree.can_undo());
    assert!(tree.can_redo());
}

#[test]
fn undo_redo() {
    let mut tree = UndoTree::with_timeout(0);
    tree.push(insert_cmd("Hello"));

    tree.undo();
    assert!(tree.can_redo());

    let redo_cmd = tree.redo();
    assert!(redo_cmd.is_some());
    assert!(!tree.can_redo());
}

#[test]
fn branching() {
    let mut tree = UndoTree::with_timeout(0);

    // Initial edit
    tree.push(insert_cmd("A"));
    tree.undo();

    // Create branch
    tree.push(insert_cmd("B"));

    // Should have 2 branches at root
    tree.undo();
    assert_eq!(tree.branch_count(), 2);
}

#[test]
fn grouped_pushes_produce_flat_compound() {
    // Large timeout so all pushes fall inside the group window.
    let mut tree = UndoTree::with_timeout(10_000);

    tree.push(insert_at(0, "a"));
    tree.push(insert_at(1, "b"));
    tree.push(insert_at(2, "c"));
    tree.push(insert_at(3, "d"));
    tree.push(insert_at(4, "e"));

    // Grouping must not create extra nodes: root plus one edit node.
    assert_eq!(tree.get_tree_info().node_count, 2);

    // A single undo must revert all five grouped commands.
    let inverse = tree.undo().expect("undo should return inverse command");
    assert!(!tree.can_undo(), "one undo should reach the root");

    // The inverse must be a flat compound with 5 direct, non-compound
    // children — the old implementation nested one level per keystroke.
    match inverse {
        Command::Compound { commands } => {
            assert_eq!(commands.len(), 5, "expected 5 flat children");
            for child in &commands {
                assert!(
                    !matches!(child, Command::Compound { .. }),
                    "grouped compound must not be nested"
                );
            }
        },
        other => panic!("expected Command::Compound, got {other:?}"),
    }
}

#[test]
fn undo_after_grouped_typing_restores_document() {
    let mut tree = UndoTree::with_timeout(10_000);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    // Simulate a fast typing burst: apply each keystroke and push it.
    for (column, ch) in ["h", "e", "l", "l", "o"].iter().enumerate() {
        let cmd = insert_at(column, ch);
        cmd.apply(&mut doc, &mut cursor).expect("apply keystroke");
        tree.push(cmd);
    }
    assert_eq!(doc.text(), "hello");

    // One undo must restore the original (empty) document text.
    let inverse = tree.undo().expect("undo should return inverse command");
    inverse.apply(&mut doc, &mut cursor).expect("apply inverse");
    assert_eq!(doc.text(), "");

    // Redo must reapply the full burst.
    let redo = tree.redo().expect("redo should return command");
    redo.apply(&mut doc, &mut cursor).expect("apply redo");
    assert_eq!(doc.text(), "hello");
}

#[test]
fn push_after_timeout_creates_new_node() {
    // Zero timeout: elapsed time is never strictly less than the
    // timeout, so consecutive pushes must never group.
    let mut tree = UndoTree::with_timeout(0);

    tree.push(insert_at(0, "a"));
    tree.push(insert_at(1, "b"));

    // Root plus two distinct edit nodes.
    assert_eq!(tree.get_tree_info().node_count, 3);

    // Two separate undos are required to reach the root.
    assert!(tree.undo().is_some());
    assert!(tree.can_undo(), "second edit node should remain");
    assert!(tree.undo().is_some());
    assert!(!tree.can_undo());
}

#[test]
fn jump_to_node_across_branch() {
    let mut tree = UndoTree::with_timeout(0);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    // Branch 1: insert "A" (node id 1).
    let cmd_a = insert_at(0, "A");
    cmd_a.apply(&mut doc, &mut cursor).expect("apply A");
    tree.push(cmd_a);

    // Undo back to root, then create branch 2: insert "B" (node id 2).
    let inv_a = tree.undo().expect("undo A");
    inv_a.apply(&mut doc, &mut cursor).expect("apply inverse A");
    let cmd_b = insert_at(0, "B");
    cmd_b.apply(&mut doc, &mut cursor).expect("apply B");
    tree.push(cmd_b);
    assert_eq!(doc.text(), "B");

    // Jump across the branch point: from node 2 to node 1.
    let commands = tree
        .jump_to_node(UndoNodeId::new(1))
        .expect("jump to sibling branch");
    assert_eq!(commands.len(), 2, "expected undo of B then redo of A");
    for cmd in &commands {
        cmd.apply(&mut doc, &mut cursor)
            .expect("apply jump command");
    }
    assert_eq!(doc.text(), "A");
    assert!(tree.can_undo());
    assert!(!tree.can_redo(), "node 1 is a leaf");
    assert_eq!(tree.branch_count(), 0);

    // Jump back across to node 2 and verify the document follows.
    let commands = tree
        .jump_to_node(UndoNodeId::new(2))
        .expect("jump back to other branch");
    assert_eq!(commands.len(), 2, "expected undo of A then redo of B");
    for cmd in &commands {
        cmd.apply(&mut doc, &mut cursor)
            .expect("apply jump command");
    }
    assert_eq!(doc.text(), "B");

    // Jumping to a nonexistent node must fail without moving.
    assert!(tree.jump_to_node(UndoNodeId::new(99)).is_none());
    assert_eq!(doc.text(), "B");
    assert!(tree.can_undo());
}

/// Regression: `redo` must return the work that was just undone, not a
/// branch abandoned earlier.
///
/// Previously `redo` was hardcoded to child index 0 while `push` appended
/// new branches to the end, so "undo, type something new, undo, redo"
/// silently restored the *old* text and left the newly typed branch
/// unreachable through any public API.
#[test]
fn redo_returns_to_newest_branch_not_oldest() {
    let mut tree = UndoTree::with_timeout(0);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    // Type "OLD".
    let cmd_old = insert_at(0, "OLD");
    cmd_old.apply(&mut doc, &mut cursor).expect("apply OLD");
    tree.push(cmd_old);
    assert_eq!(doc.text(), "OLD");

    // Undo it, then type something different: this forks the tree.
    let inv = tree.undo().expect("undo OLD");
    inv.apply(&mut doc, &mut cursor).expect("apply inverse OLD");
    assert_eq!(doc.text(), "");

    let cmd_new = insert_at(0, "NEW");
    cmd_new.apply(&mut doc, &mut cursor).expect("apply NEW");
    tree.push(cmd_new);
    assert_eq!(doc.text(), "NEW");

    // Undo the new work, landing on the branch point with both siblings.
    let inv = tree.undo().expect("undo NEW");
    inv.apply(&mut doc, &mut cursor).expect("apply inverse NEW");
    assert_eq!(doc.text(), "");
    assert_eq!(tree.branch_count(), 2, "both branches must survive");

    // Redo must restore "NEW" — the work we just undid.
    let redone = tree.redo().expect("redo after fork");
    redone.apply(&mut doc, &mut cursor).expect("apply redo");
    assert_eq!(
        doc.text(),
        "NEW",
        "redo resurrected the abandoned branch instead of the newest work"
    );

    // And the older branch is still reachable, not truncated.
    let inv = tree.undo().expect("undo back to fork");
    inv.apply(&mut doc, &mut cursor).expect("apply inverse");
    let old_branch = tree.redo_branch(0).expect("older branch still present");
    old_branch.apply(&mut doc, &mut cursor).expect("apply old");
    assert_eq!(doc.text(), "OLD");
}

/// `redo` retraces the branch the caller last travelled, even when that
/// branch is not the most recently created one.
#[test]
fn redo_retraces_the_last_travelled_branch() {
    let mut tree = UndoTree::with_timeout(0);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    // Three siblings at the root: branch 0 is "A", 1 is "B", 2 is "C".
    // The middle branch is the target deliberately — it is neither the
    // oldest child (which the pre-fix `redo` hard-coded) nor the newest
    // (which is the fallback when no branch has been travelled), so this
    // test fails against both wrong answers instead of only one.
    for text in ["A", "B", "C"] {
        let cmd = insert_at(0, text);
        cmd.apply(&mut doc, &mut cursor).expect("apply branch");
        tree.push(cmd);
        let inv = tree.undo().expect("undo branch");
        inv.apply(&mut doc, &mut cursor)
            .expect("apply inverse branch");
    }
    assert_eq!(tree.branch_count(), 3);

    // Explicitly travel into the middle branch. That becomes the active
    // path, so an undo/redo round-trip must return to it.
    let into_b = tree.redo_branch(1).expect("enter branch 1");
    into_b.apply(&mut doc, &mut cursor).expect("apply B");
    assert_eq!(doc.text(), "B");

    let inv = tree.undo().expect("undo B again");
    inv.apply(&mut doc, &mut cursor).expect("apply inverse B");
    let redone = tree.redo().expect("redo");
    redone.apply(&mut doc, &mut cursor).expect("apply redo");
    assert_eq!(
        doc.text(),
        "B",
        "redo abandoned the branch the caller was actually on"
    );

    // Every sibling survives the round-trip: nothing was pruned to make
    // the active path unambiguous.
    let inv = tree.undo().expect("undo B once more");
    inv.apply(&mut doc, &mut cursor).expect("apply inverse B");
    assert_eq!(tree.branch_count(), 3);
    for (index, expected) in [(0, "A"), (1, "B"), (2, "C")] {
        let cmd = tree.redo_branch(index).expect("enter branch");
        cmd.apply(&mut doc, &mut cursor).expect("apply branch");
        assert_eq!(doc.text(), expected, "branch {index} was not preserved");
        let inv = tree.undo().expect("leave branch");
        inv.apply(&mut doc, &mut cursor).expect("apply inverse");
    }
}

/// Regression: a jump must record the branch it travelled, so a later
/// `redo` retraces the jump rather than re-entering an earlier sibling.
///
/// Found by adversarial review of the original `preferred_child` fix,
/// which updated `push`, `undo` and `redo_branch` but omitted
/// `jump_to_node`. The counterexample: fork "A" and "B" at the root (the
/// second push leaves the root preferring B), jump B → A, then jump
/// A → root. Neither jump recorded the root→A edge, so `redo` still chose
/// B even though A was the branch just departed.
#[test]
fn jump_records_the_branch_it_travelled() {
    let mut tree = UndoTree::with_timeout(0);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    let apply = |cmds: &[Command], doc: &mut Document, cursor: &mut CursorState| {
        for cmd in cmds {
            cmd.apply(doc, cursor).expect("apply jump command");
        }
    };

    // Fork at the root: node 1 is "A", node 2 is "B".
    let cmd_a = insert_at(0, "A");
    cmd_a.apply(&mut doc, &mut cursor).expect("apply A");
    tree.push(cmd_a);
    let inv = tree.undo().expect("undo A");
    inv.apply(&mut doc, &mut cursor).expect("apply inverse A");

    let cmd_b = insert_at(0, "B");
    cmd_b.apply(&mut doc, &mut cursor).expect("apply B");
    tree.push(cmd_b);
    assert_eq!(doc.text(), "B");
    assert_eq!(tree.branch_count(), 0, "sitting on leaf B");

    // Jump across to the sibling branch A, applying what we are given.
    let cmds = tree
        .jump_to_node(UndoNodeId::new(1))
        .expect("jump to branch A");
    apply(&cmds, &mut doc, &mut cursor);
    assert_eq!(doc.text(), "A");

    // Jump up to the root. A is now the branch we just departed.
    let cmds = tree.jump_to_node(UndoNodeId::new(0)).expect("jump to root");
    apply(&cmds, &mut doc, &mut cursor);
    assert_eq!(doc.text(), "");
    assert_eq!(tree.branch_count(), 2, "both branches still present");

    // Redo must return to A, not resurrect B.
    let redone = tree.redo().expect("redo after jumps");
    redone.apply(&mut doc, &mut cursor).expect("apply redo");
    assert_eq!(
        doc.text(),
        "A",
        "redo re-entered the branch abandoned earlier instead of the one \
         the jump travelled"
    );
}

/// Linear redo is unaffected: with a single child there is no choice to
/// make, and repeated undo/redo must round-trip exactly.
#[test]
fn redo_round_trips_on_a_linear_history() {
    let mut tree = UndoTree::with_timeout(0);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    for (column, text) in [(0, "a"), (1, "b"), (2, "c")] {
        let cmd = insert_at(column, text);
        cmd.apply(&mut doc, &mut cursor).expect("apply");
        tree.push(cmd);
    }
    assert_eq!(doc.text(), "abc");

    for _ in 0..3 {
        let inv = tree.undo().expect("undo");
        inv.apply(&mut doc, &mut cursor).expect("apply inverse");
    }
    assert_eq!(doc.text(), "");
    assert!(!tree.can_undo());

    for _ in 0..3 {
        let cmd = tree.redo().expect("redo");
        cmd.apply(&mut doc, &mut cursor).expect("apply redo");
    }
    assert_eq!(doc.text(), "abc");
    assert!(!tree.can_redo());
}
