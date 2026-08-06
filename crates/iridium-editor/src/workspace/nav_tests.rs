//! Tests for movement over the flattened tab order.

use super::workspace_tests::{active_label, tab_labels, workspace};

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
    assert_eq!(active_label(&workspace), Some("a.rs"));

    assert!(workspace.next_tab());
    assert!(workspace.next_tab());

    assert_eq!(
        active_label(&workspace),
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
