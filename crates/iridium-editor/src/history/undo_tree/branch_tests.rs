//! Tests for branch selection and whole-tree description.
//!
//! [`UndoTree::undo`], [`UndoTree::redo`] and [`UndoTree::jump_to_node`] move
//! the tree pointer, and are covered next door in [`super::tests`]. The three
//! queries here move nothing: [`UndoTree::active_branch_index`] reports which
//! fork redo would take, [`UndoTree::cycle_branch`] changes that answer, and
//! [`UndoTree::snapshot`] describes the whole shape for a panel to draw. They
//! are separated because their failure mode is different — a wrong answer here
//! never corrupts a document, it silently sends the *next* redo somewhere the
//! user did not ask for.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::document::{CursorState, Document, Position};

/// Builds an insert command at column 0 of line 0.
fn insert(text: &str) -> Command {
    Command::Insert {
        position: Position::new(0, 0),
        text: text.to_string(),
    }
}

/// A tree sitting at the root with one child branch per entry of `texts`, in
/// the order given, plus the empty document those branches were built against.
///
/// Each branch is created by applying an insert, pushing it, and undoing back
/// to the root — which is exactly how a user forks the history — so the
/// resulting `preferred_child` is the *last* branch created, the one they just
/// departed.
fn fork(texts: &[&str]) -> (UndoTree, Document, CursorState) {
    let mut tree = UndoTree::with_timeout(0);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    for text in texts {
        let cmd = insert(text);
        cmd.apply(&mut doc, &mut cursor).expect("apply branch");
        tree.push(cmd);
        let inv = tree.undo().expect("undo branch");
        inv.apply(&mut doc, &mut cursor).expect("apply inverse");
    }

    assert_eq!(doc.text(), "", "the fork point is the empty document");
    assert_eq!(tree.branch_count(), texts.len());
    (tree, doc, cursor)
}

/// The contract [`UndoTree::active_branch_index`] exists to keep: the index it
/// reports is the branch a plain [`UndoTree::redo`] actually takes.
///
/// This is the whole point of the method. A panel rendering "branch 2 of 3"
/// from one rule while `redo` picks by another is a lie the user only discovers
/// after pressing the key, and the two rules are not obviously the same — redo
/// resolves `preferred_child` with a fallback, so returning the raw stored
/// index, a constant `0`, or "the newest child" all look plausible and are all
/// wrong for at least one branch. Asserting against `redo` itself, for every
/// branch, is the only formulation that cannot drift.
#[test]
fn active_branch_index_names_the_branch_redo_would_take() {
    let (mut tree, mut doc, mut cursor) = fork(&["A", "B", "C"]);

    for (index, expected) in [(0, "A"), (1, "B"), (2, "C")] {
        // Travel into the branch explicitly and come straight back out, which
        // is what makes it the active path.
        let cmd = tree.redo_branch(index).expect("enter branch");
        cmd.apply(&mut doc, &mut cursor).expect("apply branch");
        assert_eq!(doc.text(), expected);

        assert_eq!(
            tree.active_branch_index(),
            None,
            "branch {index} is a leaf, so nothing is active from here"
        );

        let inv = tree.undo().expect("leave branch");
        inv.apply(&mut doc, &mut cursor).expect("apply inverse");

        assert_eq!(
            tree.active_branch_index(),
            Some(index),
            "the branch just departed is the active one"
        );

        // The claim under test: redo goes exactly where the index says.
        let redone = tree.redo().expect("redo");
        redone.apply(&mut doc, &mut cursor).expect("apply redo");
        assert_eq!(
            doc.text(),
            expected,
            "active_branch_index reported {index} but redo went elsewhere"
        );

        let inv = tree.undo().expect("leave branch again");
        inv.apply(&mut doc, &mut cursor).expect("apply inverse");
    }

    // A tree that has never been undone has no branches at all, and so no
    // active one — distinct from "branch 0", which is what an implementation
    // reaching for a default would report.
    let mut linear = UndoTree::with_timeout(0);
    assert_eq!(
        linear.active_branch_index(),
        None,
        "the empty tree is a leaf"
    );
    linear.push(insert("only"));
    assert_eq!(linear.active_branch_index(), None, "a leaf has no branch");
}

