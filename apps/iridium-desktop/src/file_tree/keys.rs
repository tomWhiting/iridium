//! What the explorer does with a key.
//!
//! Split from `panel` by the question each method answers: everything here is
//! reached from [`FileExplorer::handle_key`], and a defect in it presents as
//! "the arrow went the wrong way" or "Enter opened the wrong thing" rather
//! than as a drawing or a timing fault.
//!
//! # The panel is modal
//!
//! Every key is consumed. A chord this module does not bind is swallowed
//! rather than passed to the document: the explorer exists to aim at a file,
//! and a keystroke falling through to text the user is not looking at would
//! edit it.
//!
//! | Key | Action |
//! |---|---|
//! | `Ctrl+Alt+E`, `⌘⌥E` | Close |
//! | `Escape` | Clear the query, or close when there is none |
//! | `Enter` | Open a file; toggle or reveal a folder |
//! | `↑` `↓`, `Ctrl+P` `Ctrl+N` | Move the selection |
//! | `←` `→` | Collapse and expand — the unfiltered tree only |
//! | `Home` `End` | First and last row |
//! | `Backspace`, any printable key | Edit the query |

use iridium_editor::{KeyCode, KeyEvent};
use iridium_explorer::NodeId;

use super::filter::FilterView;
use super::panel::{Chord, ExplorerOutcome, FileExplorer, chord};
use crate::prompt::Entry;

impl FileExplorer {
    /// Handles one key press. Every key is consumed; see [`ExplorerOutcome`].
    pub fn handle_key(&mut self, event: &KeyEvent) -> ExplorerOutcome {
        match (chord(event.modifiers), event.key) {
            (Chord::CtrlAlt | Chord::MetaAlt, KeyCode::Char('e' | 'E')) => ExplorerOutcome::Closed,
            // A query is the first thing `Escape` takes back, and the panel
            // the second. Closing a panel someone has just typed into throws
            // away the narrowing and the panel in one press, and the second
            // press costs nothing.
            (Chord::Plain, KeyCode::Escape) => {
                if self.is_filtering() {
                    self.clear_query();
                    ExplorerOutcome::Handled
                } else {
                    ExplorerOutcome::Closed
                }
            },
            (Chord::Plain, KeyCode::Enter) => self.activate(),
            (Chord::Plain, KeyCode::Up) | (Chord::Ctrl, KeyCode::Char('p' | 'P')) => {
                self.move_up();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Down) | (Chord::Ctrl, KeyCode::Char('n' | 'N')) => {
                self.move_down();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Home) => {
                self.move_to_first();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::End) => {
                self.move_to_last();
                ExplorerOutcome::Handled
            },
            // `←` and `→` are the tree's. A filtered view has no expansion
            // state to walk — every folder in it is already open — so they
            // are swallowed there rather than repurposed into something the
            // same key does not do one keystroke earlier.
            (Chord::Plain, KeyCode::Right) if !self.is_filtering() => {
                self.expand_or_descend();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Left) if !self.is_filtering() => {
                self.tree.move_left();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Backspace) => {
                if self.query.backspace() {
                    self.refilter(None);
                }
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Char(character)) => {
                if self.query.insert(character) {
                    self.refilter(None);
                }
                ExplorerOutcome::Handled
            },
            // Modal: everything else is swallowed, not passed to the
            // document.
            _ => ExplorerOutcome::Handled,
        }
    }

    /// Inserts pasted text into the query, one character at a time.
    ///
    /// The field refuses control characters, so a multi-line paste
    /// contributes its printable characters only.
    pub fn paste(&mut self, text: &str) {
        let mut changed = false;
        for character in text.chars() {
            changed |= self.query.insert(character);
        }
        if changed {
            self.refilter(None);
        }
    }

    /// Drops the query and returns to the tree the user had open.
    fn clear_query(&mut self) {
        self.query = Entry::new();
        self.view = FilterView::default();
        self.filtered = 0;
        self.scroll = 0;
    }

    /// Moves the selection one row towards the top of whichever list shows.
    fn move_up(&mut self) {
        if !self.is_filtering() {
            self.tree.move_up();
            return;
        }
        if let Some(above) = self.filtered.checked_sub(1)
            && let Some(index) = self.view.previous_selectable(above)
        {
            self.filtered = index;
        }
    }

    /// Moves the selection one row towards the bottom.
    fn move_down(&mut self) {
        if !self.is_filtering() {
            self.tree.move_down();
            return;
        }
        if let Some(index) = self.view.next_selectable(self.filtered.saturating_add(1)) {
            self.filtered = index;
        }
    }

