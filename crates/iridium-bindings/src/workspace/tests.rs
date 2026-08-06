//! Boundary tests: what the tag protects, and what the snapshot promises.
//!
//! The interesting cases here are all *silent* ones — a swapped id that
//! resolves to a real but wrong node, a depth-first walk that emits siblings
//! backwards, a `revision` read off the wrong editor. None of them throws.
//! Each looks right in the smallest workspace and wrong in a real one, which
//! is exactly the shape a test has to be built for deliberately.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use iridium_editor::EditorConfig;
use iridium_editor::commands::builtin::{PALETTE_OPEN, WORKSPACE_NEXT_TAB};
use iridium_editor::theme::Theme;
use iridium_editor::workspace::Workspace;

use super::{
    NodeKind, WorkspaceActionError, activate, close, create_group, decode_document, decode_node,
    encode_document, encode_node, handles_command, label_of, move_node, open, rename, resolve_node,
    run_command, snapshot,
};

fn workspace() -> Workspace {
    Workspace::new(EditorConfig::default(), Theme::default())
}

/// A workspace shaped `work/{a.rs, b.rs}, c.rs` — nested, and with a tab
/// after the group so that crossing a boundary is exercised.
fn nested() -> (Workspace, String) {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("top level");
    workspace.open("a", "a.rs", Some(group)).expect("in group");
    workspace.open("b", "b.rs", Some(group)).expect("in group");
    workspace.open("c", "c.rs", None).expect("top level");
    (workspace, encode_node(group))
}

// ===== The tag =====

#[test]
fn a_document_id_is_refused_where_a_node_id_is_expected() {
    // The failure this exists to prevent: both counters start at the same
    // place, so document 0 and node 0 both exist. Untagged, a host that
    // swapped them would activate a real but arbitrary tab, with no error
    // anywhere. Tagged, it cannot parse.
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    let document = workspace.active_document().expect("a tab is active");

    let node_wire = encode_node(tab);
    let document_wire = encode_document(document);

    assert!(decode_node(&node_wire).is_some());
    assert!(
        decode_node(&document_wire).is_none(),
        "a document id must not parse as a node id"
    );
    assert!(
        decode_document(&node_wire).is_none(),
        "a node id must not parse as a document id"
    );
}

#[test]
fn the_two_spaces_really_do_collide_without_the_tag() {
    // The control for the test above. If the raw counters never coincided,
    // the tag would be guarding nothing and the previous test would pass
    // for the wrong reason.
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    let document = workspace.active_document().expect("a tab is active");

    assert_eq!(
        tab.as_raw(),
        document.as_raw(),
        "the first node and the first document share a raw value, which is \
         precisely why the wire format has to distinguish them"
    );
}

#[test]
fn a_malformed_id_is_reported_as_malformed_and_a_stale_one_as_stale() {
    // Two different problems: the first is a bug in the host, the second is
    // an ordinary race between the snapshot it drew and the click it sent.
    // Collapsing them would tell a developer to go looking in the wrong
    // place.
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("top level");
    let wire = encode_node(tab);
    assert!(workspace.close(tab));

    assert!(matches!(
        resolve_node(&workspace, "7"),
        Err(WorkspaceActionError::MalformedNodeId(_))
    ));
    assert!(matches!(
        resolve_node(&workspace, "n:notanumber"),
        Err(WorkspaceActionError::MalformedNodeId(_))
    ));
    assert!(
        matches!(
            resolve_node(&workspace, "n:-1"),
            Err(WorkspaceActionError::MalformedNodeId(_))
        ),
        "a negative index must not wrap into a huge id"
    );
    assert!(
        matches!(
            resolve_node(&workspace, &wire),
            Err(WorkspaceActionError::UnknownNode(_))
        ),
        "a well-formed id for a closed node is stale, not malformed"
    );
}

#[test]
fn an_id_past_the_range_of_u64_is_refused_rather_than_saturating() {
    let workspace = workspace();
    assert!(matches!(
        resolve_node(&workspace, "n:99999999999999999999999"),
        Err(WorkspaceActionError::MalformedNodeId(_))
    ));
}