/// Cycling walks the branches in order and wraps at both ends.
///
/// Wrapping is deliberate and is asserted in both directions: the branches are
/// a ring of alternatives with no natural first or last, and nothing moves when
/// the cycle runs, so there is no overshoot to protect against.
#[test]
fn cycle_branch_wraps_in_both_directions() {
    let (mut tree, _doc, _cursor) = fork(&["A", "B", "C"]);

    // The last branch created is the one just departed, so it starts active.
    assert_eq!(tree.active_branch_index(), Some(2));

    // Forward from the last wraps to the first.
    assert_eq!(tree.cycle_branch(true), Some(0));
    assert_eq!(tree.cycle_branch(true), Some(1));
    assert_eq!(tree.cycle_branch(true), Some(2));
    assert_eq!(tree.cycle_branch(true), Some(0), "forward did not wrap");

    // Backward from the first wraps to the last.
    assert_eq!(tree.cycle_branch(false), Some(2), "backward did not wrap");
    assert_eq!(tree.cycle_branch(false), Some(1));
    assert_eq!(tree.cycle_branch(false), Some(0));

    // Every return value was also the new active index, not the old one.
    assert_eq!(tree.active_branch_index(), Some(0));
}

/// Cycling changes only which fork redo would take: no command is produced, no
/// node is entered, and the tree keeps its shape.
///
/// This is what makes the verb safe to hold down while looking at a panel, and
/// it is the property an implementation that "cycled" by calling `redo_branch`
/// would break — it would look right in an index assertion and quietly move the
/// document.
#[test]
fn cycling_moves_nothing_but_changes_what_redo_takes() {
    let (mut tree, mut doc, mut cursor) = fork(&["A", "B", "C"]);

    let before = tree.current_node_id();
    let node_count = tree.get_tree_info().node_count;

    for _ in 0..5 {
        tree.cycle_branch(true);
        assert_eq!(tree.current_node_id(), before, "cycling moved the pointer");
        assert_eq!(doc.text(), "", "cycling edited the document");
        assert_eq!(
            tree.get_tree_info().node_count,
            node_count,
            "cycling changed the tree's shape"
        );
        assert!(tree.can_redo(), "the branches are still there to redo into");
    }

    // Land on the oldest branch — the one a plain redo would never have taken,
    // since it was abandoned first — and confirm redo now takes it.
    //
    // Bounded by the branch count rather than looped until the index matches:
    // a cycle that cannot reach every branch must fail this test, not hang in
    // it.
    for _ in 0..3 {
        if tree.active_branch_index() == Some(0) {
            break;
        }
        tree.cycle_branch(true);
    }
    assert_eq!(
        tree.active_branch_index(),
        Some(0),
        "cycling never reached the oldest branch"
    );
    let redone = tree.redo().expect("redo after cycling");
    redone.apply(&mut doc, &mut cursor).expect("apply redo");
    assert_eq!(
        doc.text(),
        "A",
        "redo ignored the branch the caller cycled to"
    );
}

/// With no choice to make, cycling reports that rather than pretending.
///
/// Both cases return `None`, and neither disturbs the active path: a host
/// binding this to a key must be able to press it against a leaf without the
/// next redo changing destination.
#[test]
fn cycle_branch_refuses_when_there_is_nothing_to_choose() {
    // A leaf: no branches at all.
    let mut leaf = UndoTree::with_timeout(0);
    leaf.push(insert("only"));
    assert_eq!(leaf.cycle_branch(true), None);
    assert_eq!(leaf.cycle_branch(false), None);
    assert_eq!(leaf.active_branch_index(), None);

    // Exactly one branch: cycling would return to where it started, which is
    // not "the next branch" and must not be reported as a change.
    let (mut single, _doc, _cursor) = fork(&["A"]);
    assert_eq!(single.active_branch_index(), Some(0));
    assert_eq!(single.cycle_branch(true), None);
    assert_eq!(single.cycle_branch(false), None);
    assert_eq!(
        single.active_branch_index(),
        Some(0),
        "a refused cycle still moved the active path"
    );
}

