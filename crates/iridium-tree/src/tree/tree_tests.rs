//! Tests for expansion, the row projection, and selection tracking.

use crate::Tree;
use crate::test_source::{Fixture, Liar};

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
fn a_new_tree_shows_the_roots_closed() {
    let mut source = sample();
    let tree = Tree::new(&mut source);

    assert_eq!(Fixture::ids(&tree), ["src", "docs"]);
    assert!(tree.rows().iter().all(|row| !row.expanded));
    assert!(tree.rows().iter().all(|row| row.has_children));
    assert_eq!(tree.selected(), None);
}

#[test]
fn expanding_inserts_the_children_directly_below() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);

    assert!(tree.expand(&mut source, 0));

    assert_eq!(Fixture::ids(&tree), ["src", "main.rs", "lib", "docs"]);
    assert_eq!(
        Fixture::shape(&tree),
        [
            ("src".to_owned(), 0),
            ("main.rs".to_owned(), 1),
            ("lib".to_owned(), 1),
            ("docs".to_owned(), 0),
        ]
    );
}

#[test]
fn collapsing_removes_the_whole_subtree_not_just_the_children() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.expand(&mut source, 2); // lib
    assert_eq!(
        Fixture::ids(&tree),
        ["src", "main.rs", "lib", "mod.rs", "docs"]
    );

    assert!(tree.collapse(0));

    // `mod.rs` is a grandchild; a children-only removal would strand it.
    assert_eq!(Fixture::ids(&tree), ["src", "docs"]);
}

/// The reason `expanded` is authoritative rather than implied by the rows.
#[test]
fn collapse_then_expand_restores_the_descendants_that_were_open() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.expand(&mut source, 2); // lib, nested inside src

    tree.collapse(0);
    tree.expand(&mut source, 0);

    assert_eq!(
        Fixture::ids(&tree),
        ["src", "main.rs", "lib", "mod.rs", "docs"]
    );
    assert!(tree.is_expanded(&"lib".to_owned()));
}

#[test]
fn collapsing_keeps_a_hidden_descendant_marked_expanded() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.expand(&mut source, 2);

    tree.collapse(0);

    assert!(!tree.is_expanded(&"src".to_owned()));
    assert!(tree.is_expanded(&"lib".to_owned()));
}

#[test]
fn a_node_that_promised_children_and_produced_none_becomes_a_leaf() {
    let mut source = Liar;
    let mut tree = Tree::new(&mut source);
    assert!(
        tree.rows()[0].has_children,
        "the fixture must start optimistic"
    );

    // Expanding fails, and says so, rather than leaving an open empty node.
    assert!(!tree.expand(&mut source, 0));

    assert_eq!(tree.len(), 1);
    assert!(
        !tree.rows()[0].has_children,
        "the arrow must stop inviting the click"
    );
    assert!(!tree.rows()[0].expanded);
    assert!(!tree.is_expanded(&"only"));
}

#[test]
fn a_closed_subtree_is_never_enumerated() {
    let mut source = sample();
    let tree = Tree::new(&mut source);

    // One call, for the roots. Neither root's children were read.
    assert_eq!(source.children_calls, 1);
    assert_eq!(tree.len(), 2);
}

#[test]
fn selection_follows_its_node_when_rows_are_inserted_above_it() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.select(1); // docs
    assert_eq!(tree.selected_id(), Some(&"docs".to_owned()));

    tree.expand(&mut source, 0); // two rows inserted above docs

    assert_eq!(tree.selected(), Some(3));
    assert_eq!(tree.selected_id(), Some(&"docs".to_owned()));
}

#[test]
fn selection_moves_to_the_collapsed_node_when_its_own_row_is_hidden() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(1); // main.rs, inside src

    tree.collapse(0);

    assert_eq!(tree.selected(), Some(0));
    assert_eq!(tree.selected_id(), Some(&"src".to_owned()));
}

#[test]
fn selection_shifts_up_when_a_subtree_above_it_collapses() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(3); // docs, now below src's two children

    tree.collapse(0);

    assert_eq!(tree.selected(), Some(1));
    assert_eq!(tree.selected_id(), Some(&"docs".to_owned()));
}

