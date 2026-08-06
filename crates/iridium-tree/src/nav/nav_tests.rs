//! Tests for keyboard movement.

use crate::Tree;
use crate::test_source::Fixture;

/// ```text
/// src/          docs/
///   main.rs       guide.md
///   lib/
///     mod.rs
/// ```
fn sample() -> Fixture {
    Fixture::new(&[
        ("", "src"),
        ("", "docs"),
        ("src", "main.rs"),
        ("src", "lib"),
        ("lib", "mod.rs"),
        ("docs", "guide.md"),
    ])
}

#[test]
fn down_from_nothing_selects_the_first_row() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);

    assert!(tree.move_down());

    assert_eq!(tree.selected(), Some(0));
}

#[test]
fn up_from_nothing_selects_the_last_row() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);

    assert!(tree.move_up());

    assert_eq!(tree.selected(), Some(1));
}

#[test]
fn movement_clamps_at_both_ends_rather_than_wrapping() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.select(1);

    assert!(!tree.move_down(), "the last row must not wrap to the first");
    assert_eq!(tree.selected(), Some(1));

    tree.select(0);
    assert!(!tree.move_up(), "the first row must not wrap to the last");
    assert_eq!(tree.selected(), Some(0));
}

#[test]
fn down_walks_into_an_open_subtree_without_being_told_to() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(0);

    tree.move_down();

    assert_eq!(tree.selected_id(), Some(&"main.rs".to_owned()));
}

#[test]
fn right_opens_a_closed_node_and_leaves_the_selection_on_it() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.select(0);

    assert!(tree.move_right(&mut source));

    assert_eq!(
        tree.selected(),
        Some(0),
        "children appear below what revealed them"
    );
    assert_eq!(tree.len(), 4);
}

#[test]
fn right_on_an_already_open_node_steps_to_its_first_child() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(0);

    assert!(tree.move_right(&mut source));

    assert_eq!(tree.selected_id(), Some(&"main.rs".to_owned()));
}

#[test]
fn right_on_a_leaf_does_nothing_and_says_so() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(1); // main.rs, a leaf

    assert!(
        !tree.move_right(&mut source),
        "right must not become a second down"
    );

    assert_eq!(tree.selected(), Some(1));
}

#[test]
fn left_closes_an_open_node() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(0);

    assert!(tree.move_left());

    assert_eq!(tree.len(), 2);
    assert_eq!(tree.selected(), Some(0));
}

#[test]
fn left_on_a_leaf_ascends_to_its_parent() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(1); // main.rs

    assert!(tree.move_left());

    assert_eq!(tree.selected_id(), Some(&"src".to_owned()));
}

/// The behaviour that makes left-arrow a reliable way out of a deep subtree.
#[test]
fn repeated_left_alternates_closing_and_ascending_then_stops() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0); // src
    tree.expand(&mut source, 2); // lib
    tree.select(3); // mod.rs, two levels deep

    assert!(tree.move_left()); // ascend to lib
    assert_eq!(tree.selected_id(), Some(&"lib".to_owned()));
    assert!(tree.move_left()); // close lib
    assert_eq!(tree.selected_id(), Some(&"lib".to_owned()));
    assert!(tree.move_left()); // ascend to src
    assert_eq!(tree.selected_id(), Some(&"src".to_owned()));
    assert!(tree.move_left()); // close src
    assert_eq!(tree.selected_id(), Some(&"src".to_owned()));

    assert!(!tree.move_left(), "a closed root has nowhere left to go");
    assert_eq!(tree.selected(), Some(0));
}

#[test]
fn left_on_a_closed_root_does_nothing() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.select(0);

    assert!(!tree.move_left());
}

#[test]
fn collapse_parent_closes_the_containing_node_from_inside_it() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.expand(&mut source, 2);
    tree.select(3); // mod.rs, inside lib, inside src

    assert!(tree.collapse_parent());

    assert_eq!(tree.selected_id(), Some(&"lib".to_owned()));
    assert!(!tree.is_expanded(&"lib".to_owned()));
    assert_eq!(Fixture::ids(&tree), ["src", "main.rs", "lib", "docs"]);
}

#[test]
fn parent_of_finds_the_nearest_shallower_row_not_the_previous_one() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0); // src
    tree.expand(&mut source, 2); // lib
    // ["src", "main.rs", "lib", "mod.rs", "docs"]

    assert_eq!(tree.parent_of(0), None, "a root has no parent");
    assert_eq!(tree.parent_of(1), Some(0), "main.rs -> src");
    assert_eq!(
        tree.parent_of(2),
        Some(0),
        "lib -> src, skipping its sibling"
    );
    assert_eq!(tree.parent_of(3), Some(2), "mod.rs -> lib");
    assert_eq!(tree.parent_of(4), None, "docs is a root");
    assert_eq!(tree.parent_of(99), None);
}

#[test]
fn first_and_last_jump_to_the_ends() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);

    assert!(tree.move_to_last());
    assert_eq!(tree.selected(), Some(3));
    assert!(!tree.move_to_last(), "already there");

    assert!(tree.move_to_first());
    assert_eq!(tree.selected(), Some(0));
    assert!(!tree.move_to_first());
}

#[test]
fn every_movement_on_an_empty_tree_is_a_no_op() {
    let mut source = Fixture::new(&[]);
    let mut tree = Tree::new(&mut source);

    assert!(!tree.move_down());
    assert!(!tree.move_up());
    assert!(!tree.move_to_first());
    assert!(!tree.move_to_last());
    assert!(!tree.move_right(&mut source));
    assert!(!tree.move_left());
    assert!(!tree.collapse_parent());
    assert_eq!(tree.selected(), None);
}
