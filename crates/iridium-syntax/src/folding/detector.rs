//! The detector: one language's fold rules, applied to a tree it does not own.

use std::ops::Range;

use tree_sitter::{Node, Tree, TreeCursor};

use super::{FoldKind, FoldRegion, FoldableNodeTypes, TrackedRegion, publish};
use crate::Language;

/// Detects foldable regions in a parse tree.
///
/// The detector owns no parser and no tree: it holds one language's rules about
/// which node kinds are foldable, and reads a [`Tree`] someone else parsed. See
/// [`crate::SyntaxTree`] for the owner.
///
/// # Example
///
/// ```
/// use iridium_syntax::{FoldDetector, Language, SyntaxTree};
///
/// let mut tree = SyntaxTree::new(Language::Rust)?;
/// let source = "fn main() {\n    println!(\"Hello\");\n}";
/// let parsed = tree.parse(source).expect("valid Rust parses");
///
/// let detector = FoldDetector::new(Language::Rust);
/// let regions = detector.regions_in(parsed);
///
/// // Should find at least the function body as a foldable region
/// assert!(!regions.is_empty());
/// # Ok::<(), iridium_syntax::SyntaxError>(())
/// ```
#[derive(Debug)]
pub struct FoldDetector {
    language: Language,
    node_types: FoldableNodeTypes,
}

impl FoldDetector {
    /// Creates a fold detector for the given language.
    ///
    /// Cannot fail: the rules are a static table, and a language with no row in
    /// it gets the empty set — it folds nothing rather than failing to build.
    ///
    /// No longer `const`: the table is keyed by language identifier now that
    /// the language set is data rather than a closed enum, and comparing
    /// strings is not something a `const fn` can do. Nothing constructed one of
    /// these in a constant, so this costs nothing — the lookup is a handful of
    /// string comparisons, once per detector rather than once per node.
    #[must_use]
    pub fn new(language: Language) -> Self {
        Self {
            language,
            node_types: FoldableNodeTypes::for_language(language),
        }
    }

    /// Returns the language this detector is configured for.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Returns the foldable regions in `tree`, sorted by start line.
    ///
    /// Every fold this detector recognises is decided by node kind and position
    /// alone, so no source text is needed — and none is asked for, because the
    /// only way a caller could supply it on a rope-backed document is to copy
    /// the whole document.
    ///
    /// Only regions spanning at least two lines survive, since a single-line
    /// construct cannot be meaningfully folded — the one exception being runs
    /// of consecutive single-line imports, which are merged into one region.
    ///
    /// This walks the whole tree. On a document being edited, prefer
    /// [`super::FoldCache`], which produces the same answer without the walk.
    #[must_use]
    pub fn regions_in(&self, tree: &Tree) -> Vec<FoldRegion> {
        let mut tracked = Vec::new();
        self.collect_all(tree, &mut tracked);
        publish(&tracked)
    }

    /// Walks `tree` whole in pre-order, appending one [`TrackedRegion`] per
    /// foldable node, and returns how many nodes were looked at.
    ///
    /// One [`TreeCursor`] is created for the whole traversal. Calling
    /// `Node::walk` per node instead — which is what this replaced — allocates a
    /// fresh cursor at every node; on a 100,000-line JSON document that was
    /// millions of allocations to produce a single region.
    pub(crate) fn collect_all(&self, tree: &Tree, out: &mut Vec<TrackedRegion>) -> u64 {
        let appended_from = out.len();
        let mut cursor = tree.walk();
        let mut visited: u64 = 0;

        'walk: loop {
            visited += 1;
            self.emit(cursor.node(), out);

            if cursor.goto_first_child() {
                continue 'walk;
            }
            loop {
                if cursor.goto_next_sibling() {
                    continue 'walk;
                }
                if !cursor.goto_parent() {
                    break 'walk;
                }
            }
        }

