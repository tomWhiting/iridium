//! The undo tree flattened for a panel to draw, and a selection to browse it.
//!
//! Every face that shows the history draws the same picture: the root at the
//! top, time running downward, forks indenting their children, the active
//! undo/redo path distinguished from parked branches, and a selection that
//! survives the tree being re-linearized underneath it. None of that is
//! painting — it is the *shape* of the tree and the arithmetic of browsing it
//! — so it lives here, beside the tree itself, and a face contributes only
//! cells or pixels. Two faces with private copies of this walk is how their
//! panels drift apart the first time one of them is adjusted.
//!
//! [`linearize`] turns an [`UndoTreeSnapshot`] into rows; [`TreeViewSelection`]
//! holds which row is selected — by node id, not by index — and which slice of
//! the rows a bounded window shows.

use std::collections::{HashMap, HashSet};

use super::{UndoNodeId, UndoNodeInfo, UndoTreeSnapshot};

/// One row of the linearized tree: a node, its indent, and whether it lies on
/// the active undo/redo path.
#[derive(Debug)]
pub struct TreeViewRow<'a> {
    /// The node this row shows.
    pub node: &'a UndoNodeInfo,
    /// How many branch points lie between the root and this node.
    pub indent: usize,
    /// Whether plain undo/redo travels through this node.
    pub on_active_path: bool,
}

/// Linearizes the tree for display: root first, depth-first, children in
/// creation order, so time runs downward and a fork's branches sit under it.
///
/// The indent counts branch points, not depth — a two-hundred-edit straight
/// history stays flush left, and only genuine forks move text sideways.
#[must_use]
pub fn linearize(snapshot: &UndoTreeSnapshot) -> Vec<TreeViewRow<'_>> {
    let by_id: HashMap<&str, &UndoNodeInfo> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect();

    let active = active_path(snapshot, &by_id);

    let mut rows = Vec::with_capacity(snapshot.nodes.len());
    let mut stack: Vec<(&str, usize)> = vec![(snapshot.info.root_id.as_str(), 0)];
    while let Some((id, indent)) = stack.pop() {
        let Some(node) = by_id.get(id) else {
            continue;
        };
        rows.push(TreeViewRow {
            node,
            indent,
            on_active_path: active.contains(id),
        });
        let child_indent = if node.child_ids.len() > 1 {
            indent + 1
        } else {
            indent
        };
        // Reversed so the first child is popped first and appears first.
        for child in node.child_ids.iter().rev() {
            stack.push((child.as_str(), child_indent));
        }
    }
    rows
}

/// The node ids plain undo and redo travel through: the current node, its
/// ancestors, and the preferred-child descent below it.
fn active_path<'a>(
    snapshot: &'a UndoTreeSnapshot,
    by_id: &HashMap<&str, &'a UndoNodeInfo>,
) -> HashSet<&'a str> {
    let mut active = HashSet::new();
    let mut upward: Option<&str> = Some(snapshot.info.current_id.as_str());
    while let Some(id) = upward {
        if !active.insert(id) {
            break;
        }
        upward = by_id.get(id).and_then(|node| node.parent_id.as_deref());
    }
    let mut downward = by_id
        .get(snapshot.info.current_id.as_str())
        .and_then(|node| node.preferred_child_id.as_deref());
    while let Some(id) = downward {
        if !active.insert(id) {
            break;
        }
        downward = by_id
            .get(id)
            .and_then(|node| node.preferred_child_id.as_deref());
    }
    active
}

/// A selection browsing linearized tree rows through a bounded window.
///
/// The selection is a node id rather than a row index: a jump reshapes nothing
/// but a refresh re-linearizes the tree, and an index would silently point at
/// a different state after one. Until something is selected — or when the
/// selected node no longer exists — the selection follows the document's
/// current node.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TreeViewSelection {
    /// The selected node, or `None` to follow the document's current node.
    selected: Option<u64>,
    /// The first visible row: the scroll position of the list window.
    scroll: usize,
    /// How many rows the window last showed — the page the page keys hop.
    window: usize,
}

impl TreeViewSelection {
    /// A selection following the current node, with `window` as the page size
    /// until [`Self::follow`] learns the real one.
    ///
    /// A non-zero starting window matters: a page hop before the first paint
    /// must still move.
    #[must_use]
    pub const fn with_window(window: usize) -> Self {
        Self {
            selected: None,
            scroll: 0,
            window,
        }
    }

    /// Resets the selection to follow the current node, scrolled to the top.
    pub const fn reset(&mut self) {
        self.selected = None;
        self.scroll = 0;
    }

