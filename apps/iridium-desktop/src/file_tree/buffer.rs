//! The editable buffer: the panel's rows, as loaded and as they now read.
//!
//! This is the bridge between what the panel draws and what [`super::plan`]
//! diffs, and it is **where the by-id rule is actually kept**. Each row's
//! origin is captured here, once, from the node the row was drawn from — while
//! the panel still holds its [`iridium_explorer::NodeId`]. Nothing downstream
//! re-matches rows to nodes by name, because nothing downstream can: `plan`
//! never sees a tree.
//!
//! # Whatever is on screen is what is edited
//!
//! Both row sources — the opened tree and the filtered view — hand over the
//! same thing, a node and a depth, so the buffer does not care which is
//! showing. That is what makes the panel editable while filtered without a
//! second code path: a filter narrows the rows, and the diff is therefore
//! scoped to them. A file that is not on screen is not in the buffer and
//! cannot be changed by editing it.
//!
//! # Nesting comes from indentation
//!
//! A row's parent is the nearest row above it at a shallower depth, which is
//! exactly what the eye reads off the screen. Typing a row under a folder puts
//! it in that folder because it is drawn there.
//!
//! # The rows are not public
//!
//! Editing goes through the methods below rather than through the vector,
//! because every one of them maintains the same invariant — a row's parent
//! index points at a row that comes before it — and an insertion or a removal
//! that shifted indices without repairing parents would reparent rows into
//! folders they were never drawn in. That failure is silent, and the operation
//! it produces is a rename into the wrong directory.

// Step 3 of #58, and nothing calls it yet — the panel's edit mode is the
// caller. Scoped to the non-test build because the tests below do exercise
// every item; when a real caller lands this expectation goes unfulfilled and
// the build warns, so it removes itself.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "step 3 of the editable file list; wired up by the panel's edit mode"
    )
)]

use std::path::PathBuf;

use super::plan::{EditedRow, Plan, Refusal, RowOrigin};

/// One row as the panel currently draws it.
///
/// Deliberately not an [`iridium_explorer::NodeId`]: by the time a row reaches
/// here its identity has already been resolved to the name and path it had,
/// and keeping the id would invite something downstream to look it up again
/// later, against a tree that has since moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    /// How far the row is indented, the root at zero.
    pub depth: usize,
    /// The name the node has right now.
    pub name: String,
    /// Where the node is right now.
    pub path: PathBuf,
    /// Whether the node is a directory, so a row typed under it nests.
    pub directory: bool,
}

/// The editable buffer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Buffer {
    /// The rows as they now read, which is what gets planned.
    rows: Vec<EditedRow>,
    /// The rows exactly as loaded.
    ///
    /// Kept so that "has this been edited" is a comparison rather than a flag
    /// somebody has to remember to set. A flag drifts; a comparison cannot.
    loaded: Vec<EditedRow>,
}

impl Buffer {
    /// Builds a buffer from the rows the panel is drawing.
    #[must_use]
    pub fn load(source: &[SourceRow]) -> Self {
        let rows = rows_from(source);
        Self {
            loaded: rows.clone(),
            rows,
        }
    }

    /// The rows as they now read.
    #[must_use]
    pub fn rows(&self) -> &[EditedRow] {
        &self.rows
    }

