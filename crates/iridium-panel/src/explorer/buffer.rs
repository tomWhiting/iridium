//! The listing being edited: the rows the panel drew, written as text through
//! [`crate::outline`], and read back as the rows [`super::plan`] takes.
//!
//! Each row's origin is captured here, once, from the node it was drawn from,
//! and travels with its line through every edit. Nothing downstream matches a
//! row to a file by name.
//!
//! The folder the panel shows is not a line: it is the parent of every
//! top-level line, so it cannot be renamed or deleted by editing the text.

use std::fmt;
use std::path::PathBuf;

use iridium_explorer::NodeId;

use super::panel::FileExplorer;
use super::plan::{EditedRow, Plan, Refusal, RowOrigin};
use crate::outline::{LineState, Outline, OutlineEntry, ProjectionError};

impl FileExplorer {
    /// The rows the panel is drawing, root first, from whichever list shows.
    ///
    /// A filter narrows these, which is what scopes an edit to the rows on
    /// screen: a file the query left off cannot be changed by editing.
    pub(super) fn source_rows(&self) -> Vec<SourceRow> {
        if self.is_filtering() {
            return self
                .view
                .rows
                .iter()
                .filter_map(|row| self.source_row(row.id, row.depth))
                .collect();
        }
        (0..self.tree.len())
            .filter_map(|index| self.tree.row(index))
            .filter_map(|row| self.source_row(row.id, row.depth))
            .collect()
    }

    /// One drawn row, or nothing when the arena has no record of the node.
    ///
    /// A dropped row leaves a gap the planner refuses rather than a path
    /// computed against the wrong folder.
    fn source_row(&self, id: NodeId, depth: usize) -> Option<SourceRow> {
        let info = self.files.info(id)?;
        Some(SourceRow {
            depth,
            name: info.name.to_owned(),
            path: info.path.to_path_buf(),
            directory: info.kind.is_expandable(),
        })
    }
}

/// One row as the panel currently draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    /// How far the row is indented, the root at zero.
    pub depth: usize,
    /// The name the node has right now.
    pub name: String,
    /// Where the node is right now.
    pub path: PathBuf,
    /// Whether the node is a directory.
    pub directory: bool,
}

/// Why the rows on screen cannot be edited as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The rows do not start with the folder the panel is showing.
    NoRoot,
    /// A name no line of text can hold.
    Projection(ProjectionError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoRoot => formatter.write_str("clear the filter to edit the rows"),
            Self::Projection(error) => write!(formatter, "{error}, so these rows cannot be edited"),
        }
    }
}

/// The rows the planner reads, and the text line each one is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingRows {
    /// The root, then every non-blank line, then every entry no line holds,
    /// struck through.
    pub rows: Vec<EditedRow>,
    /// For each row, the line of text it is; `None` for the root and for an
    /// entry that has no line any more.
    pub lines: Vec<Option<usize>>,
}

/// The listing being edited.
#[derive(Debug, Clone)]
pub struct Listing {
    /// The folder the panel shows: every top-level line's parent.
    root: RowOrigin,
    /// The rows under it, as text.
    outline: Outline<RowOrigin>,
}

impl Listing {
    /// The listing for `source`, whose first row is the folder being shown.
    ///
    /// # Errors
    ///
    /// [`LoadError::NoRoot`] when the first row is not a depth-zero folder or
    /// a later row claims depth zero, and [`LoadError::Projection`] for a name
    /// that cannot be one line.
    pub fn load(source: &[SourceRow]) -> Result<Self, LoadError> {
        let Some((root, rows)) = source.split_first() else {
            return Err(LoadError::NoRoot);
        };
        if root.depth != 0 || !root.directory || rows.iter().any(|row| row.depth == 0) {
            return Err(LoadError::NoRoot);
        }
        let entries = rows
            .iter()
            .map(|row| OutlineEntry {
                depth: row.depth - 1,
                label: row.name.clone(),
                branch: row.directory,
                key: RowOrigin {
                    name: row.name.clone(),
                    path: row.path.clone(),
                },
            })
            .collect();
        Ok(Self {
            root: RowOrigin {
                name: root.name.clone(),
                path: root.path.clone(),
            },
            outline: Outline::project(entries).map_err(LoadError::Projection)?,
        })
    }

    /// The text the listing is edited as.
    #[must_use]
    pub fn text(&self) -> String {
        self.outline.text()
    }

    /// Follows the text to `lines`, reporting whether it changed.
    pub fn track(&mut self, lines: Vec<String>) -> bool {
        self.outline.track(lines)
    }

    /// How each line of the text compares with what was loaded.
    #[must_use]
    pub fn line_states(&self) -> Vec<LineState> {
        self.outline.line_states()
    }

    /// How many loaded entries no line holds any more.
    #[must_use]
    pub fn removed_count(&self) -> usize {
        self.outline.read().removed.len()
    }

    /// Whether the text says anything that would be applied.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.outline.is_edited()
    }

    /// What the text says, as operations, or every reason it cannot be done.
    ///
    /// # Errors
    ///
    /// One [`Refusal`] per row that cannot be acted on; see [`super::plan`].
    pub fn plan(&self) -> Result<Plan, Vec<Refusal>> {
        super::plan::plan(&self.rows().rows)
    }

    /// The text as the rows the planner reads.
    #[must_use]
    pub fn rows(&self) -> ListingRows {
        let reading = self.outline.read();
        let entries = self.outline.entries();
        let capacity = 1 + reading.lines.len() + reading.removed.len();
        let mut rows = Vec::with_capacity(capacity);
        let mut lines = Vec::with_capacity(capacity);
        rows.push(EditedRow {
            origin: Some(self.root.clone()),
            name: self.root.name.clone(),
            parent: None,
            deleted: false,
            directory: true,
        });
        lines.push(None);

        let mut entry_row: Vec<Option<usize>> = vec![None; entries.len()];
        for line in &reading.lines {
            if let Some(entry) = line.origin {
                entry_row[entry] = Some(rows.len());
            }
            rows.push(EditedRow {
                origin: line.origin.map(|entry| entries[entry].key.clone()),
                name: line.label.clone(),
                parent: Some(line.parent.map_or(0, |parent| parent + 1)),
                deleted: false,
                directory: line.branch,
            });
            lines.push(Some(line.line));
        }

        // An entry with no line is struck through where it was loaded. Loaded
        // order puts a folder before what it held, so its row already exists.
        for &entry in &reading.removed {
            let parent = self
                .outline
                .loaded_parent(entry)
                .map_or(Some(0), |loaded| entry_row[loaded]);
            entry_row[entry] = Some(rows.len());
            rows.push(EditedRow {
                origin: Some(entries[entry].key.clone()),
                name: entries[entry].label.clone(),
                parent,
                deleted: true,
                directory: entries[entry].branch,
            });
            lines.push(None);
        }
        ListingRows { rows, lines }
    }
}
