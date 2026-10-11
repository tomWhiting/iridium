//! Syntax steps retain depth when crossing enclosing branches.
#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::Tree;
#[cfg(feature = "syntax")]
use iridium_syntax::{Tree, navigate};
use std::ops::Range;

#[cfg(feature = "syntax")]
pub(super) fn child(tree: Option<&Tree>, range: &Range<usize>) -> Option<Range<usize>> {
    let mut node = navigate::node_at(tree?.root_node(), range)?;
    loop {
        node = navigate::first_child(node)?;
        if node.byte_range() != *range {
            return Some(node.byte_range());
        }
    }
}

#[cfg(feature = "syntax")]
pub(super) fn parent(tree: Option<&Tree>, range: &Range<usize>) -> Option<Range<usize>> {
    navigate::expand(tree?.root_node(), range).map(|node| node.byte_range())
}

#[cfg(feature = "syntax")]
pub(super) fn sibling(
    tree: Option<&Tree>,
    range: &Range<usize>,
    forward: bool,
) -> Option<Range<usize>> {
    let original = navigate::node_at(tree?.root_node(), range)?;
    let leaf = original.named_child_count() == 0;
    let mut current = original;
    let mut climbed = 0usize;
    loop {
        let next = if forward {
            current.next_named_sibling()
        } else {
            current.prev_named_sibling()
        };
        if let Some(mut next) = next {
            while climbed > 0 || leaf {
                let child = if forward {
                    navigate::first_child(next)
                } else {
                    navigate::last_child(next)
                };
                let Some(child) = child else {
                    break;
                };
                next = child;
                climbed = climbed.saturating_sub(1);
            }
            return Some(next.byte_range());
        }
        current = current.parent()?;
        climbed += 1;
    }
}

#[cfg(not(feature = "syntax"))]
pub(super) const fn child(_tree: Option<&Tree>, _range: &Range<usize>) -> Option<Range<usize>> {
    None
}
#[cfg(not(feature = "syntax"))]
pub(super) const fn parent(_tree: Option<&Tree>, _range: &Range<usize>) -> Option<Range<usize>> {
    None
}
#[cfg(not(feature = "syntax"))]
pub(super) const fn sibling(
    _tree: Option<&Tree>,
    _range: &Range<usize>,
    _forward: bool,
) -> Option<Range<usize>> {
    None
}

#[cfg(feature = "syntax")]
pub(super) fn exact_kind(tree: Option<&Tree>, range: &Range<usize>) -> Option<&'static str> {
    let node = navigate::node_at(tree?.root_node(), range)?;
    (node.byte_range() == *range).then_some(node.kind())
}
#[cfg(not(feature = "syntax"))]
pub(super) const fn exact_kind(
    _tree: Option<&Tree>,
    _range: &Range<usize>,
) -> Option<&'static str> {
    None
}

/// Choose the closest disjoint node of the same kind on the destination row,
/// or the next eligible row in that direction. Irrelevant subtrees are skipped.
#[cfg(feature = "syntax")]
pub(super) fn spatial_peer(
    tree: Option<&Tree>,
    range: &Range<usize>,
    position: crate::Position,
    down: bool,
) -> Option<Range<usize>> {
    let kind = exact_kind(tree, range)?;
    let mut walk = tree?.walk();
    let mut best: Option<((usize, usize), Range<usize>)> = None;
    loop {
        let node = walk.node();
        let start = node.start_position();
        let end = node.end_position();
        let relevant = if down {
            node.end_byte() > range.end && end.row >= position.line
        } else {
            node.start_byte() < range.start && start.row <= position.line
        };
        let beyond_best = best.as_ref().is_some_and(|((distance, _), _)| {
            if down {
                start.row > position.line.saturating_add(*distance)
            } else {
                end.row < position.line.saturating_sub(*distance)
            }
        });
        if relevant && !beyond_best {
            let disjoint = if down {
                node.start_byte() >= range.end && start.row >= position.line
            } else {
                node.end_byte() <= range.start && start.row <= position.line
            };
            if disjoint && node.kind() == kind && !node.byte_range().is_empty() {
                // The caller supplied a byte column, matching tree-sitter coordinates.
                let score = (
                    start.row.abs_diff(position.line),
                    start.column.abs_diff(position.column),
                );
                if best.as_ref().is_none_or(|(previous, _)| score < *previous) {
                    best = Some((score, node.byte_range()));
                }
            }
            if walk.goto_first_child() {
                continue;
            }
        }
        while !walk.goto_next_sibling() {
            if !walk.goto_parent() {
                return best.map(|(_, range)| range);
            }
        }
    }
}
#[cfg(not(feature = "syntax"))]
pub(super) const fn spatial_peer(
    _tree: Option<&Tree>,
    _range: &Range<usize>,
    _position: crate::Position,
    _down: bool,
) -> Option<Range<usize>> {
    None
}
