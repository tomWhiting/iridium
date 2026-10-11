//! Navigation survives a close; directory contents and Oil buffers do not.
use std::path::PathBuf;

use super::FileExplorer;
use crate::Entry;

/// An explorer location, independent of directory-reader node identities.
///
/// Hosts must check for unapplied edits before closing a panel. A bookmark
/// stores navigation only and always reopens in browsing mode.
#[derive(Debug, Clone)]
pub struct ExplorerBookmark {
    root: PathBuf,
    crawl: bool,
    hidden: bool,
    query: String,
    pub(super) navigation: Navigation,
}

#[derive(Debug, Clone)]
pub(super) struct Navigation {
    expanded: Vec<PathBuf>,
    selected: Option<PathBuf>,
    scroll: usize,
}

impl FileExplorer {
    /// Remembers the query, visible expanded folders and focused path.
    #[must_use]
    pub fn bookmark(&self) -> ExplorerBookmark {
        let navigation = self.restoring.clone().unwrap_or_else(|| Navigation {
            expanded: self
                .tree
                .rows()
                .iter()
                .filter(|row| row.expanded || self.wanted == Some(row.id))
                .filter_map(|row| self.files.info(row.id))
                .map(|info| info.path.to_path_buf())
                .collect(),
            selected: self.selected_path().map(std::path::Path::to_path_buf),
            scroll: self.scroll,
        });
        ExplorerBookmark {
            root: self.root_path(),
            crawl: self.crawl,
            hidden: self.files.show_hidden(),
            query: self.query.text().to_owned(),
            navigation,
        }
    }

    /// Opens fresh directory reads and restores navigation as they arrive.
    /// New input cancels pending restoration so it cannot steal focus later.
    /// User key bindings must be supplied by the host as with [`Self::open`].
    ///
    /// # Errors
    /// Returns the reader's error if it cannot start.
    pub fn open_bookmark(bookmark: &ExplorerBookmark) -> Result<Self, String> {
        let mut explorer = Self::open(bookmark.root.clone(), bookmark.crawl)?;
        explorer.files.set_show_hidden(bookmark.hidden);
        explorer.query = Entry::with_text(&bookmark.query);
        explorer.requery();
        explorer.wanted = None;
        explorer.restoring = Some(bookmark.navigation.clone());
        explorer.restore_navigation();
        Ok(explorer)
    }

    /// Expansions only use rows found in the new tree. Missing paths never
    /// become fabricated entries and never open unrelated replacement rows.
    pub(super) fn restore_navigation(&mut self) {
        let Some(navigation) = self.restoring.take() else {
            return;
        };
        for path in &navigation.expanded {
            if self.wanted.is_some() {
                break;
            }
            let Some(node) = self.files.node_at(path) else {
                continue;
            };
            let Some(row) = self.tree.index_of(&node) else {
                continue;
            };
            if !self.tree.is_expanded(&node)
                && self
                    .files
                    .info(node)
                    .is_some_and(|info| info.kind.is_expandable() && info.error.is_none())
            {
                self.open_row(row);
            }
        }
        if self.is_filtering() {
            self.read_selected_ancestors(&navigation);
            self.refilter(self.selected_node());
        }
        if let Some(node) = navigation
            .selected
            .as_ref()
            .and_then(|path| self.files.node_at(path))
        {
            let row = if self.is_filtering() {
                self.view
                    .rows
                    .iter()
                    .position(|row| row.id == node && row.is_selectable())
            } else {
                self.tree.index_of(&node)
            };
            if let Some(row) = row {
                if self.is_filtering() {
                    self.filtered = row;
                } else {
                    self.tree.select(row);
                }
                self.scroll = navigation.scroll;
                self.followed = Some(row);
            }
        }
        if self.is_waiting() {
            self.restoring = Some(navigation);
        }
    }
    /// A match can live beneath a collapsed folder that was read earlier.
    /// Restore that path even when general crawling is disabled, without
    /// changing the remembered tree expansion state or reading its siblings.
    fn read_selected_ancestors(&mut self, navigation: &Navigation) {
        let Some(path) = navigation.selected.as_ref() else {
            return;
        };
        for parent in path.ancestors().skip(1) {
            let Some(node) = self.files.node_at(parent) else {
                continue;
            };
            if !self.files.is_listed(node)
                && self.files.info(node).is_some_and(|info| {
                    info.kind.is_expandable() && !info.is_loading && info.error.is_none()
                })
            {
                self.files.reload(node);
            }
        }
    }
}
