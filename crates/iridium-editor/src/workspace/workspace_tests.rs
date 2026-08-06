//! Tests for the workspace: tabs, nested groups, and what survives a close.

use iridium_tree::{Tree, TreeSource};

use super::{Node, NodeId, Workspace};
use crate::editor::{Editor, EditorConfig};
use crate::theme::Theme;

fn workspace() -> Workspace {
    Workspace::new(EditorConfig::default(), Theme::default())
}

/// The labels of every tab, in display order.
fn tab_labels(workspace: &Workspace) -> Vec<String> {
    workspace
        .tabs()
        .into_iter()
        .filter_map(|id| workspace.node(id))
        .map(|node| node.label().to_owned())
        .collect()
}

#[test]
fn a_new_workspace_is_empty_and_every_query_is_defined_on_it() {
    let mut workspace = workspace();

    assert!(workspace.is_empty());
    assert_eq!(workspace.document_count(), 0);
    assert_eq!(workspace.tab_count(), 0);
    assert_eq!(workspace.active(), None);
    assert!(workspace.active_editor().is_none());
    assert!(workspace.active_editor_mut().is_none());
    assert!(!workspace.next_tab());
    assert!(!workspace.previous_tab());
    assert!(!workspace.first_tab());
    assert!(!workspace.last_tab());
    assert!(workspace.roots().is_empty());
}

#[test]
fn the_first_document_opened_becomes_active() {
    let mut workspace = workspace();

    let tab = workspace.open("fn main() {}", "main.rs", None);

    assert!(tab.is_some());
    assert_eq!(workspace.active(), tab);
    assert_eq!(workspace.document_count(), 1);
    assert!(workspace.active_editor().is_some());
}

#[test]
fn opening_a_second_document_does_not_steal_the_activation() {
    let mut workspace = workspace();
    let first = workspace.open("one", "one.rs", None);

    workspace.open("two", "two.rs", None);

    assert_eq!(
        workspace.active(),
        first,
        "opening in the background must not move the user"
    );
    assert_eq!(workspace.document_count(), 2);
}

#[test]
fn each_document_gets_its_own_content_and_history() {
    let mut workspace = workspace();
    let one = workspace.open("first", "one.rs", None).expect("opened");
    let two = workspace.open("second", "two.rs", None).expect("opened");

    let one_doc = workspace.node(one).and_then(Node::document).expect("a tab");
    let two_doc = workspace.node(two).and_then(Node::document).expect("a tab");

    assert_ne!(one_doc, two_doc, "two files are two documents");
    assert_eq!(
        workspace.editor(one_doc).map(Editor::content),
        Some("first".to_owned())
    );
    assert_eq!(
        workspace.editor(two_doc).map(Editor::content),
        Some("second".to_owned())
    );
}

/// The reason `DocumentId` and `NodeId` are separate types.
#[test]
fn two_tabs_can_share_one_document_and_therefore_one_buffer() {
    let mut workspace = workspace();
    let first = workspace.open("shared", "here.rs", None).expect("opened");
    let document = workspace
        .node(first)
        .and_then(Node::document)
        .expect("a tab");

    let second = workspace
        .open_existing(document, "also here.rs", None)
        .expect("opened a second view");

    assert_ne!(first, second, "two places");
    assert_eq!(
        workspace.node(second).and_then(Node::document),
        Some(document),
        "one buffer"
    );
    assert_eq!(
        workspace.document_count(),
        1,
        "a second view must not load the file twice"
    );
    assert_eq!(workspace.tab_count(), 2);
}

#[test]
fn closing_one_of_two_tabs_onto_a_document_keeps_the_buffer() {
    let mut workspace = workspace();
    let first = workspace.open("shared", "here.rs", None).expect("opened");
    let document = workspace
        .node(first)
        .and_then(Node::document)
        .expect("a tab");
    workspace
        .open_existing(document, "also", None)
        .expect("opened");

    assert!(workspace.close(first));

    assert_eq!(
        workspace.document_count(),
        1,
        "the surviving tab still needs its buffer, cursors and undo history"
    );
    assert!(workspace.editor(document).is_some());
}

#[test]
fn closing_the_last_tab_onto_a_document_drops_the_buffer() {
    let mut workspace = workspace();
    let tab = workspace
        .open("gone soon", "here.rs", None)
        .expect("opened");
    let document = workspace.node(tab).and_then(Node::document).expect("a tab");

    assert!(workspace.close(tab));

    assert_eq!(workspace.document_count(), 0);
    assert!(workspace.editor(document).is_none());
    assert_eq!(workspace.active(), None);
}

