//! Narrowing the tree to what a query matches, without flattening it.
//!
//! # Why the hierarchy survives the filter
//!
//! A ranked flat list is the easier thing to build and the wrong thing to
//! show. Two files called `mod.rs` are told apart only by where they live, and
//! a list that ranks them side by side has thrown away the one fact that
//! distinguishes them. So a match is drawn where it lives: its folders stay
//! above it, indented, and the rows read as a smaller version of the same tree
//! rather than as a different kind of thing that appears when you type.
//!
//! The cost is that folders occupy rows without being results, which is why
//! they are **not selectable**: `↑`/`↓` step between matches and skip the
//! context around them, so the arrow keys never land somewhere `Enter` has
//! nothing to do.
//!
//! # It walks only what has been read
//!
//! [`FileTree::listed_children`] posts nothing. That is deliberate and it is
//! the difference between a filter and a crawl: sweeping with the frame-path
//! accessor would request a directory read for every unlisted folder the walk
//! touched, so one keystroke on a large project would queue thousands of
//! reads. Widening the reach is a separate, bounded mechanism — not a side
//! effect of typing.
//!
//! # Every ancestor of a hit is already listed
//!
//! The invariant the reveal path depends on. A node exists in the arena
//! because it appeared in its parent's listing, so a walk up from any row
//! meets only nodes whose children are known — which means expanding that
//! chain cannot trip [`iridium_tree::TreeSource`]'s fourth rule, and needs no
//! deferred-intent machinery.

use iridium_editor::fuzzy::{Query, match_path};
use iridium_explorer::{FileTree, NodeId};

/// One row of the filtered view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterRow {
    /// The node this row draws.
    pub id: NodeId,
    /// How far the row is indented, counted from the root at zero.
    pub depth: usize,
    /// Character positions **into the node's name** to highlight.
    ///
    /// Empty for a folder kept only because something inside it matched, and
    /// also for a hit whose characters all landed in the directory chain —
    /// see [`iridium_editor::fuzzy::PathMatch::matched_in_name`].
    pub positions: Vec<u32>,
    /// The weighted score, or `None` for a row kept only as context.
    ///
    /// This is also what makes a row selectable: a context row is scenery.
    pub score: Option<i32>,
}

impl FilterRow {
    /// Whether the selection may land on this row.
    #[must_use]
    pub const fn is_selectable(&self) -> bool {
        self.score.is_some()
    }
}

/// The rows a query produces, and which of them is the best answer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FilterView {
    /// Every row, in tree order — matches and the folders that hold them.
    pub rows: Vec<FilterRow>,
    /// The index into [`Self::rows`] of the highest-scoring match.
    ///
    /// `None` when nothing matched, which is also when [`Self::rows`] is
    /// empty: a view with context but no hits would be the whole tree with
    /// extra steps.
    pub best: Option<usize>,
}

impl FilterView {
    /// The first selectable row at or after `from`, if there is one.
    #[must_use]
    pub fn next_selectable(&self, from: usize) -> Option<usize> {
        self.rows
            .iter()
            .enumerate()
            .skip(from)
            .find(|(_, row)| row.is_selectable())
            .map(|(index, _)| index)
    }

    /// The last selectable row at or before `from`, if there is one.
    #[must_use]
    pub fn previous_selectable(&self, from: usize) -> Option<usize> {
        self.rows
            .iter()
            .enumerate()
            .take(from.saturating_add(1))
            .rfind(|(_, row)| row.is_selectable())
            .map(|(index, _)| index)
    }
}

/// One node as the walk found it, before the keep decision is made.
struct Visit {
    id: NodeId,
    depth: usize,
    /// Index into the visit list of the node this one was listed under.
    parent: Option<usize>,
    /// The path this node is scored by, relative to the root and with the
    /// root's own name excluded — the root is where the search starts, so
    /// its name is not part of what distinguishes anything inside it.
    relative: String,
    score: Option<i32>,
    positions: Vec<u32>,
}

