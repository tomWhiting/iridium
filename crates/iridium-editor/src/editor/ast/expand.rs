//! The expansion stack, and the selection changes that fill it.
//!
//! Expanding is easy to get right and shrinking is not. Walking back *down* the
//! tree cannot restore what walking up destroyed, because expansion is lossy in
//! two ways at once: several cursors can expand onto the same node and merge
//! into one, and a selection that did not begin on a node boundary is snapped to
//! one on the way out. A tree walk alone would answer both cases with something
//! plausible and wrong.
//!
//! So the ranges are remembered. [`ExpandStack`] holds whole [`CursorState`]s —
//! not one stack per cursor — which is the only shape that survives merging:
//! expand three cursors, watch them collapse into one at the enclosing node,
//! shrink, and all three come back.
//!
//! # When the stack is thrown away
//!
//! Two conditions, following the same discipline the sticky preferred columns
//! use. The stack is discarded when the document revision has moved, and when
//! the live cursor state is not the one the last expansion produced. The second
//! is what makes every unrelated path self-invalidating: a click, a search jump,
//! an arrow key — none of them has to know this stack exists, because none of
//! them leaves the cursors where expansion left them.
//!
//! Shrinking with an empty stack still works: it falls back to a walk down the
//! tree, which is right for "I selected this by hand and want to narrow it" and
//! merely approximate for "I expanded and want it back". The stack is what makes
//! the second case exact.

#[cfg(feature = "syntax")]
use crate::document::Selection;
use crate::document::{CursorState, Document};
use crate::input::keyboard::AstRequest;

#[cfg(feature = "syntax")]
use iridium_syntax::{Node, Tree, navigate};

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::Tree;

/// The cursor states an expansion walked out through, newest last.
#[derive(Debug, Default, Clone)]
pub struct ExpandStack {
    /// The states to return to, in the order they were left.
    frames: Vec<CursorState>,
    /// The state the last push or pop produced.
    ///
    /// Compared against the live cursors to decide whether this stack still
    /// describes where the caller is. Anything that moves the cursors by another
    /// route makes them disagree, and the stack is dropped without that other
    /// route having to know it exists.
    produced: Option<CursorState>,
    /// The document revision the frames were recorded against.
    ///
    /// Only the tree-reading build has anything to compare it against; without
    /// tree-sitter nothing ever fills this stack, so carrying the field there
    /// would be a value written once and read never.
    #[cfg(feature = "syntax")]
    revision: u64,
}

impl ExpandStack {
    /// How many expansions can still be undone.
    ///
    /// Exposed because "shrink retraces expansion exactly" is a claim about
    /// state that is otherwise invisible, and a claim nothing can observe is a
    /// claim nothing can hold to.
    /// Not `const`: `Vec::len` only became usable in a const context in Rust
    /// 1.87, and this crate supports 1.85.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Drops every frame.
    ///
    /// Called outright whenever the document changes underneath the stack,
    /// rather than left to the lazy check below. The lazy check would reach the
    /// same answer at the next request, but until then [`ExpandStack::depth`]
    /// would report frames that are already dead — and a number that lies is
    /// worse than no number.
    pub(super) fn clear(&mut self) {
        self.frames.clear();
        self.produced = None;
    }
}

/// The half of the stack only a build with a parse tree can reach.
///
/// Split out rather than gated one method at a time: without tree-sitter no
/// expansion ever happens, so every one of these would be dead code, and dead
/// code that looks live is worse than an honest `cfg`.
#[cfg(feature = "syntax")]
impl ExpandStack {
    /// Discards the frames unless they still describe the caller's situation.
    fn refresh(&mut self, revision: u64, cursor: &CursorState) {
        if self.revision != revision || self.produced.as_ref() != Some(cursor) {
            self.frames.clear();
            self.produced = None;
        }
        self.revision = revision;
    }

    /// Records a widening: `previous` becomes the state a shrink returns to.
    fn push(&mut self, previous: CursorState, produced: &CursorState) {
        self.frames.push(previous);
        self.produced = Some(produced.clone());
    }

    /// Returns to the most recently recorded state, if there is one.
    fn pop(&mut self) -> Option<CursorState> {
        let frame = self.frames.pop()?;
        self.produced = Some(frame.clone());
        Some(frame)
    }

    /// Notes a state this stack produced without recording a frame.
    fn record(&mut self, produced: &CursorState) {
        self.produced = Some(produced.clone());
    }
}