// ===== The snapshot =====

#[test]
fn the_snapshot_walks_depth_first_in_display_order() {
    // The bug this catches: a stack-based traversal that forgets to reverse
    // its children emits every group's contents right-to-left. In a group of
    // one — which is what a hand-written smoke test tends to build — that is
    // indistinguishable from correct.
    let (workspace, _) = nested();
    let snapshot = snapshot(&workspace);

    let labels: Vec<&str> = snapshot
        .nodes
        .iter()
        .map(|node| node.label.as_str())
        .collect();
    assert_eq!(labels, ["work", "a.rs", "b.rs", "c.rs"]);
}

#[test]
fn depth_counts_from_zero_at_the_top_level() {
    let (workspace, _) = nested();
    let snapshot = snapshot(&workspace);

    let depths: Vec<u32> = snapshot.nodes.iter().map(|node| node.depth).collect();
    assert_eq!(depths, [0, 1, 1, 0]);
}

#[test]
fn a_group_is_a_group_and_a_tab_carries_its_document() {
    let (workspace, group) = nested();
    let snapshot = snapshot(&workspace);

    let group_node = snapshot
        .nodes
        .iter()
        .find(|node| node.id == group)
        .expect("the group is in the snapshot");
    assert_eq!(group_node.kind, NodeKind::Group);
    assert_eq!(group_node.document_id, None);
    assert_eq!(group_node.children.len(), 2);
    assert_eq!(group_node.revision, None);

    let tab = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "a.rs")
        .expect("a.rs is in the snapshot");
    assert_eq!(tab.kind, NodeKind::Tab);
    assert!(tab.document_id.is_some());
    assert_eq!(tab.parent_id.as_deref(), Some(group.as_str()));
    assert!(tab.children.is_empty());
    assert!(tab.revision.is_some());
}

#[test]
fn the_tab_order_crosses_group_boundaries_exactly_as_the_keys_do() {
    // The strip and `workspace.nextTab` must agree. They do here by
    // construction — both read `Workspace::tabs` — and this asserts that the
    // boundary did not quietly substitute its own flattening.
    let (mut workspace, _) = nested();
    let snapshot = snapshot(&workspace);
    assert_eq!(snapshot.tab_order.len(), 3);
    assert_eq!(snapshot.roots.len(), 2, "the group and c.rs");

    let mut walked = vec![snapshot.active_id.clone().expect("a.rs is active")];
    while workspace.next_tab() {
        walked.push(encode_node(workspace.active().expect("still active")));
    }
    assert_eq!(walked, snapshot.tab_order);
}

#[test]
fn one_document_in_two_tabs_reports_two_tabs_and_one_document() {
    // The whole reason the two id spaces exist. A snapshot that deduplicated
    // by document id would draw one tab where the user opened two.
    let mut workspace = workspace();
    workspace.open("shared", "a.rs", None).expect("top level");
    let document = workspace.active_document().expect("a tab is active");
    workspace
        .open_existing(document, "a.rs (2)", None)
        .expect("the document is open");

    let snapshot = snapshot(&workspace);
    assert_eq!(snapshot.tab_order.len(), 2);
    assert_eq!(snapshot.document_count, 1);

    let documents: Vec<&String> = snapshot
        .nodes
        .iter()
        .filter_map(|node| node.document_id.as_ref())
        .collect();
    assert_eq!(documents.len(), 2);
    assert_eq!(documents[0], documents[1], "one buffer, two places");
}

#[test]
fn the_revision_on_a_tab_is_that_tab_s_own_document() {
    // A snapshot that read the *active* editor's revision for every row
    // would pass every test built on one document.
    let mut workspace = workspace();
    let first = workspace.open("a", "a.rs", None).expect("top level");
    workspace.open("b", "b.rs", None).expect("top level");

    let document = workspace
        .node(first)
        .and_then(iridium_editor::workspace::Node::document)
        .expect("a tab has a document");
    let editor = workspace.editor_mut(document).expect("open");
    editor.set_content("a edited into a different revision");

    let snapshot = snapshot(&workspace);
    let a = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "a.rs")
        .expect("present");
    let b = snapshot
        .nodes
        .iter()
        .find(|node| node.label == "b.rs")
        .expect("present");
    assert_ne!(
        a.revision, b.revision,
        "each tab must report its own document's revision, not the active one's"
    );
}

