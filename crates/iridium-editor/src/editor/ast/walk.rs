//! The tree walks behind each structural verb, as byte ranges.
//!
//! [`iridium_syntax::navigate`] answers in *nodes*, which is the right shape for
//! a tree library and the wrong one here. Two of the verbs do not land on a node
//! at all: extending covers the current selection *and* a sibling, and the caret
//! motions collapse onto one edge of a node. So this module is the thin layer
//! that turns "which node" into "which bytes", and it is where the composition
//! lives — resolve what the selection is on, then step.
//!
//! Every walk has the same shape, `(root, range) -> Option<Range>`, so the
//! caller applies them uniformly and `None` uniformly means "this cursor stays
//! where it is". None of them needs to capture anything, which is why they are
//! plain functions rather than closures.

use std::ops::Range;

use iridium_syntax::{Node, navigate};

/// One structural step, in bytes.
///
/// `None` means the tree has no answer — the end of the document, a node with no
/// children — and the caller leaves that cursor alone.
pub(super) type Walk = for<'tree> fn(Node<'tree>, &Range<usize>) -> Option<Range<usize>>;

/// Snaps to the smallest node covering the range.
pub(super) fn select_node(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    navigate::node_at(root, range).map(|node| node.byte_range())
}

/// Widens to the smallest node strictly containing the range.
pub(super) fn expand(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    navigate::expand(root, range).map(|node| node.byte_range())
}

/// Narrows to the largest node strictly inside the range.
pub(super) fn shrink(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    navigate::shrink(root, range).map(|node| node.byte_range())
}

/// Moves to the next node beside the one under the range.
pub(super) fn next_sibling(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    sibling(root, range, navigate::next_sibling).map(|node| node.byte_range())
}

/// Moves to the previous node beside the one under the range.
pub(super) fn previous_sibling(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    sibling(root, range, navigate::previous_sibling).map(|node| node.byte_range())
}

/// Descends to the first child of the node under the range.
pub(super) fn first_child(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    navigate::first_child(navigate::node_at(root, range)?).map(|node| node.byte_range())
}

/// Descends to the last child of the node under the range.
pub(super) fn last_child(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    navigate::last_child(navigate::node_at(root, range)?).map(|node| node.byte_range())
}

/// Grows the range to also cover the node after it.
pub(super) fn extend_next_sibling(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    let next = beyond(root, range, Side::After)?;
    Some(union(range, &next.byte_range()))
}

/// Grows the range to also cover the node before it.
pub(super) fn extend_previous_sibling(
    root: Node<'_>,
    range: &Range<usize>,
) -> Option<Range<usize>> {
    let previous = beyond(root, range, Side::Before)?;
    Some(union(range, &previous.byte_range()))
}

/// Collapses onto the start of the node the range sits in.
pub(super) fn node_start(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    let start = navigate::node_starting_before(root, range)?.start_byte();
    Some(start..start)
}

/// Collapses onto the end of the node the range sits in.
pub(super) fn node_end(root: Node<'_>, range: &Range<usize>) -> Option<Range<usize>> {
    let end = navigate::node_ending_after(root, range)?.end_byte();
    Some(end..end)
}

/// Every node beside the one under the range, including it, in source order.
///
/// Returns an empty vector when there is nothing to spread onto, which the
/// caller reads the same way it reads a `None` from a [`Walk`].
pub(super) fn every_sibling(root: Node<'_>, range: &Range<usize>) -> Vec<Range<usize>> {
    navigate::node_at(root, range).map_or_else(Vec::new, |node| {
        navigate::siblings(node)
            .into_iter()
            .map(|node| node.byte_range())
            .collect()
    })
}

/// Every child of the node under the range, in source order.
pub(super) fn every_child(root: Node<'_>, range: &Range<usize>) -> Vec<Range<usize>> {
    navigate::node_at(root, range).map_or_else(Vec::new, |node| {
        navigate::children(node)
            .into_iter()
            .map(|node| node.byte_range())
            .collect()
    })
}

/// Resolves the range to a node, then takes one sideways step from it.
///
/// Shared by both moving sibling verbs so they cannot disagree about which node
/// the walk starts from.
fn sibling<'tree>(
    root: Node<'tree>,
    range: &Range<usize>,
    step: fn(Node<'tree>) -> Option<Node<'tree>>,
) -> Option<Node<'tree>> {
    step(navigate::node_at(root, range)?)
}

/// Which end of the range the extend verbs are reaching past.
#[derive(Clone, Copy)]
enum Side {
    /// Towards the start of the document.
    Before,
    /// Towards its end.
    After,
}

/// Returns the next whole node beyond one end of `range`.
///
/// This is **not** [`sibling`], and the difference is the whole reason extending
/// is a separate verb rather than a wrapper over one. Once a selection covers two
/// array elements it no longer *is* a node — it resolves to the array that
/// contains them, whose own next sibling is somewhere else entirely. Asking for
/// the array's sibling would make the second press of an extend key jump out of
/// the array instead of picking up its third element.
///
/// So a range that exactly matches a node steps outward from that node, and a
/// range that spans part of one looks *inside* it for the first child clear of
/// the range's edge — falling back to the outward step when the range already
/// reaches the node's own edge.
fn beyond<'tree>(root: Node<'tree>, range: &Range<usize>, side: Side) -> Option<Node<'tree>> {
    let covering = navigate::node_at(root, range)?;

    let step = match side {
        Side::Before => navigate::previous_sibling,
        Side::After => navigate::next_sibling,
    };

    if covering.byte_range() == *range {
        return step(covering);
    }

    let children = navigate::children(covering);
    let inside = match side {
        Side::Before => children
            .into_iter()
            .rev()
            .find(|child| child.end_byte() <= range.start),
        Side::After => children
            .into_iter()
            .find(|child| child.start_byte() >= range.end),
    };

    inside.or_else(|| step(covering))
}

/// The smallest range covering both.
///
/// Not a set union: the gap between two siblings is the comma and whitespace
/// holding them apart, and selecting three array elements has to include the
/// commas or the selection is not a thing that can be cut and pasted.
fn union(left: &Range<usize>, right: &Range<usize>) -> Range<usize> {
    left.start.min(right.start)..left.end.max(right.end)
}