/// Builds a tree with a fork and a two-deep branch, and returns it.
///
/// Shape, with node ids as assigned: root `0`, whose children are `1` ("A",
/// carrying its own child `2` for "B") and `3` ("X"). The pointer ends on `3`.
/// Deliberately not a flat fork — a snapshot that only ever sees depth 1 cannot
/// show that parent links are reported per node rather than assumed.
fn forked_and_deep() -> UndoTree {
    let mut tree = UndoTree::with_timeout(0);
    let mut doc = Document::new("");
    let mut cursor = CursorState::at(Position::zero());

    for text in ["A", "B"] {
        let cmd = insert(text);
        cmd.apply(&mut doc, &mut cursor).expect("apply");
        tree.push(cmd);
    }
    for _ in 0..2 {
        let inv = tree.undo().expect("undo");
        inv.apply(&mut doc, &mut cursor).expect("apply inverse");
    }
    let cmd = insert("X");
    cmd.apply(&mut doc, &mut cursor).expect("apply X");
    tree.push(cmd);

    tree
}

/// The snapshot describes every node exactly once, in chronological order, and
/// its links are internally consistent.
///
/// A panel draws depth by following `parent_id` and the active path by
/// following `preferred_child_id`, so a snapshot that omits a node, repeats
/// one, or reports a link only one of the two ends agrees with produces a
/// drawing that cannot be reconciled with the tree the user is navigating.
#[test]
fn snapshot_describes_every_node_once_with_consistent_links() {
    let tree = forked_and_deep();
    let snapshot = tree.snapshot();

    assert_eq!(
        snapshot.nodes.len(),
        snapshot.info.node_count,
        "the summary and the node list disagree about how big the tree is"
    );
    assert_eq!(snapshot.nodes.len(), 4, "root, A, B and X");

    // Ids are unique and ascending, which is assignment order and therefore
    // chronological: a consumer ignoring the links still gets a usable list.
    let ids: Vec<u64> = snapshot
        .nodes
        .iter()
        .map(|node| node.id.parse::<u64>().expect("ids are decimal strings"))
        .collect();
    assert_eq!(
        ids,
        vec![0, 1, 2, 3],
        "nodes are not in chronological order"
    );

    // Exactly one node is current, and it is the one the summary names.
    let current: Vec<&str> = snapshot
        .nodes
        .iter()
        .filter(|node| node.is_current)
        .map(|node| node.id.as_str())
        .collect();
    assert_eq!(current, vec![snapshot.info.current_id.as_str()]);

    // Exactly one node is the root, and it is the one the summary names.
    let roots: Vec<&str> = snapshot
        .nodes
        .iter()
        .filter(|node| node.parent_id.is_none())
        .map(|node| node.id.as_str())
        .collect();
    assert_eq!(roots, vec![snapshot.info.root_id.as_str()]);

    // Every link is agreed by both ends: a child names this node as its
    // parent, and a preferred child is a child.
    for node in &snapshot.nodes {
        for child_id in &node.child_ids {
            let child = snapshot
                .nodes
                .iter()
                .find(|candidate| candidate.id == *child_id)
                .expect("a child id must name a node in the same snapshot");
            assert_eq!(
                child.parent_id.as_ref(),
                Some(&node.id),
                "node {} claims child {} which does not claim it back",
                node.id,
                child_id
            );
        }
        if let Some(preferred) = &node.preferred_child_id {
            assert!(
                node.child_ids.contains(preferred),
                "node {} prefers {preferred}, which is not one of its children",
                node.id
            );
        }
    }

    // The shape is the one that was built, including the depth-2 branch.
    let by_id = |id: &str| {
        snapshot
            .nodes
            .iter()
            .find(|node| node.id == id)
            .expect("node exists")
    };
    assert_eq!(by_id("0").child_ids, vec!["1".to_string(), "3".to_string()]);
    assert_eq!(by_id("1").child_ids, vec!["2".to_string()]);
    assert!(by_id("2").child_ids.is_empty());
    assert!(by_id("3").child_ids.is_empty());
    assert_eq!(by_id("3").parent_id.as_deref(), Some("0"));
}