#[test]
fn an_empty_workspace_snapshots_to_an_empty_but_well_formed_payload() {
    let snapshot = snapshot(&workspace());

    assert!(snapshot.nodes.is_empty());
    assert!(snapshot.roots.is_empty());
    assert!(snapshot.tab_order.is_empty());
    assert_eq!(snapshot.active_id, None);
    assert_eq!(snapshot.active_document_id, None);
    assert_eq!(snapshot.document_count, 0);
}

#[test]
fn a_deeply_nested_workspace_serializes_without_overflowing_the_stack() {
    // Nesting is user-controlled. A recursive walk would take the face down
    // with it; the iterative one must not.
    let mut workspace = workspace();
    let mut parent = None;
    for depth in 0..10_000 {
        parent = workspace.create_group(format!("g{depth}"), parent);
        assert!(parent.is_some(), "group {depth} was refused");
    }
    workspace.open("leaf", "leaf.rs", parent).expect("in group");

    let snapshot = snapshot(&workspace);
    assert_eq!(snapshot.nodes.len(), 10_001);
    assert_eq!(
        snapshot.nodes.last().expect("the leaf").depth,
        10_000,
        "depth must keep counting rather than wrapping or clamping"
    );
}

#[test]
fn the_snapshot_round_trips_through_json_unchanged() {
    let (workspace, _) = nested();
    let snapshot = snapshot(&workspace);

    let json = serde_json::to_string(&snapshot).expect("serializable");
    let back: super::WorkspaceSnapshot = serde_json::from_str(&json).expect("deserializable");
    assert_eq!(back, snapshot);

    assert!(
        json.contains("\"tabOrder\""),
        "the wire is camelCase: {json}"
    );
    assert!(json.contains("\"kind\":\"tab\""), "{json}");
}

// ===== The actions =====

#[test]
fn activating_a_group_is_declined_rather_than_erroring() {
    // A sidebar click lands on whatever row was clicked. A group row is a
    // perfectly ordinary thing to click and must not produce an error the
    // face has to filter out.
    let (mut workspace, group) = nested();

    assert_eq!(activate(&mut workspace, &group), Ok(false));
    assert_eq!(
        label_of(&workspace, &snapshot(&workspace).active_id.expect("active")),
        Some("a.rs".to_owned()),
        "the activation must not have moved"
    );
}

#[test]
fn activating_the_already_active_tab_reports_no_movement() {
    let (mut workspace, _) = nested();
    let active = snapshot(&workspace).active_id.expect("active");

    assert_eq!(activate(&mut workspace, &active), Ok(false));
}

#[test]
fn closing_a_group_through_the_boundary_takes_its_whole_subtree() {
    let (mut workspace, group) = nested();

    assert_eq!(close(&mut workspace, &group), Ok(true));

    let snapshot = snapshot(&workspace);
    assert_eq!(snapshot.nodes.len(), 1);
    assert_eq!(snapshot.tab_order.len(), 1);
    assert_eq!(snapshot.document_count, 1, "a.rs and b.rs went with it");
}

#[test]
fn renaming_reaches_both_kinds() {
    let (mut workspace, group) = nested();
    let tab = snapshot(&workspace).tab_order[0].clone();

    assert_eq!(rename(&mut workspace, &group, "later"), Ok(true));
    assert_eq!(rename(&mut workspace, &tab, "renamed.rs"), Ok(true));

    assert_eq!(label_of(&workspace, &group), Some("later".to_owned()));
    assert_eq!(label_of(&workspace, &tab), Some("renamed.rs".to_owned()));
}