/// Computes the cursor state a request should produce, or `None` for no change.
///
/// `None` means "nothing to do", never "something went wrong": a caret already
/// on the outermost node, a shrink with nothing left to shrink, a document whose
/// language has no grammar. Each of those is a key that quietly does nothing,
/// which is the right behaviour for a key held down.
#[cfg(feature = "syntax")]
pub(super) fn selection_for(
    request: AstRequest,
    tree: &Tree,
    document: &Document,
    cursor: &CursorState,
    stack: &mut ExpandStack,
) -> Option<CursorState> {
    stack.refresh(document.revision(), cursor);
    let root = tree.root_node();

    let (next, widening) = match request {
        AstRequest::SelectNode => (
            map_selections(root, document, cursor, navigate::node_at)?,
            true,
        ),
        AstRequest::ExpandSelection => (
            map_selections(root, document, cursor, navigate::expand)?,
            true,
        ),
        AstRequest::ShrinkSelection => {
            if let Some(frame) = stack.pop() {
                return Some(frame);
            }
            (
                map_selections(root, document, cursor, navigate::shrink)?,
                false,
            )
        },
    };

    if widening {
        stack.push(cursor.clone(), &next);
    } else {
        // A downward walk must not become a frame: pushing here would make the
        // *next* shrink widen the selection again.
        stack.record(&next);
    }

    Some(next)
}

/// The stub build has no tree to read, so every structural verb does nothing.
///
/// The verbs still exist, with the same ids and the same default bindings, so a
/// keymap and a palette listing are identical whether or not tree-sitter is
/// compiled in. Only the answer differs, and "no structure here" is the honest
/// one.
#[cfg(not(feature = "syntax"))]
pub(super) const fn selection_for(
    _request: AstRequest,
    _tree: &Tree,
    _document: &Document,
    _cursor: &CursorState,
    _stack: &mut ExpandStack,
) -> Option<CursorState> {
    None
}

/// Applies one tree walk to every cursor, or returns `None` if none moved.
///
/// Cursors that cannot be resolved against the document, or that the walk has no
/// answer for, are left exactly where they are rather than dropped — losing a
/// cursor is far worse than failing to move it, and a mixed result is normal:
/// one caret at the top level has nothing left to expand into while its
/// companions still do.
#[cfg(feature = "syntax")]
fn map_selections<'tree, F>(
    root: Node<'tree>,
    document: &Document,
    cursor: &CursorState,
    walk: F,
) -> Option<CursorState>
where
    F: Fn(Node<'tree>, &std::ops::Range<usize>) -> Option<Node<'tree>>,
{
    let mut moved = false;
    let mut next = Vec::with_capacity(cursor.cursor_count());

    for &selection in cursor.all_selections() {
        let walked = walk_one(root, document, selection, &walk);
        moved |= walked != selection;
        next.push(walked);
    }

    if !moved {
        return None;
    }

    // `add_cursor` merges anything that now overlaps, which is exactly what
    // should happen when two cursors expand onto the same node — and precisely
    // why the stack keeps whole states rather than one stack per cursor.
    let (&primary, rest) = next.split_first()?;
    let mut state = CursorState::new(primary);
    for &selection in rest {
        state.add_cursor(selection);
    }

    Some(state)
}

/// Moves one selection to the node a walk returns for it.
///
/// The selection's direction is preserved, so extending with `Shift` after an
/// expansion still grows from the end the person was working at.
#[cfg(feature = "syntax")]
fn walk_one<'tree, F>(
    root: Node<'tree>,
    document: &Document,
    selection: Selection,
    walk: &F,
) -> Selection
where
    F: Fn(Node<'tree>, &std::ops::Range<usize>) -> Option<Node<'tree>>,
{
    let (Some(start), Some(end)) = (
        document.position_to_offset(selection.start()),
        document.position_to_offset(selection.end()),
    ) else {
        return selection;
    };

    let Some(node) = walk(root, &(start..end)) else {
        return selection;
    };

    let range = node.byte_range();
    let (Some(from), Some(to)) = (
        document.offset_to_position(range.start),
        document.offset_to_position(range.end),
    ) else {
        return selection;
    };

    if selection.is_backward() {
        Selection::new(to, from)
    } else {
        Selection::new(from, to)
    }
}

#[cfg(test)]
#[cfg(feature = "syntax")]
mod tests;
