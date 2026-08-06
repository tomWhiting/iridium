//! The workspace projected through [`iridium_tree::Tree`].
//!
//! Separate from `workspace_tests` because these exercise a different
//! contract: not what the workspace does, but what the tree crate sees when
//! it asks.

use iridium_tree::{Tree, TreeSource};

use super::NodeId;
use super::workspace_tests::workspace;

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