#[test]
fn an_id_is_never_reused_so_a_stale_one_resolves_to_nothing() {
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("opened");
    workspace.close(tab);

    let replacement = workspace.open("b", "b.rs", None).expect("opened");

    assert_ne!(
        tab, replacement,
        "reusing the id would make a stale reference draw the wrong document"
    );
    assert!(workspace.node(tab).is_none());
}

#[test]
fn groups_nest_and_hold_tabs_and_groups_in_any_mixture() {
    let mut workspace = workspace();
    let outer = workspace.create_group("work", None).expect("created");
    let inner = workspace
        .create_group("auth", Some(outer))
        .expect("created");
    workspace.open("a", "a.rs", Some(outer)).expect("opened");
    workspace.open("b", "b.rs", Some(inner)).expect("opened");
    workspace.open("c", "c.rs", None).expect("opened");

    assert_eq!(workspace.roots(), [outer, workspace.tabs()[2]]);
    assert_eq!(tab_labels(&workspace), ["b.rs", "a.rs", "c.rs"]);
    assert_eq!(workspace.parent_of(inner), Some(outer));
    assert_eq!(workspace.parent_of(outer), None);
}

#[test]
fn a_tab_cannot_be_a_parent() {
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("opened");

    assert!(
        workspace.open("b", "b.rs", Some(tab)).is_none(),
        "placing a node inside a tab has no meaning and must be refused, \
         not silently redirected to the root"
    );
    assert!(workspace.create_group("g", Some(tab)).is_none());
    assert_eq!(workspace.tab_count(), 1);
}

#[test]
fn an_unknown_parent_is_refused() {
    let mut workspace = workspace();
    let stale = workspace.create_group("g", None).expect("created");
    workspace.close(stale);

    assert!(workspace.open("a", "a.rs", Some(stale)).is_none());
    assert!(workspace.create_group("h", Some(stale)).is_none());
}

#[test]
fn closing_a_group_removes_its_whole_subtree() {
    let mut workspace = workspace();
    let outer = workspace.create_group("work", None).expect("created");
    let inner = workspace
        .create_group("auth", Some(outer))
        .expect("created");
    workspace.open("a", "a.rs", Some(inner)).expect("opened");
    let kept = workspace.open("b", "b.rs", None).expect("opened");

    assert!(workspace.close(outer));

    assert_eq!(workspace.roots(), [kept]);
    assert!(workspace.node(inner).is_none());
    assert_eq!(workspace.document_count(), 1, "a.rs went with its group");
}

#[test]
fn closing_the_active_tab_activates_the_one_after_it() {
    let mut workspace = workspace();
    let first = workspace.open("a", "a.rs", None).expect("opened");
    let second = workspace.open("b", "b.rs", None).expect("opened");
    workspace.open("c", "c.rs", None).expect("opened");
    assert!(workspace.activate(second));

    assert!(workspace.close(second));

    assert_eq!(
        tab_labels(&workspace),
        ["a.rs", "c.rs"],
        "sanity: b is gone"
    );
    assert_ne!(workspace.active(), Some(first), "forward, not backward");
    assert_eq!(
        workspace
            .active()
            .and_then(|id| workspace.node(id))
            .map(Node::label),
        Some("c.rs")
    );
}

#[test]
fn closing_the_last_active_tab_falls_back_to_the_one_before_it() {
    let mut workspace = workspace();
    workspace.open("a", "a.rs", None).expect("opened");
    let last = workspace.open("b", "b.rs", None).expect("opened");
    assert!(workspace.activate(last));

    assert!(workspace.close(last));

    assert_eq!(
        workspace
            .active()
            .and_then(|id| workspace.node(id))
            .map(Node::label),
        Some("a.rs"),
        "closing the end must land somewhere, not nowhere"
    );
}

#[test]
fn closing_a_group_containing_the_active_tab_moves_activation_outside_it() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    let inside = workspace.open("a", "a.rs", Some(group)).expect("opened");
    workspace.open("b", "b.rs", None).expect("opened");
    assert!(workspace.activate(inside));

    assert!(workspace.close(group));

    assert_eq!(
        workspace
            .active()
            .and_then(|id| workspace.node(id))
            .map(Node::label),
        Some("b.rs")
    );
}

#[test]
fn closing_an_inactive_tab_leaves_the_activation_alone() {
    let mut workspace = workspace();
    let active = workspace.open("a", "a.rs", None).expect("opened");
    let other = workspace.open("b", "b.rs", None).expect("opened");

    assert!(workspace.close(other));

    assert_eq!(workspace.active(), Some(active));
}