    /// Selects the first row that can be selected.
    fn move_to_first(&mut self) {
        if self.is_filtering() {
            self.filtered = self.view.next_selectable(0).unwrap_or(self.filtered);
        } else {
            self.tree.move_to_first();
        }
    }

    /// Selects the last row that can be selected.
    fn move_to_last(&mut self) {
        if !self.is_filtering() {
            self.tree.move_to_last();
            return;
        }
        let Some(last) = self.view.rows.len().checked_sub(1) else {
            return;
        };
        self.filtered = self.view.previous_selectable(last).unwrap_or(self.filtered);
    }

    /// What `Right` does: open a closed node, or step into an open one.
    ///
    /// Split from [`iridium_tree::Tree::move_right`] only for the closed
    /// case, which has to go through [`open_row`](Self::open_row) rather
    /// than expanding a node whose children may not have arrived.
    fn expand_or_descend(&mut self) {
        let Some(index) = self.tree.selected() else {
            return;
        };
        if self
            .tree
            .row(index)
            .is_some_and(iridium_tree::Row::is_openable)
        {
            self.open_row(index);
        } else {
            self.tree.move_right(&mut self.files);
        }
    }

    /// What `Enter` does to the selected row.
    ///
    /// A file opens, from either list. A directory toggles in the tree, and
    /// in a filtered view is *revealed* instead — the query is dropped and
    /// the tree is opened down to it, which is the only reading of "enter
    /// this folder" that leaves the panel somewhere the user can carry on
    /// from.
    ///
    /// **The tree half is the placeholder of an undecided question** —
    /// whether `Enter` on a directory should instead descend, re-rooting the
    /// panel, is Tom's to rule on. Toggling is the answer that cannot
    /// surprise anyone in the meantime: it is what `Right` already does, and
    /// it never moves the ground under the selection.
    fn activate(&mut self) -> ExplorerOutcome {
        if self.is_filtering() {
            return self.activate_filtered();
        }
        let Some(index) = self.tree.selected() else {
            return ExplorerOutcome::Handled;
        };
        let Some(id) = self.tree.row(index).map(|row| row.id) else {
            return ExplorerOutcome::Handled;
        };
        let Some(info) = self.files.info(id) else {
            return ExplorerOutcome::Handled;
        };
        if info.kind.is_expandable() {
            if self.tree.row(index).is_some_and(|row| row.expanded) {
                self.tree.collapse(index);
            } else {
                self.open_row(index);
            }
            return ExplorerOutcome::Handled;
        }
        ExplorerOutcome::Open(info.path.to_path_buf())
    }

    /// `Enter` on a filtered row.
    fn activate_filtered(&mut self) -> ExplorerOutcome {
        let Some(row) = self.view.rows.get(self.filtered) else {
            return ExplorerOutcome::Handled;
        };
        if !row.is_selectable() {
            return ExplorerOutcome::Handled;
        }
        let id = row.id;
        let Some(info) = self.files.info(id) else {
            return ExplorerOutcome::Handled;
        };
        if !info.kind.is_expandable() {
            return ExplorerOutcome::Open(info.path.to_path_buf());
        }
        self.reveal(id);
        ExplorerOutcome::Handled
    }

    /// Drops the query and opens the tree down to `id`, selecting it.
    ///
    /// `id` itself is opened too when it is a directory, because that is what
    /// entering a folder means — landing on it closed would have thrown away
    /// the query for a row the user then has to open by hand.
    ///
    /// Safe to expand every step of the way without the deferred-intent
    /// dance: a node is in the arena because it appeared in its parent's
    /// listing, so every ancestor of anything the filter found is already
    /// listed. The guard is kept anyway — [`open_row`](Self::open_row) is
    /// the only expansion path in this module, and a second one would be a
    /// second place for rule four to bite.
    fn reveal(&mut self, id: NodeId) {
        self.clear_query();

        let mut chain = vec![id];
        let mut current = self.files.parent(id);
        while let Some(node) = current {
            chain.push(node);
            current = self.files.parent(node);
        }
        // Root first: a child has no row to find until its parent is open.
        chain.reverse();

        for node in chain {
            let Some(index) = self.tree.index_of(&node) else {
                continue;
            };
            let openable = self
                .tree
                .row(index)
                .is_some_and(|row| row.has_children && !row.expanded);
            if openable {
                self.open_row(index);
            }
        }
        if let Some(index) = self.tree.index_of(&id) {
            self.tree.select(index);
        }
    }
}
