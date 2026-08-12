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
//! | `Ctrl+Alt+B`, `⌘B` | Move between a floating panel and a sidebar |
//! | `Escape` | Clear the query, or close when there is none |
//! | `Enter` | Open a file; toggle or reveal a folder |
//! | `↑` `↓`, `Ctrl+P` `Ctrl+N` | Move the selection |
//! | `←` `→` | Collapse and expand — the unfiltered tree only |
//! | `⌘↓`, `Ctrl+↓` | Make the selected folder the root |
//! | `⌘↑`, `Ctrl+↑` | Make the folder above the root the root |
//! | `⌘.`, `Ctrl+.` | Show dot-prefixed entries, or stop showing them |
//! | `Home` `End` | First and last row |
//! | `Backspace`, any printable key | Edit the query |
//! | `Tab` | Edit the rows as text |
//!
//! # The table above is the *browse* table, and it is not the whole panel
//!
//! ⛔ Once the rows are being edited, every key means something else — and it
//! has to, because the row under the cursor is a text field and the arm at the
//! bottom of this table swallows every printable character into the query.
//! [`super::edit_keys`] is that second table, and [`FileExplorer::handle_key`]
//! reaches it **before** the match below rather than through four more arms
//! inside it. Adding edit-mode bindings to the table here would be the one
//! shape that cannot work.

use iridium_editor::pattern::Pattern;
use iridium_editor::{KeyCode, KeyEvent};
use iridium_explorer::NodeId;

use super::filter::FilterView;
use super::panel::{Chord, ExplorerOutcome, FileExplorer, chord};
use crate::prompt::Entry;