#[test]
fn a_group_cannot_be_activated() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    let tab = workspace.open("a", "a.rs", Some(group)).expect("opened");

    assert!(
        !workspace.activate(group),
        "there is no document to type into"
    );
    assert_eq!(
        workspace.active(),
        Some(tab),
        "a refused activation must not clear the good one"
    );
}

#[test]
fn tab_movement_walks_the_flattened_order_across_group_boundaries() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    workspace.open("a", "a.rs", Some(group)).expect("opened");
    workspace.open("b", "b.rs", Some(group)).expect("opened");
    workspace.open("c", "c.rs", None).expect("opened");
    assert_eq!(tab_labels(&workspace), ["a.rs", "b.rs", "c.rs"]);
    // `first_tab` reports whether it *moved*, and a.rs is already active
    // because it was opened first — so `false` here is the correct answer,
    // not a failure. Asserted on the resulting state instead.
    workspace.first_tab();
    assert_eq!(
        workspace
            .active()
            .and_then(|id| workspace.node(id))
            .map(Node::label),
        Some("a.rs")
    );

    assert!(workspace.next_tab());
    assert!(workspace.next_tab());

    assert_eq!(
        workspace
            .active()
            .and_then(|id| workspace.node(id))
            .map(Node::label),
        Some("c.rs"),
        "movement must cross out of the group, as the display order does"
    );
}

#[test]
fn tab_movement_clamps_at_both_ends_rather_than_wrapping() {
    let mut workspace = workspace();
    workspace.open("a", "a.rs", None).expect("opened");
    workspace.open("b", "b.rs", None).expect("opened");

    assert!(workspace.last_tab());
    assert!(
        !workspace.next_tab(),
        "wrapping would jump to the first tab under a held key"
    );
    assert!(workspace.first_tab());
    assert!(!workspace.previous_tab());
}

#[test]
fn moving_a_node_reparents_it_at_the_requested_index() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    workspace.open("a", "a.rs", Some(group)).expect("opened");
    let moved = workspace.open("b", "b.rs", None).expect("opened");

    assert!(workspace.move_node(moved, Some(group), 0));

    assert_eq!(workspace.parent_of(moved), Some(group));
    assert_eq!(tab_labels(&workspace), ["b.rs", "a.rs"]);
    assert_eq!(workspace.roots(), [group]);
}

#[test]
fn a_move_index_past_the_end_appends_rather_than_failing() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    workspace.open("a", "a.rs", Some(group)).expect("opened");
    let moved = workspace.open("b", "b.rs", None).expect("opened");

    assert!(workspace.move_node(moved, Some(group), usize::MAX));

    assert_eq!(tab_labels(&workspace), ["a.rs", "b.rs"]);
}

#[test]
fn moving_a_node_out_to_the_top_level_clears_its_parent() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    let tab = workspace.open("a", "a.rs", Some(group)).expect("opened");

    assert!(workspace.move_node(tab, None, 0));

    assert_eq!(workspace.parent_of(tab), None);
    assert_eq!(workspace.roots(), [tab, group]);
}

/// A cycle here is not a visible glitch — it is a traversal that never
/// terminates, in a projection that runs every frame.
#[test]
fn a_group_cannot_be_moved_into_its_own_subtree() {
    let mut workspace = workspace();
    let outer = workspace.create_group("outer", None).expect("created");
    let inner = workspace
        .create_group("inner", Some(outer))
        .expect("created");
    let deep = workspace
        .create_group("deep", Some(inner))
        .expect("created");

    assert!(!workspace.move_node(outer, Some(inner), 0));
    assert!(!workspace.move_node(outer, Some(deep), 0));
    assert!(!workspace.move_node(outer, Some(outer), 0));

    assert_eq!(workspace.parent_of(outer), None, "the refusal left it put");
    assert_eq!(workspace.roots(), [outer]);
}

/// The divergence named in `docs/WORKSPACE-DESIGN.md`: reading the active
/// editor's theme as the workspace's agrees with the truth until a document
/// is opened after a theme change.
#[test]
fn a_theme_change_reaches_background_tabs_and_documents_opened_later() {
    let mut workspace = workspace();
    let first = workspace.open("a", "a.rs", None).expect("opened");
    let background = workspace.open("b", "b.rs", None).expect("opened");
    let theme = Theme {
        name: "later".to_owned(),
        ..Theme::default()
    };

    workspace.set_theme(theme);

    let opened_after = workspace.open("c", "c.rs", None).expect("opened");
    for tab in [first, background, opened_after] {
        let document = workspace.node(tab).and_then(Node::document).expect("a tab");
        assert_eq!(
            workspace
                .editor(document)
                .map(|e| e.get_theme().name.as_str()),
            Some("later"),
            "every tab must agree with the workspace, not only the active one"
        );
    }
    assert_eq!(workspace.theme().name, "later");
}