#[test]
fn selection_on_the_collapsed_node_itself_does_not_move() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(0);

    tree.collapse(0);

    assert_eq!(tree.selected(), Some(0));
}

#[test]
fn out_of_range_indices_are_refused_rather_than_panicking() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.select(0);

    assert!(!tree.expand(&mut source, 99));
    assert!(!tree.collapse(99));
    assert!(!tree.toggle(&mut source, 99));
    assert!(!tree.select(99));
    assert_eq!(tree.row(99), None);
    // A refused select must not clear a good selection.
    assert_eq!(tree.selected(), Some(0));
}

#[test]
fn the_window_clamps_at_both_ends() {
    let mut source = sample();
    let tree = Tree::new(&mut source);

    assert_eq!(tree.window(0, 99).len(), 2);
    assert_eq!(tree.window(1, 99).len(), 1);
    assert_eq!(tree.window(99, 10).len(), 0);
    assert_eq!(tree.window(0, 0).len(), 0);
    assert_eq!(tree.window(usize::MAX, usize::MAX).len(), 0);
}

#[test]
fn toggle_opens_then_closes() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);

    assert!(tree.toggle(&mut source, 0));
    assert_eq!(tree.len(), 4);
    assert!(tree.toggle(&mut source, 0));
    assert_eq!(tree.len(), 2);
}

#[test]
fn refresh_keeps_expansion_and_moves_the_selection_with_its_node() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(3); // docs

    tree.refresh(&mut source);

    assert_eq!(Fixture::ids(&tree), ["src", "main.rs", "lib", "docs"]);
    assert_eq!(tree.selected_id(), Some(&"docs".to_owned()));
}

#[test]
fn refresh_drops_a_selection_whose_node_has_gone() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.select(1); // main.rs

    // The hierarchy loses that file underneath us.
    let mut shrunk = Fixture::new(&[("", "src"), ("", "docs"), ("src", "lib"), ("lib", "mod.rs")]);
    tree.refresh(&mut shrunk);

    assert_eq!(
        tree.selected(),
        None,
        "a dropped node must not leave the selection on whatever took its index"
    );
}

#[test]
fn collapse_all_forgets_state_where_a_collapse_would_have_kept_it() {
    let mut source = sample();
    let mut tree = Tree::new(&mut source);
    tree.expand(&mut source, 0);
    tree.expand(&mut source, 2);

    tree.collapse_all(&mut source);

    assert_eq!(Fixture::ids(&tree), ["src", "docs"]);
    assert!(
        !tree.is_expanded(&"lib".to_owned()),
        "collapse_all is the start-again verb"
    );
}

/// The projection is built with an explicit stack precisely so this cannot
/// overflow. 40,000 levels is far past anything real and well past the depth
/// a recursive build survives.
#[test]
fn a_very_deep_chain_neither_overflows_nor_truncates_its_depths() {
    const DEPTH: usize = 40_000;
    let mut source = Fixture::chain(DEPTH);
    let mut tree = Tree::new(&mut source);

    for index in 0..DEPTH - 1 {
        assert!(
            tree.expand(&mut source, index),
            "failed to expand level {index}"
        );
    }

    assert_eq!(tree.len(), DEPTH);
    assert_eq!(tree.rows()[DEPTH - 1].depth, DEPTH - 1);
    // The whole chain is one subtree of the root; collapsing must take all
    // of it, which is what a saturating depth would have broken.
    assert!(tree.collapse(0));
    assert_eq!(tree.len(), 1);
}

#[test]
fn an_empty_hierarchy_supports_every_operation() {
    let mut source = Fixture::new(&[]);
    let mut tree = Tree::new(&mut source);

    assert!(tree.is_empty());
    assert_eq!(tree.len(), 0);
    assert!(!tree.expand(&mut source, 0));
    assert!(!tree.collapse(0));
    assert!(!tree.select(0));
    assert_eq!(tree.selected_id(), None);
    assert_eq!(tree.window(0, 10).len(), 0);
    tree.refresh(&mut source);
    assert!(tree.is_empty());
}
