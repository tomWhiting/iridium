//! A selected-text search scope follows committed edits and history nodes.
use std::{collections::HashMap, ops::Range};

use super::SearchState;
use crate::{Command, Document, Selection, history::UndoNodeId};

mod tracking;

pub(super) const STALE_SCOPE: &str =
    "Search selection changed outside tracked edits. Select the scope again or clear it.";

#[derive(Clone, Debug)]
pub(super) struct SearchScope {
    ranges: Vec<Range<usize>>,
    revision: u64,
    valid: bool,
    snapshots: HashMap<UndoNodeId, Vec<Range<usize>>>,
}

impl SearchScope {
    pub(super) fn invalidate(&mut self) {
        self.valid = false;
        self.snapshots.clear();
    }
    pub(super) const fn is_current(&self, document: &Document) -> bool {
        self.valid && self.revision == document.revision()
    }

    fn contains(&self, start: usize, end: usize) -> bool {
        let index = self.ranges.partition_point(|range| range.start <= start);
        index
            .checked_sub(1)
            .and_then(|index| self.ranges.get(index))
            .is_some_and(|range| end <= range.end)
    }
}

impl SearchState {
    /// Whether an untracked mutation invalidated the selected-text scope.
    #[must_use]
    pub fn scope_is_stale(&self) -> bool {
        self.scope.as_ref().is_some_and(|scope| !scope.valid)
    }
    /// Number of retained selection ranges, or None for a whole-document search.
    /// An invalid scope remains present and yields an explicit search diagnostic.
    #[must_use]
    pub fn scope_range_count(&self) -> Option<usize> {
        self.scope.as_ref().map(|scope| scope.ranges.len())
    }

    pub(super) fn within_scope(&self, start: usize, end: usize) -> bool {
        self.scope
            .as_ref()
            .is_none_or(|scope| scope.contains(start, end))
    }

    pub(crate) fn capture_scope(
        &mut self,
        selections: impl Iterator<Item = Selection>,
        document: &Document,
        node: UndoNodeId,
    ) -> Result<(), String> {
        let mut ranges = Vec::new();
        for selection in selections.filter(|selection| !selection.is_collapsed()) {
            let start = document
                .position_to_offset(selection.start())
                .ok_or("Invalid scope start")?;
            let end = document
                .position_to_offset(selection.end())
                .ok_or("Invalid scope end")?;
            ranges.push(start..end);
        }
        if ranges.is_empty() {
            return Err("Select some text before searching in selections".to_owned());
        }
        normalize(&mut ranges);
        let snapshots = HashMap::from([(node, ranges.clone())]);
        self.scope = Some(SearchScope {
            ranges,
            revision: document.revision(),
            valid: true,
            snapshots,
        });
        Ok(())
    }

    /// Tracks only after command application succeeds. Unknown mutations keep
    /// the scope invalid, never silently falling back to the whole document.
    pub(crate) fn track_scope(
        &mut self,
        before: &Document,
        commands: &[Command],
        after: &Document,
        target: Option<UndoNodeId>,
    ) {
        let Some(scope) = &mut self.scope else {
            return;
        };
        if !scope.is_current(before) {
            scope.valid = false;
            scope.snapshots.clear();
            return;
        }
        if let Some(ranges) = target.and_then(|node| scope.snapshots.get(&node)) {
            scope.ranges.clone_from(ranges);
            scope.revision = after.revision();
            return;
        }
        match tracking::map_ranges(&scope.ranges, before, commands) {
            Ok(ranges) => {
                scope.ranges = ranges;
                scope.revision = after.revision();
                if let Some(node) = target {
                    scope.snapshots.insert(node, scope.ranges.clone());
                }
            },
            Err(error) => {
                tracing::error!(%error, "committed edit could not update search scope");
                scope.valid = false;
                scope.snapshots.clear();
            },
        }
    }

    pub(crate) fn remember_scope(&mut self, node: UndoNodeId) {
        if let Some(scope) = self.scope.as_mut().filter(|scope| scope.valid) {
            scope.snapshots.insert(node, scope.ranges.clone());
        }
    }
}

fn normalize(ranges: &mut Vec<Range<usize>>) {
    ranges.sort_unstable_by_key(|range| (range.start, range.end));
    let mut normalized: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges.drain(..) {
        if let Some(previous) = normalized
            .last_mut()
            .filter(|previous| range.start <= previous.end)
        {
            previous.end = previous.end.max(range.end);
        } else {
            normalized.push(range);
        }
    }
    *ranges = normalized;
}