#[test]
fn a_config_change_reaches_background_tabs_and_documents_opened_later() {
    let mut workspace = workspace();
    let first = workspace.open("a", "a.rs", None).expect("opened");
    let config = EditorConfig {
        tab_width: 7,
        ..EditorConfig::default()
    };

    workspace.set_config(config);

    let opened_after = workspace.open("b", "b.rs", None).expect("opened");
    for tab in [first, opened_after] {
        let document = workspace.node(tab).and_then(Node::document).expect("a tab");
        assert_eq!(
            workspace.editor(document).map(|e| e.get_config().tab_width),
            Some(7)
        );
    }
    assert_eq!(workspace.config().tab_width, 7);
}

#[test]
fn renaming_works_for_both_kinds_and_refuses_an_unknown_id() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    let tab = workspace.open("a", "a.rs", Some(group)).expect("opened");

    assert!(workspace.rename(group, "renamed"));
    assert!(workspace.rename(tab, "b.rs"));
    assert!(!workspace.rename(NodeId(9_999), "nothing"));

    assert_eq!(workspace.node(group).map(Node::label), Some("renamed"));
    assert_eq!(workspace.node(tab).map(Node::label), Some("b.rs"));
}

#[test]
fn closing_an_unknown_node_is_refused_rather_than_panicking() {
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("opened");

    assert!(!workspace.close(NodeId(9_999)));
    assert!(!workspace.move_node(NodeId(9_999), None, 0));
    assert!(!workspace.activate(NodeId(9_999)));

    assert_eq!(workspace.active(), Some(tab));
    assert_eq!(workspace.tab_count(), 1);
}

/// A deeply nested workspace must not overflow the stack — the traversals
/// are iterative precisely so this holds.
#[test]
fn a_very_deep_nesting_neither_overflows_nor_truncates() {
    const DEPTH: usize = 10_000;
    let mut workspace = workspace();

    let mut parent = None;
    for level in 0..DEPTH {
        parent = workspace.create_group(format!("g{level}"), parent);
        assert!(parent.is_some(), "failed to create level {level}");
    }
    workspace.open("deep", "deep.rs", parent).expect("opened");

    assert_eq!(workspace.tab_count(), 1);
    let root = workspace.roots().first().copied().expect("one root");
    assert!(workspace.close(root));
    assert!(workspace.is_empty());
    assert_eq!(workspace.document_count(), 0);
}

// =========================================================================
// The workspace as a tree source
// =========================================================================

#[test]
fn the_tree_projects_the_workspace_and_starts_closed() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    workspace.open("a", "a.rs", Some(group)).expect("opened");
    let loose = workspace.open("b", "b.rs", None).expect("opened");

    let tree = Tree::new(&mut workspace);

    assert_eq!(tree.len(), 2, "the group's contents start hidden");
    assert_eq!(tree.rows()[0].id, group);
    assert_eq!(tree.rows()[1].id, loose);
    assert!(tree.rows()[0].has_children);
    assert!(!tree.rows()[1].has_children, "a tab is a leaf");
}

#[test]
fn expanding_a_group_in_the_tree_reveals_its_children() {
    let mut workspace = workspace();
    let group = workspace.create_group("work", None).expect("created");
    let inner = workspace.open("a", "a.rs", Some(group)).expect("opened");
    let mut tree = Tree::new(&mut workspace);

    assert!(tree.expand(&mut workspace, 0));

    assert_eq!(tree.len(), 2);
    assert_eq!(tree.rows()[1].id, inner);
    assert_eq!(tree.rows()[1].depth, 1);
}

#[test]
fn an_empty_group_reports_no_children_so_it_draws_without_an_arrow() {
    let mut workspace = workspace();
    let empty = workspace.create_group("empty", None).expect("created");

    assert!(!workspace.has_children(&empty));
    assert!(workspace.children(Some(&empty)).is_empty());

    let tree = Tree::new(&mut workspace);
    assert!(
        !tree.rows()[0].has_children,
        "an arrow onto nothing invites the click again"
    );
}

#[test]
fn the_tree_source_answers_for_a_tab_and_for_an_unknown_id() {
    let mut workspace = workspace();
    let tab = workspace.open("a", "a.rs", None).expect("opened");

    assert!(workspace.children(Some(&tab)).is_empty());
    assert!(!workspace.has_children(&tab));
    assert!(workspace.children(Some(&NodeId(9_999))).is_empty());
    assert!(!workspace.has_children(&NodeId(9_999)));
}