/// The snapshot is the same answer the per-node queries give, so a panel taking
/// one call cannot disagree with a host taking many.
///
/// That equivalence is the entire justification for the method existing: it
/// exists to save N round trips across the wasm boundary, not to report
/// anything the tree would not report node by node.
#[test]
fn snapshot_agrees_with_the_per_node_queries() {
    let tree = forked_and_deep();
    let snapshot = tree.snapshot();

    for node in &snapshot.nodes {
        let raw = node.id.parse::<u64>().expect("ids are decimal strings");
        let direct = tree
            .node_info(UndoNodeId::from_u64(raw))
            .expect("the snapshot named a node the tree does not have");
        assert_eq!(*node, direct, "snapshot and node_info disagree about {raw}");
    }

    assert_eq!(snapshot.info.can_undo, tree.can_undo());
    assert_eq!(snapshot.info.can_redo, tree.can_redo());
    assert_eq!(snapshot.info.branch_count, tree.branch_count());
    assert_eq!(
        snapshot.info.current_id,
        tree.current_node_id().as_u64().to_string()
    );
    assert_eq!(
        snapshot.info.root_id,
        tree.root_node_id().as_u64().to_string()
    );
}

/// The snapshot tracks the pointer: it is taken fresh each time, never cached.
#[test]
fn snapshot_follows_the_current_position() {
    let mut tree = forked_and_deep();

    let before = tree.snapshot();
    assert_eq!(before.info.current_id, "3");
    assert!(!before.info.can_redo, "X is a leaf");

    tree.undo().expect("undo X");

    let after = tree.snapshot();
    assert_eq!(
        after.info.current_id, "0",
        "the snapshot did not follow undo"
    );
    assert_eq!(after.info.branch_count, 2);
    assert!(after.info.can_redo);

    // The node list moved too, not just the summary: a panel highlights the
    // current node from the node it drew, so a list that still marks the old
    // position would draw the highlight in the wrong place however correct the
    // summary is.
    let marked: Vec<&str> = after
        .nodes
        .iter()
        .filter(|node| node.is_current)
        .map(|node| node.id.as_str())
        .collect();
    assert_eq!(marked, vec!["0"], "the node list did not follow undo");
    assert_eq!(
        after.nodes.len(),
        before.nodes.len(),
        "undo must not prune the tree"
    );
}

/// The JSON a host actually reads, pinned key by key.
///
/// Nothing in Rust would notice this changing — every caller here goes through
/// the struct — but the undo-tree panel reads the JSON, and it is the only
/// consumer of these types that exists. Two properties are load-bearing and
/// neither is visible from the Rust side: **camelCase keys**, matching the
/// palette's wire shape rather than the Rust field names, and **ids as decimal
/// strings**, so a 64-bit identifier survives a language whose numbers are
/// doubles.
#[test]
fn the_snapshot_serializes_in_the_shape_a_host_reads() {
    let tree = forked_and_deep();
    let json = serde_json::to_value(tree.snapshot()).expect("a snapshot must serialize");

    let info = json.get("info").expect("info is present");
    for key in [
        "currentId",
        "rootId",
        "nodeCount",
        "canUndo",
        "canRedo",
        "branchCount",
    ] {
        assert!(info.get(key).is_some(), "info is missing {key}");
    }
    assert!(
        info.get("current_id").is_none(),
        "snake_case leaked into the wire shape"
    );
    assert!(
        info.get("currentId")
            .and_then(serde_json::Value::as_str)
            .is_some(),
        "ids must cross as strings, not as numbers a double cannot hold"
    );

    let nodes = json
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .expect("nodes is an array");
    assert_eq!(nodes.len(), 4);
    for node in nodes {
        for key in [
            "id",
            "parentId",
            "childIds",
            "preferredChildId",
            "elapsedMs",
            "description",
            "isCurrent",
        ] {
            assert!(node.get(key).is_some(), "a node is missing {key}");
        }
        assert!(
            node.get("child_ids").is_none(),
            "snake_case leaked into the wire shape"
        );
        assert!(
            node.get("id").and_then(serde_json::Value::as_str).is_some(),
            "ids must cross as strings"
        );
        for child in node
            .get("childIds")
            .and_then(serde_json::Value::as_array)
            .expect("childIds is an array")
        {
            assert!(child.is_string(), "child ids must cross as strings too");
        }
    }

    // The whole shape round-trips, so a host that stores one can hand it back.
    let restored: UndoTreeSnapshot =
        serde_json::from_value(json).expect("the wire shape must deserialize");
    assert_eq!(restored.nodes, tree.snapshot().nodes);
}