#[test]
fn dropping_a_node_onto_a_tab_says_it_is_a_tab() {
    // Drag-and-drop makes this reachable by accident constantly. "Unknown
    // node" would send a developer looking for a stale id that is in fact
    // perfectly live.
    let (mut workspace, _) = nested();
    let tabs = snapshot(&workspace).tab_order;

    assert!(matches!(
        move_node(&mut workspace, &tabs[2], Some(&tabs[0]), 0),
        Err(WorkspaceActionError::NotAGroup(_))
    ));
    assert!(matches!(
        create_group(&mut workspace, "nope", Some(&tabs[0])),
        Err(WorkspaceActionError::NotAGroup(_))
    ));
    assert!(matches!(
        open(&mut workspace, "x", "x.rs", Some(&tabs[0])),
        Err(WorkspaceActionError::NotAGroup(_))
    ));
}

#[test]
fn a_move_index_past_the_end_appends_rather_than_failing() {
    let (mut workspace, group) = nested();
    let tabs = snapshot(&workspace).tab_order;

    assert_eq!(
        move_node(&mut workspace, &tabs[2], Some(&group), u32::MAX),
        Ok(true)
    );

    let snapshot = snapshot(&workspace);
    let labels: Vec<&str> = snapshot
        .nodes
        .iter()
        .map(|node| node.label.as_str())
        .collect();
    assert_eq!(labels, ["work", "a.rs", "b.rs", "c.rs"]);
    assert_eq!(snapshot.roots.len(), 1, "c.rs moved inside the group");
}

#[test]
fn moving_to_the_top_level_takes_no_parent() {
    let (mut workspace, _) = nested();
    let tabs = snapshot(&workspace).tab_order;

    assert_eq!(move_node(&mut workspace, &tabs[0], None, 0), Ok(true));

    let snapshot = snapshot(&workspace);
    assert_eq!(snapshot.roots.len(), 3);
    assert_eq!(snapshot.nodes[0].label, "a.rs");
    assert_eq!(snapshot.nodes[0].depth, 0);
}

#[test]
fn opening_into_a_group_lands_in_that_group() {
    let (mut workspace, group) = nested();

    let id = open(&mut workspace, "d", "d.rs", Some(&group)).expect("the group accepts tabs");

    let snapshot = snapshot(&workspace);
    let node = snapshot
        .nodes
        .iter()
        .find(|node| node.id == id)
        .expect("present");
    assert_eq!(node.parent_id.as_deref(), Some(group.as_str()));
    assert_eq!(node.depth, 1);
}

// ===== Command routing =====

#[test]
fn a_workspace_command_runs_and_a_foreign_one_is_reported() {
    let (mut workspace, _) = nested();

    assert!(handles_command(&workspace, WORKSPACE_NEXT_TAB.as_str()));
    assert_eq!(
        run_command(&mut workspace, WORKSPACE_NEXT_TAB.as_str()),
        Ok(true)
    );

    assert!(!handles_command(&workspace, PALETTE_OPEN.as_str()));
    assert!(matches!(
        run_command(&mut workspace, PALETTE_OPEN.as_str()),
        Err(WorkspaceActionError::NotAWorkspaceCommand(_))
    ));
}

#[test]
fn reaching_the_end_of_the_strip_is_ok_false_not_an_error() {
    let (mut workspace, _) = nested();
    while run_command(&mut workspace, WORKSPACE_NEXT_TAB.as_str()) == Ok(true) {}

    assert_eq!(
        run_command(&mut workspace, WORKSPACE_NEXT_TAB.as_str()),
        Ok(false),
        "a face must be able to tell 'at the end' from 'went wrong'"
    );
}

#[test]
fn every_error_prints_the_offending_text() {
    // An error a face logs without the id in it is an error nobody can act
    // on.
    let cases = [
        WorkspaceActionError::MalformedNodeId("7".to_owned()),
        WorkspaceActionError::MalformedDocumentId("7".to_owned()),
        WorkspaceActionError::UnknownNode("n:7".to_owned()),
        WorkspaceActionError::NotAGroup("n:7".to_owned()),
        WorkspaceActionError::NotAWorkspaceCommand("palette.open".to_owned()),
    ];
    for case in &cases {
        let text = case.to_string();
        assert!(
            text.contains('7') || text.contains("palette.open"),
            "`{case:?}` printed as `{text}` without naming what went wrong"
        );
    }
}
