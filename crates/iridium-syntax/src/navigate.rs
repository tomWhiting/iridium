//! Structural walks over a parse tree.
//!
//! Every function here is a pure map from a tree position to another tree
//! position. Nothing in this module knows what a cursor, a selection or an
//! editor is: the input is a node or a byte range, the output is a node. That
//! is deliberate — the walks are the part worth testing exhaustively, and they
//! are testable only while they stay this narrow.
//!
//! # Named nodes only
//!
//! Every result is a *named* node. Anonymous nodes are the grammar's
//! punctuation and keywords — `{`, `,`, `fn` — and selecting one is almost
//! never what a person meant by "select the thing I am on". Anonymous nodes are
//! still traversed on the way through; they are just never the answer.
//!
//! Error nodes are not filtered out. A document under active editing is broken
//! more often than not, and a navigator that refuses to move inside an
//! incomplete expression is a navigator that stops working exactly when it is
//! being used.
//!
//! # Two rules that are not tree-sitter's defaults
//!
//! **A caret touches the token on either side of it.** Tree-sitter resolves an
//! empty range at the *start* of a token to that token, but an empty range at
//! its *end* to the token's parent. Since a caret sitting just after the word
//! that was typed is the single most common place for it to be, taking that
//! asymmetry literally would make [`expand`] skip the token roughly half the
//! time. [`resolve`] probes both sides of an empty range and keeps the smaller
//! answer.
//!
//! **Expansion never returns the range it was handed.** Grammars are full of
//! wrappers with the same extent as their child: in Python, `assignment`,
//! `expression_statement` and `block` can all span exactly the same bytes.
//! Returning the immediate parent would mean three keypresses that visibly do
//! nothing. [`expand`] climbs until the extent actually grows.

use std::ops::Range;

use tree_sitter::Node;

/// Returns the smallest named node covering `range`.
///
/// This is "what am I on" — the node a structural selection starts from. An
/// empty `range` is a caret, and follows the touching rule described in the
/// module docs.
///
/// Returns `None` only when `root` has no named node covering the range at all,
/// which for a real parse means the tree is empty.
#[must_use]
pub fn node_at<'tree>(root: Node<'tree>, range: &Range<usize>) -> Option<Node<'tree>> {
    let range = clamped(root, range);
    resolve(root, &range)
}

/// Returns the smallest named node that covers `range` and is strictly larger.
///
/// The daily driver: press it from a caret to select the token, again to select
/// the expression, again for the statement. Each call is guaranteed to widen
/// the selection or return `None`, so a caller can bind it to a key and trust
/// that a press always either moves or stops.
///
/// Returns `None` when nothing in the tree is larger — the range already covers
/// the outermost node — which callers should treat as "stay where you are"
/// rather than as an error.
#[must_use]
pub fn expand<'tree>(root: Node<'tree>, range: &Range<usize>) -> Option<Node<'tree>> {
    let range = clamped(root, range);
    let mut candidate = Some(resolve(root, &range)?);

    while let Some(node) = candidate {
        if node.is_named() && strictly_contains(&node.byte_range(), &range) {
            return Some(node);
        }
        candidate = node.parent();
    }

    None
}

/// Returns the largest named node strictly inside `range`, nearest its start.
///
/// The inverse of [`expand`] for callers that have no record of how they got
/// where they are. It descends towards `range.start`, which makes it round-trip
/// with `expand` in the common case, and it is *not* the mechanism a shrink
/// command should prefer: an editor that remembers the ranges it expanded
/// through can restore them exactly, including the cursor count, which this
/// cannot.
///
/// Returns `None` when nothing is strictly smaller — notably for an empty
/// range, where no node can fit inside.
#[must_use]
pub fn shrink<'tree>(root: Node<'tree>, range: &Range<usize>) -> Option<Node<'tree>> {
    let range = clamped(root, range);
    let mut node = resolve(root, &range)?;

    loop {
        let child = child_towards(node, range.start)?;
        if strictly_contains(&range, &child.byte_range()) {
            return Some(child);
        }
        node = child;
    }
}

/// Returns the next named node at or above `node`'s level.
///
/// When `node` is the last of its siblings the walk climbs and asks again, so
/// the end of an argument list continues into whatever follows the call rather
/// than stopping dead. Returns `None` only at the end of the tree.
#[must_use]
pub fn next_sibling(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        if let Some(sibling) = current.next_named_sibling() {
            return Some(sibling);
        }
        current = current.parent()?;
    }
}