    /// The first visible row.
    #[must_use]
    pub const fn scroll(&self) -> usize {
        self.scroll
    }

    /// The rows one page hop moves, never zero.
    #[must_use]
    pub const fn page(&self) -> usize {
        if self.window == 0 { 1 } else { self.window }
    }

    /// The selected row, resolved against the rows on screen.
    ///
    /// Falls back to the current node's row when nothing was selected yet or
    /// the selected node no longer exists.
    #[must_use]
    pub fn selected_row(&self, rows: &[TreeViewRow<'_>]) -> Option<usize> {
        let explicit = self.selected.and_then(|wanted| {
            rows.iter()
                .position(|row| row.node.id.parse::<u64>().ok() == Some(wanted))
        });
        explicit.or_else(|| rows.iter().position(|row| row.node.is_current))
    }

    /// The selected row's node id, ready to jump to.
    #[must_use]
    pub fn selected_node(&self, rows: &[TreeViewRow<'_>]) -> Option<UndoNodeId> {
        let index = self.selected_row(rows)?;
        let row = rows.get(index)?;
        row.node.id.parse::<u64>().ok().map(UndoNodeId::from_u64)
    }

    /// Moves the selection by `delta` rows, clamping at both ends.
    ///
    /// `isize::MIN` and `isize::MAX` are honest arguments — they land on the
    /// root and the newest row, which is what `Home` and `End` mean.
    pub fn move_by(&mut self, rows: &[TreeViewRow<'_>], delta: isize) {
        let Some(last) = rows.len().checked_sub(1) else {
            return;
        };
        let current = self.selected_row(rows).unwrap_or(0).min(last);
        let target = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta.unsigned_abs()).min(last)
        };
        if let Some(row) = rows.get(target) {
            self.selected = row.node.id.parse::<u64>().ok();
        }
    }

    /// Slides the scroll window so the selection is inside `visible` rows,
    /// and remembers `visible` as the page size.
    ///
    /// Called from a face's paint pass, because painting is where the real
    /// window height is known.
    pub fn follow(&mut self, rows: &[TreeViewRow<'_>], visible: usize) {
        self.window = visible.max(1);
        if visible == 0 || rows.is_empty() {
            self.scroll = 0;
            return;
        }
        let selected = self.selected_row(rows).unwrap_or(0);
        if selected < self.scroll {
            self.scroll = selected;
        } else if selected >= self.scroll + visible {
            self.scroll = selected + 1 - visible;
        }
        self.scroll = self.scroll.min(rows.len().saturating_sub(visible));
    }
}

#[cfg(test)]
mod tests {
    use super::super::{UndoNodeInfo, UndoTreeInfo, UndoTreeSnapshot};
    use super::{TreeViewSelection, linearize};

    /// A node with the given links and no description.
    fn node(id: u64, parent: Option<u64>, children: &[u64], current: bool) -> UndoNodeInfo {
        UndoNodeInfo {
            id: id.to_string(),
            parent_id: parent.map(|parent| parent.to_string()),
            child_ids: children.iter().map(ToString::to_string).collect(),
            preferred_child_id: children.first().map(ToString::to_string),
            elapsed_ms: 0,
            description: None,
            is_current: current,
        }
    }

    /// A snapshot over `nodes` with the given root and current ids.
    fn snapshot(nodes: Vec<UndoNodeInfo>, root: u64, current: u64) -> UndoTreeSnapshot {
        let count = nodes.len();
        UndoTreeSnapshot {
            nodes,
            info: UndoTreeInfo {
                current_id: current.to_string(),
                root_id: root.to_string(),
                node_count: count,
                can_undo: false,
                can_redo: false,
                branch_count: 0,
            },
        }
    }

    /// Root → 1 → 2, current at 2.
    fn straight() -> UndoTreeSnapshot {
        snapshot(
            vec![
                node(0, None, &[1], false),
                node(1, Some(0), &[2], false),
                node(2, Some(1), &[], true),
            ],
            0,
            2,
        )
    }

    /// Root forks: 0 → 1 parked, 0 → 2 current.
    fn forked() -> UndoTreeSnapshot {
        let mut root = node(0, None, &[1, 2], false);
        root.preferred_child_id = Some("2".to_owned());
        snapshot(
            vec![
                root,
                node(1, Some(0), &[], false),
                node(2, Some(0), &[], true),
            ],
            0,
            2,
        )
    }

    #[test]
    fn a_straight_history_stays_flush_left_in_time_order() {
        let snapshot = straight();
        let rows = linearize(&snapshot);
        let ids: Vec<&str> = rows.iter().map(|row| row.node.id.as_str()).collect();
        assert_eq!(ids, ["0", "1", "2"], "root first, time downward");
        assert!(rows.iter().all(|row| row.indent == 0));
        assert!(rows.iter().all(|row| row.on_active_path));
    }

    #[test]
    fn a_fork_indents_its_children_and_parks_the_abandoned_branch() {
        let snapshot = forked();
        let rows = linearize(&snapshot);
        let ids: Vec<&str> = rows.iter().map(|row| row.node.id.as_str()).collect();
        assert_eq!(ids, ["0", "1", "2"], "children in creation order");
        assert_eq!(rows[0].indent, 0);
        assert_eq!(rows[1].indent, 1);
        assert_eq!(rows[2].indent, 1);
        assert!(rows[0].on_active_path, "the root is on the path");
        assert!(!rows[1].on_active_path, "the abandoned branch is parked");
        assert!(rows[2].on_active_path, "the current node is on the path");
    }

    #[test]
    fn the_active_path_descends_through_preferred_children() {
        let mut nodes = vec![
            node(0, None, &[1], false),
            node(1, Some(0), &[2], true),
            node(2, Some(1), &[], false),
        ];
        nodes[1].preferred_child_id = Some("2".to_owned());
        let snapshot = snapshot(nodes, 0, 1);
        let rows = linearize(&snapshot);
        assert!(
            rows.iter().all(|row| row.on_active_path),
            "the redo descent below the current node is active"
        );
    }

    #[test]
    fn the_selection_follows_the_current_node_until_told_otherwise() {
        let snapshot = straight();
        let rows = linearize(&snapshot);
        let selection = TreeViewSelection::with_window(12);
        assert_eq!(selection.selected_row(&rows), Some(2));
        assert_eq!(
            selection
                .selected_node(&rows)
                .map(super::UndoNodeId::as_u64),
            Some(2)
        );
    }

    #[test]
    fn motion_clamps_at_both_ends() {
        let snapshot = straight();
        let rows = linearize(&snapshot);
        let mut selection = TreeViewSelection::with_window(12);
        selection.move_by(&rows, 5);
        assert_eq!(selection.selected_row(&rows), Some(2), "clamped at the end");
        selection.move_by(&rows, isize::MIN);
        assert_eq!(selection.selected_row(&rows), Some(0), "home is the root");
        selection.move_by(&rows, -1);
        assert_eq!(
            selection.selected_row(&rows),
            Some(0),
            "clamped at the root"
        );
    }

    #[test]
    fn the_selection_is_a_node_id_and_survives_relinearizing() {
        let snapshot = forked();
        let rows = linearize(&snapshot);
        let mut selection = TreeViewSelection::with_window(12);
        selection.move_by(&rows, -1);
        assert_eq!(
            selection
                .selected_node(&rows)
                .map(super::UndoNodeId::as_u64),
            Some(1),
            "one up from the current row is the parked branch"
        );
        // The tree is linearized again — the selection still names node 1.
        let again = linearize(&snapshot);
        assert_eq!(
            selection
                .selected_node(&again)
                .map(super::UndoNodeId::as_u64),
            Some(1)
        );
    }

    #[test]
    fn the_window_follows_the_selection_and_learns_the_page() {
        let nodes: Vec<UndoNodeInfo> = (0..10_u64)
            .map(|id| {
                let parent = id.checked_sub(1);
                let children: &[u64] = if id < 9 { &[id + 1] } else { &[] };
                node(id, parent, children, id == 9)
            })
            .collect();
        let snapshot = snapshot(nodes, 0, 9);
        let rows = linearize(&snapshot);
        let mut selection = TreeViewSelection::with_window(12);

        selection.follow(&rows, 3);
        assert_eq!(selection.page(), 3, "the window is the page");
        assert_eq!(selection.scroll(), 7, "the current row is kept in view");

        selection.move_by(&rows, isize::MIN);
        selection.follow(&rows, 3);
        assert_eq!(selection.scroll(), 0, "the window slid back to the root");
    }

    #[test]
    fn empty_rows_answer_nothing_and_move_nowhere() {
        let mut selection = TreeViewSelection::with_window(12);
        assert_eq!(selection.selected_row(&[]), None);
        assert_eq!(selection.selected_node(&[]), None);
        selection.move_by(&[], 1);
        selection.follow(&[], 3);
        assert_eq!(selection.scroll(), 0);
        assert!(selection.page() >= 1, "a page hop must always move");
    }
}