    /// Whether anything has been changed since it loaded.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.rows != self.loaded
    }

    /// Throws away every edit, restoring what loaded.
    pub fn discard(&mut self) {
        self.rows.clone_from(&self.loaded);
    }

    /// What the buffer now says, as operations, or every reason it says
    /// nothing that can be carried out.
    ///
    /// # Errors
    ///
    /// One [`Refusal`] per row that cannot be acted on; see [`super::plan`].
    pub fn plan(&self) -> Result<Plan, Vec<Refusal>> {
        super::plan::plan(&self.rows)
    }

    /// Renames the row at `index`, if there is one.
    ///
    /// A trailing `/` is oil's way of saying "a folder", and it is the only
    /// way to say it — so it is read here and stripped, and the plan only ever
    /// sees names. Stripping it for a row that already exists too, because the
    /// kind of an existing path is whatever it is on disk and a slash typed
    /// after its name is not a request to change that.
    pub fn rename(&mut self, index: usize, name: &str) {
        let Some(row) = self.rows.get_mut(index) else {
            return;
        };
        let directory = name.ends_with('/');
        name.strip_suffix('/')
            .unwrap_or(name)
            .clone_into(&mut row.name);
        if row.origin.is_none() {
            row.directory = directory;
        }
    }

    /// Strikes the row at `index` through, or unstrikes it.
    ///
    /// A toggle rather than a removal, and that is the point: nothing is
    /// destroyed until the whole buffer is applied, so a delete pressed by
    /// mistake is undone by pressing it again rather than by reloading.
    pub fn toggle_deleted(&mut self, index: usize) {
        if let Some(row) = self.rows.get_mut(index) {
            row.deleted = !row.deleted;
        }
    }

    /// Inserts a new, empty row directly below `index`, nested to match.
    ///
    /// A row typed under an *open folder* belongs inside it; a row typed under
    /// a file belongs beside that file. That is what the eye reads from the
    /// indentation, and building the nesting any other way would put the new
    /// file somewhere other than where it appears.
    ///
    /// Returns where the new row landed.
    pub fn insert_below(&mut self, index: usize, into_folder: bool) -> usize {
        let (parent, at) = match self.rows.get(index) {
            Some(_) if into_folder => (Some(index), index + 1),
            Some(row) => (row.parent, index + 1),
            // An empty buffer: the row can only be a child of nothing, which
            // the planner refuses. The panel always has a root row, so this is
            // unreachable in practice and is not worth a panic.
            None => (None, self.rows.len()),
        };
        self.rows.insert(
            at,
            EditedRow {
                origin: None,
                name: String::new(),
                parent,
                deleted: false,
                directory: false,
            },
        );
        // Everything after the insertion point moved down one, so every parent
        // index naming a row at or after it names a row that has moved.
        for row in &mut self.rows[at + 1..] {
            if let Some(parent) = row.parent.as_mut()
                && *parent >= at
            {
                *parent += 1;
            }
        }
        at
    }

    /// Removes a typed row that was never on disk, and everything drawn inside
    /// it.
    ///
    /// Only ever a row the user added: an existing file is struck through, not
    /// removed, so that the buffer keeps saying what is there. **A typed row's
    /// descendants are always typed** — rows loaded from the tree are parented
    /// by the indentation they arrived with, and nothing reparents them under
    /// something that was never on disk — so taking the subtree can never take
    /// an existing file's row with it.
    ///
    /// Removing the row alone would be worse than wrong: its children's parent
    /// indices would survive it and, after the shift, name whichever row landed
    /// in that slot.
    pub fn remove_typed(&mut self, index: usize) -> bool {
        if self.rows.get(index).is_none_or(|row| row.origin.is_some()) {
            return false;
        }

        // Parents always precede their children, so one forward pass marks the
        // whole subtree.
        let mut removed = vec![false; self.rows.len()];
        removed[index] = true;
        for position in index + 1..self.rows.len() {
            if let Some(parent) = self.rows[position].parent
                && removed.get(parent).copied().unwrap_or(false)
            {
                removed[position] = true;
            }
        }

        // Where each surviving row lands once the removed ones are gone.
        let mut moved_to: Vec<Option<usize>> = vec![None; self.rows.len()];
        let mut next = 0;
        for (position, gone) in removed.iter().enumerate() {
            if !gone {
                moved_to[position] = Some(next);
                next += 1;
            }
        }

        let mut kept = Vec::with_capacity(next);
        for (position, row) in self.rows.drain(..).enumerate() {
            if removed[position] {
                continue;
            }
            // A survivor's parent survived too — a removed parent takes its
            // children with it — so this only ever renumbers. `and_then`
            // rather than an index because a wrong answer here must not be a
            // panic in a file manager.
            let parent = row.parent.and_then(|parent| moved_to[parent]);
            kept.push(EditedRow { parent, ..row });
        }
        self.rows = kept;
        true
    }
}

/// Turns drawn rows into editable ones, deriving each row's parent from the
/// indentation above it.
fn rows_from(source: &[SourceRow]) -> Vec<EditedRow> {
    // The rows still open above the one being read, shallowest first. A row's
    // parent is the nearest row above it at a shallower depth, so the stack is
    // unwound down to that row and its top is the answer.
    //
    // Holding depths rather than indexing a vector *by* depth is what makes
    // this independent of the depths arriving contiguously. Both row sources
    // do produce contiguous depths today — the tree draws an expansion, and
    // the filter keeps every ancestor of a hit — but a lookup table indexed by
    // depth answers a jump by silently attaching the row to the wrong parent,
    // and a wrong parent here is a rename into the wrong directory.
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut rows = Vec::with_capacity(source.len());

    for (index, row) in source.iter().enumerate() {
        while open.last().is_some_and(|&(depth, _)| depth >= row.depth) {
            open.pop();
        }
        let parent = open.last().map(|&(_, above)| above);
        open.push((row.depth, index));

        rows.push(EditedRow {
            origin: Some(RowOrigin {
                name: row.name.clone(),
                path: row.path.clone(),
            }),
            name: row.name.clone(),
            parent,
            deleted: false,
            directory: row.directory,
        });
    }

    rows
}