/// Returns the previous named node at or above `node`'s level.
///
/// The mirror of [`next_sibling`], climbing on the same terms.
#[must_use]
pub fn previous_sibling(node: Node<'_>) -> Option<Node<'_>> {
    let mut current = node;
    loop {
        if let Some(sibling) = current.prev_named_sibling() {
            return Some(sibling);
        }
        current = current.parent()?;
    }
}

/// Returns `node`'s first named child.
#[must_use]
pub fn first_child(node: Node<'_>) -> Option<Node<'_>> {
    node.named_child(0)
}

/// Returns `node`'s last named child.
#[must_use]
pub fn last_child(node: Node<'_>) -> Option<Node<'_>> {
    let count = u32::try_from(node.named_child_count()).ok()?;
    node.named_child(count.checked_sub(1)?)
}

/// Returns every named child of `node`, in source order.
///
/// The list a caller needs to put one cursor on each element of an array or
/// each field of an object. Anonymous children — the commas and braces holding
/// them apart — are absent, which is exactly what makes the result usable as a
/// cursor set.
#[must_use]
pub fn children(node: Node<'_>) -> Vec<Node<'_>> {
    // A node cannot really hold four billion children; saturating rather than
    // failing keeps this total, and the loop below simply stops early.
    let count = u32::try_from(node.named_child_count()).unwrap_or(u32::MAX);
    (0..count)
        .filter_map(|index| node.named_child(index))
        .collect()
}

/// Returns `node` and every named node beside it, in source order.
///
/// `node` itself is included, so the result is the full set a "cursor on every
/// sibling" command should land on. A node with no parent is its own only
/// sibling.
#[must_use]
pub fn siblings(node: Node<'_>) -> Vec<Node<'_>> {
    node.parent().map_or_else(|| vec![node], children)
}

/// Resolves a byte range to the smallest named node covering it.
///
/// For a non-empty range this is tree-sitter's own answer. For an empty range —
/// a caret — it is the smaller of the nodes on either side, which is what makes
/// `foo|` behave like `|foo`. See the module docs for why that matters.
fn resolve<'tree>(root: Node<'tree>, range: &Range<usize>) -> Option<Node<'tree>> {
    let ahead = root.named_descendant_for_byte_range(range.start, range.end);

    if !range.is_empty() || range.start == 0 {
        return ahead;
    }

    // Probing one byte back may land mid-character on a multi-byte codepoint.
    // That is safe: this is an offset comparison against node bounds, not a
    // slice of the source, so the enclosing node is found either way.
    let behind = root.named_descendant_for_byte_range(range.start - 1, range.start);

    match (ahead, behind) {
        (Some(ahead), Some(behind)) if extent(behind) < extent(ahead) => Some(behind),
        (Some(ahead), _) => Some(ahead),
        (None, behind) => behind,
    }
}

/// Returns the named child of `node` to descend into for `anchor`.
///
/// The child covering `anchor` when there is one; otherwise the first named
/// child, because an anchor that sits in whitespace or on punctuation still has
/// to descend somewhere and the first child is the only choice that does not
/// depend on how the grammar happens to lay the node out.
fn child_towards(node: Node<'_>, anchor: usize) -> Option<Node<'_>> {
    children(node)
        .into_iter()
        .find(|child| child.byte_range().contains(&anchor))
        .or_else(|| node.named_child(0))
}

/// Whether `outer` covers `inner` and is larger than it.
///
/// Equal ranges are not strictly contained, which is the whole reason this
/// exists: it is what stops [`expand`] returning a same-extent wrapper.
const fn strictly_contains(outer: &Range<usize>, inner: &Range<usize>) -> bool {
    outer.start <= inner.start
        && inner.end <= outer.end
        && (outer.start != inner.start || outer.end != inner.end)
}

/// How many bytes a node covers.
fn extent(node: Node<'_>) -> usize {
    node.end_byte().saturating_sub(node.start_byte())
}

/// Brings a range inside `root`'s bounds, ordering its ends if need be.
///
/// A range can arrive reversed (a selection dragged backwards) or reaching past
/// the tree (a tree that has not caught up with the document). Neither is worth
/// refusing to navigate over, and both have one obvious reading.
fn clamped(root: Node<'_>, range: &Range<usize>) -> Range<usize> {
    let bounds = root.byte_range();
    let low = range.start.min(range.end).clamp(bounds.start, bounds.end);
    let high = range.start.max(range.end).clamp(bounds.start, bounds.end);
    low..high
}

#[cfg(test)]
mod tests;
