//! The hierarchy a [`Tree`](crate::Tree) projects.

use core::hash::Hash;

/// Supplies the hierarchy a [`Tree`](crate::Tree) walks.
///
/// One implementation per kind of thing a sidebar can show: a directory
/// listing, a syntax tree, a task list. The tree calls into it lazily — only
/// when a node is expanded — and never holds a borrow across its own
/// mutations, so an implementation is free to cache whatever it likes.
///
/// # Contract
///
/// Implementations must satisfy all four of these. The tree's guarantees are
/// only as good as these are, and it cannot check them cheaply:
///
/// 1. **`children` is stable between refreshes.** Calling it twice for the
///    same node, with no [`Tree::refresh`](crate::Tree::refresh) in between,
///    yields the same ids in the same order. The tree splices rows on the
///    strength of this; a source that reordered underneath it would leave
///    the projection describing a shape that no longer exists.
/// 2. **Ids are unique across the whole hierarchy, not merely among
///    siblings.** Expansion is keyed by id, so two nodes sharing one would
///    open and close together. A path is unique; a filename is not.
/// 3. **The hierarchy is acyclic.** A cycle would make an expanded subtree
///    unbounded. A filesystem source is responsible for not following
///    symlinks into an ancestor — this crate has no way to see it coming.
/// 4. **`has_children` agrees with `children` being non-empty**, or errs
///    towards `true`. Disagreement is not corrupting: the tree treats a node
///    that promised children and produced none as a leaf from then on. It
///    only means a disclosure arrow may appear on something unopenable, and
///    then quietly stop appearing.
///
/// # Cost
///
/// `has_children` is called for every row the tree materialises, so it must
/// be cheap — a cached flag, or a `st_mode` already in hand. If answering it
/// truthfully would cost a directory read, answer `true` and let the
/// expansion discover the truth; that is the case rule 4 exists to permit.
pub trait TreeSource {
    /// Identifies a node uniquely within the whole hierarchy.
    ///
    /// `Clone` because the tree stores one per visible row and one per
    /// expanded node; keep it small, or make cloning cheap. A path is fine
    /// as an `Arc<Path>` or an index into the source's own arena, and an
    /// index is usually the better answer.
    type Id: Clone + Eq + Hash;

    /// The children of `parent`, or the roots when it is `None`.
    ///
    /// Order is the source's to decide and the tree preserves it exactly:
    /// sorting, grouping directories first, and hiding dotfiles are all
    /// policy, and policy belongs to whoever knows what the nodes are.
    ///
    /// Returning an empty `Vec` marks the node a leaf, whatever
    /// [`has_children`](TreeSource::has_children) said earlier.
    fn children(&mut self, parent: Option<&Self::Id>) -> Vec<Self::Id>;

    /// Whether `node` should be drawn as openable.
    ///
    /// May be answered optimistically; see the contract above.
    fn has_children(&mut self, node: &Self::Id) -> bool;
}