impl FileExplorer {
    /// Handles one key press. Every key is consumed; see [`ExplorerOutcome`].
    pub fn handle_key(&mut self, event: &KeyEvent) -> ExplorerOutcome {
        // ⛔ Before the match, not inside it. `(Plain, Char(_))` below takes
        // every printable character into the filter, and in edit mode those
        // characters are what the user is typing into a filename. A branch
        // ordered after that arm would be a branch the letters never reach.
        if self.mode.is_editing() {
            return self.handle_edit_key(event);
        }
        match (chord(event.modifiers), event.key) {
            (Chord::CtrlAlt | Chord::MetaAlt, KeyCode::Char('e' | 'E')) => ExplorerOutcome::Closed,
            // The sidebar chord, named here for the reason the close chord is:
            // the catch-all below consumes every key, so a chord this panel
            // does not name never reaches the host command it belongs to. `⌘B`
            // without `⌥` because that is what VS Code and Zed both bind.
            (Chord::CtrlAlt | Chord::Meta, KeyCode::Char('b' | 'B')) => {
                ExplorerOutcome::TogglePlacement
            },
            // A query is the first thing `Escape` takes back, and the panel
            // the second. Closing a panel someone has just typed into throws
            // away the narrowing and the panel in one press, and the second
            // press costs nothing.
            (Chord::Plain, KeyCode::Escape) => {
                if self.is_filtering() {
                    self.clear_query();
                    ExplorerOutcome::Handled
                } else {
                    ExplorerOutcome::Dismissed
                }
            },
            (Chord::Plain, KeyCode::Enter) => self.activate(),
            // Tab is the ruled key into edit mode, and it is bound here rather
            // than in `edit_keys` because it is the one of the four that is
            // pressed while still browsing. Nothing else in this panel uses
            // it, and there is no focus ring for it to walk.
            (Chord::Plain, KeyCode::Tab) => self.begin_editing(),
            (Chord::Plain, KeyCode::Up) | (Chord::Ctrl, KeyCode::Char('p' | 'P')) => {
                self.move_up();
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Down) | (Chord::Ctrl, KeyCode::Char('n' | 'N')) => {
                self.move_down();
                ExplorerOutcome::Handled
            },
            // Re-rooting. `⌘↓` and `⌘↑` are what macOS itself binds for
            // "open this folder" and "enclosing folder", so the pair needs no
            // learning; `Ctrl` is the same pair for a keyboard without a
            // command key. **Not `Enter`**, which is a question still open
            // with Tom — binding these separately means whatever he rules for
            // `Enter` only adds a second way in, and takes nothing away.
            (Chord::Meta | Chord::Ctrl, KeyCode::Down) => self.root_at_selection(),
            (Chord::Meta | Chord::Ctrl, KeyCode::Up) => self.root_above(),
            // `.` for dotfiles, which is the mnemonic every file manager that
            // has this key already uses. **Not `⌘H`**, which macOS takes to
            // hide the application before any window sees it — a binding the
            // user would experience as the editor vanishing. `.` is also
            // unshifted on every layout this face runs on, which matters
            // because [`Chord`] deliberately does not distinguish Shift.
            (Chord::Meta | Chord::Ctrl, KeyCode::Char('.')) => self.toggle_hidden(),
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
                    self.requery();
                }
                ExplorerOutcome::Handled
            },
            (Chord::Plain, KeyCode::Char(character)) => {
                if self.query.insert(character) {
                    self.requery();
                }
                ExplorerOutcome::Handled
            },
            // Modal: everything else is swallowed, not passed to the
            // document.
            _ => ExplorerOutcome::Handled,
        }
    }

    /// Makes the selected folder the root — "go in here".
    ///
    /// A file selected roots at the folder holding it, because that is the
    /// only reading of the key that does anything, and a key that looks dead
    /// half the time it is pressed is a key nobody trusts.
    fn root_at_selection(&mut self) -> ExplorerOutcome {
        let Some(directory) = self.selected_directory() else {
            return ExplorerOutcome::Handled;
        };
        self.reroot(&directory)
    }

    /// Makes the folder above the current root the root — "go up".
    ///
    /// Silently does nothing at the top of the filesystem, which is the one
    /// place there is genuinely nothing above. Saying so would be noise: the
    /// row showing the root is right there and already says where you are.
    fn root_above(&mut self) -> ExplorerOutcome {
        let Some(parent) = self.parent_of_root() else {
            return ExplorerOutcome::Handled;
        };
        self.reroot(&parent)
    }

    /// Starts editing the rows the panel is drawing.
    ///
    /// Refused in two states, and both refusals are about the same thing —
    /// what the buffer would be a snapshot *of*.
    ///
    /// A listing the panel has already asked for and not yet received
    /// ([`wanted`](FileExplorer::wanted)) means rows are on their way. Opening
    /// an editable session over a list that is still arriving would freeze
    /// half a directory into a buffer and leave the rest out of the diff, and
    /// the user has no way to tell which half they got.
    ///
    /// No rows at all is the other: [`super::plan`] requires the first row to
    /// be the folder the panel is showing, so a buffer built from nothing
    /// refuses everything typed into it. That happens for real — a query
    /// matching nothing produces an empty filtered view — and the honest
    /// answer is to say so rather than to open a session that cannot be
    /// applied.
    fn begin_editing(&mut self) -> ExplorerOutcome {
        if self.wanted.is_some() {
            return ExplorerOutcome::Failed(
                "still reading this folder — nothing to edit yet".to_owned(),
            );
        }
        let source = self.source_rows();
        if source.is_empty() {
            return ExplorerOutcome::Failed("there are no rows to edit".to_owned());
        }
        // ⚠️ Read *before* the mode changes. `selected_row` answers for the
        // cursor as soon as there is one, so asking after `begin_edit` would
        // ask the fresh session where it is and be told row zero — putting
        // every session on the root no matter what was selected.
        let start = self.selected_row().unwrap_or(0);
        if self.mode.begin_edit(&source) {
            // The row the user was looking at is the row they meant to edit.
            // `seat` clamps, so a filtered selection that outruns the buffer
            // lands on the last row rather than anywhere surprising.
            self.mode.seat(start);
        }
        ExplorerOutcome::Handled
    }

    /// Inserts pasted text into whichever field is taking characters.
    ///
    /// ⚠️ **Mode-aware, and it has to be.** The host intercepts the paste
    /// chord before [`Self::handle_key`] ever sees it — the panel is modal and
    /// has a text field, so a paste belongs to that field — which means the
    /// pre-match branch in `handle_key` cannot cover this path. Pasting into
    /// the query while a buffer is open would rebuild the filtered view, and
    /// that view is the row source the buffer was snapshotted from.
    ///
    /// The field refuses control characters, so a multi-line paste
    /// contributes its printable characters only.
    pub fn paste(&mut self, text: &str) {
        if self.mode.is_editing() {
            // Nothing to paste into while a refusal or a confirmation is on
            // screen: neither is showing the rows, so the characters would
            // land somewhere the user cannot see.
            if !self.mode.refusals().is_empty() {
                return;
            }
            for character in text.chars() {
                self.mode.edit_name(|entry| entry.insert(character));
            }
            return;
        }
        let mut changed = false;
        for character in text.chars() {
            changed |= self.query.insert(character);
        }
        if changed {
            self.requery();
        }
    }

    /// Drops the query and returns to the tree the user had open.
    fn clear_query(&mut self) {
        self.query = Entry::new();
        self.pattern = Pattern::Unfiltered;
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
    /// **Toggling is the ruling, not a placeholder.** Whether `Enter` on a
    /// directory should instead descend and re-root the panel was put to Tom
    /// on 7 Aug 2026 and answered: *"I thought we just had unfold"*. A folder
    /// unfolds; it never moves the ground under the selection. Re-rooting is
    /// not a thing this panel does, so nothing downstream should assume the
    /// selected row's ancestors can change under it.
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