/// Narrows `files` to the nodes matching `query`, keeping their ancestors.
///
/// `query` is taken as text and normalized here, so a caller holding a text
/// field does not have to know about [`Query`]. An empty query returns an
/// empty view: "show everything" is the unfiltered tree, which is a different
/// code path and a different set of rows.
#[must_use]
pub fn filter(files: &FileTree, query: &str) -> FilterView {
    let query = Query::new(query);
    if query.is_empty() {
        return FilterView::default();
    }

    let visits = walk(files, &query);
    let keep = keep_flags(&visits);

    let mut rows = Vec::new();
    let mut best: Option<(usize, i32)> = None;
    for (index, visit) in visits.into_iter().enumerate() {
        if !keep.get(index).copied().unwrap_or(false) {
            continue;
        }
        let row = rows.len();
        if let Some(score) = visit.score {
            // Ties go to the earlier row, which is the shallower or
            // alphabetically earlier one — so the same query always lands on
            // the same file rather than on whichever the walk reached first
            // after an unrelated directory was read.
            if best.is_none_or(|(_, current)| score > current) {
                best = Some((row, score));
            }
        }
        rows.push(FilterRow {
            id: visit.id,
            depth: visit.depth,
            positions: visit.positions,
            score: visit.score,
        });
    }

    FilterView {
        rows,
        best: best.map(|(row, _)| row),
    }
}

/// Visits every listed node under the root, in the order it would be drawn.
///
/// Iterative rather than recursive: a directory tree's depth is bounded by the
/// filesystem in practice, but "in practice" is not a bound, and a stack
/// overflow is not an error anything can catch.
fn walk(files: &FileTree, query: &Query) -> Vec<Visit> {
    let root = files.root();
    let mut visits: Vec<Visit> = Vec::new();
    // `(node, depth, parent visit index)`. Children are pushed in reverse so
    // they pop in listing order, which is the order they are drawn in.
    let mut stack: Vec<(NodeId, usize, Option<usize>)> = vec![(root, 0, None)];

    while let Some((id, depth, parent)) = stack.pop() {
        let index = visits.len();
        let relative = match parent {
            None => String::new(),
            Some(parent_index) => {
                let Some(info) = files.info(id) else {
                    continue;
                };
                let prefix = visits
                    .get(parent_index)
                    .map_or("", |visit| visit.relative.as_str());
                if prefix.is_empty() {
                    info.name.to_owned()
                } else {
                    format!("{prefix}/{}", info.name)
                }
            },
        };

        let found = if relative.is_empty() {
            None
        } else {
            match_path(query, &relative)
        };
        visits.push(Visit {
            id,
            depth,
            parent,
            relative,
            score: found.map(|matched| matched.score),
            positions: found
                .map(|matched| matched.matched_in_name().to_vec())
                .unwrap_or_default(),
        });

        for &child in files.listed_children(id).iter().rev() {
            stack.push((child, depth + 1, Some(index)));
        }
    }

    visits
}

/// Which visits survive: the ones that matched, and every ancestor of one.
///
/// Walked in reverse, which is what makes one pass enough. The walk is
/// pre-order, so a node's index is always lower than its descendants' — by the
/// time the reverse sweep reaches a node, every node below it has already had
/// its say.
fn keep_flags(visits: &[Visit]) -> Vec<bool> {
    let mut keep = vec![false; visits.len()];
    for index in (0..visits.len()).rev() {
        let Some(visit) = visits.get(index) else {
            continue;
        };
        if visit.score.is_some() {
            if let Some(flag) = keep.get_mut(index) {
                *flag = true;
            }
        }
        if keep.get(index).copied().unwrap_or(false)
            && let Some(parent) = visit.parent
            && let Some(flag) = keep.get_mut(parent)
        {
            *flag = true;
        }
    }
    keep
}