        finish(out, appended_from);
        visited
    }

    /// Walks only the part of `tree` that overlaps `scope`, appending one
    /// [`TrackedRegion`] per foldable node found there, and returns how many
    /// nodes were looked at.
    ///
    /// A node whose extent shares no byte with `scope` is skipped along with its
    /// whole subtree, which is sound because a child is always contained in its
    /// parent: a parent that misses the range has no descendant that could hit
    /// it. That pruning is the entire reason an edit does not cost a full walk.
    ///
    /// Descent uses [`TreeCursor::goto_first_child_for_byte`] rather than
    /// stepping through siblings. It matters more than it looks: the root of a
    /// 100,000-line JSON array has two hundred thousand direct children, and
    /// examining each one from Rust to reject it cost 34 ms per keystroke on its
    /// own. Note what that means for the count returned — tree-sitter still
    /// walks those children internally to answer the seek, so this reports the
    /// nodes *this detector* examined, not every node tree-sitter touched.
    ///
    /// The count is returned rather than kept because it is the only way
    /// anything outside here can hold the pruning to its claim; a wall-clock
    /// assertion would measure the machine rather than the algorithm.
    pub(crate) fn collect_scoped(
        &self,
        tree: &Tree,
        scope: &Range<usize>,
        out: &mut Vec<TrackedRegion>,
    ) -> u64 {
        let appended_from = out.len();
        let mut cursor = tree.walk();
        let mut visited: u64 = 0;

        // The root spans the document, so it always overlaps and is always the
        // right place to start.
        'walk: loop {
            visited += 1;
            self.emit(cursor.node(), out);

            if descend_into_scope(&mut cursor, scope) {
                continue 'walk;
            }
            loop {
                if cursor.goto_next_sibling() && in_scope(cursor.node(), scope) {
                    continue 'walk;
                }
                if !cursor.goto_parent() {
                    break 'walk;
                }
            }
        }

        finish(out, appended_from);
        visited
    }

    /// Appends the region `node` produces, if it produces one.
    fn emit(&self, node: Node<'_>, out: &mut Vec<TrackedRegion>) {
        let node_type = node.kind();
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;

        // At most one region per node: the multi-line and single-line arms are
        // mutually exclusive by construction.
        let kind = if end_line > start_line {
            // Only create fold regions for multi-line constructs
            if self.node_types.blocks.contains(&node_type) {
                Some(FoldKind::Block)
            } else if self.node_types.comments.contains(&node_type) {
                // Line comments form a block only as a run, which a single node
                // cannot see; they are left to the grouping pass instead.
                if node_type == "line_comment" || node_type == "comment" {
                    None
                } else {
                    Some(FoldKind::Comment)
                }
            } else if self.node_types.imports.contains(&node_type) {
                Some(FoldKind::Import)
            } else {
                None
            }
        } else if self.node_types.imports.contains(&node_type) {
            // Single-line imports that might form a group
            Some(FoldKind::Import)
        } else {
            None
        };

        if let Some(kind) = kind {
            out.push(TrackedRegion {
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                region: FoldRegion::new(start_line, end_line, kind),
            });
        }
    }
}

/// Puts what a walk appended into [`TrackedRegion::order_key`] order.
///
/// A pre-order walk already emits it that way for every tree with no zero-width
/// foldable node, and the sort is stable, so this changes nothing for a normal
/// tree. It is here because the incremental cache merges two such runs by that
/// key, and a merge is only as ordered as its inputs.
fn finish(out: &mut [TrackedRegion], appended_from: usize) {
    out[appended_from..].sort_by_key(TrackedRegion::order_key);
}

/// True when `node` shares any byte with `scope`, endpoints included.
fn in_scope(node: Node<'_>, scope: &Range<usize>) -> bool {
    node.start_byte() <= scope.end && node.end_byte() >= scope.start
}

/// Moves the cursor to the first child overlapping `scope`, if there is one.
///
/// The seek goal is one byte before the scope so that a child ending *exactly*
/// where the scope begins is still found: tree-sitter looks for the first child
/// whose end is strictly past the goal, and a node abutting an edit is one the
/// caller has already discarded and is relying on this to find again.
///
/// Every child before the one found ends before the scope starts, and every
/// child after the one found begins later still, so overlap for the rest of the
/// sweep reduces to the single start-byte test in [`in_scope`]'s caller.
fn descend_into_scope(cursor: &mut TreeCursor<'_>, scope: &Range<usize>) -> bool {
    let goal = scope.start.saturating_sub(1);
    if cursor.goto_first_child_for_byte(goal).is_none() {
        return false;
    }
    if in_scope(cursor.node(), scope) {
        return true;
    }
    // The first child past the goal already starts beyond the scope, so no
    // child of this node overlaps. Undo the descent.
    cursor.goto_parent();
    false
}
