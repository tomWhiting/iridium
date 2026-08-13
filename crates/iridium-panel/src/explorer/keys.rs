//! What the explorer does with a key it has resolved.
//!
//! Split from `panel` by the question each method answers: everything here is
//! reached from [`FileExplorer::handle_key`], and a defect in it presents as
//! "the arrow went the wrong way" or "Enter opened the wrong thing" rather
//! than as a drawing or a timing fault.
//!
//! # ⭐ The chords are defaults now, not facts
//!
//! This module used to `match` on `(Chord, KeyCode)`. It matches on a resolved
//! [`Verb`] instead: the key is looked up in [`super::keymap`] — the panel's
//! default layer, with the user's `[keys]` bindings on top — and what arrives
//! here is *what the user asked for*, which may not be what the table below
//! ships with.
//!
//! The defaults, for the browsing screen:
//!
//! | default key | command |
//! |---|---|
//! | `Ctrl+Alt+E`, `⌘⌥E` | `explorer.togglePanel` |
//! | `Ctrl+Alt+B`, `⌘B` | `explorer.toggleSidebar` |
//! | `Escape` | `explorer.dismiss` — clear the query, or close |
//! | `Enter` | `explorer.activate` |
//! | `Tab` | `explorer.beginEdit` |
//! | `↑` `↓`, `Ctrl+P` `Ctrl+N` | `explorer.moveUp` / `explorer.moveDown` |
//! | `←` `→` | `explorer.collapse` / `explorer.expand` |
//! | `⌘↓`, `Ctrl+↓` | `explorer.rootAtSelection` |
//! | `⌘↑`, `Ctrl+↑` | `explorer.rootAbove` |
//! | `⌘.`, `Ctrl+.` | `explorer.toggleHidden` |
//! | `Home` `End` | `explorer.moveToFirst` / `explorer.moveToLast` |
//! | `Backspace` | `explorer.queryBackspace` |
//! | any other printable key | edits the query — a *field*, not a binding |
//!
//! # The panel is still modal
//!
//! Every key is consumed. A chord nothing binds is swallowed rather than passed
//! to the document: the explorer exists to aim at a file, and a keystroke
//! falling through to text the user is not looking at would edit it.
//!
//! # ⚠️ The editing screens are no longer reached by a branch before the match
//!
//! They used to have to be: the last arm here took every printable character
//! into the query, so a branch after it was a branch the letters never reached.
//! With resolution first that inversion is gone — a character typed into a
//! filename is simply an *unclaimed key in `explorer.edit` mode*, and the
//! screen decides where an unclaimed key goes. [`super::edit_keys`] holds the
//! editing screens' dispatch.

use iridium_editor::commands::builtin::{EXPLORER_TOGGLE_PANEL, EXPLORER_TOGGLE_SIDEBAR};
use iridium_editor::pattern::Pattern;
use iridium_editor::{CommandId, KeyCode, KeyEvent};
use iridium_explorer::NodeId;

use super::filter::{FilterRow, FilterView};
use super::panel::{ExplorerOutcome, FileExplorer};
use super::resolve::{Resolved, types_text};
use super::verb::Verb;
use crate::Entry;

impl FileExplorer {
    /// Handles one key press. Every key is consumed; see [`ExplorerOutcome`].
    ///
    /// ⭐ **Resolution first, then the screen.** The key is matched against the
    /// panel's keymap in the mode naming whichever screen is showing, and only
    /// what nothing claimed reaches the field below. That order is what lets a
    /// `[keys]` line move any of these keys — and it is also why the editing
    /// screens no longer need a branch *before* the match: a character typed
    /// into a filename is an unclaimed key in `explorer.edit` mode, and the
    /// screen decides where an unclaimed key goes.
    pub fn handle_key(&mut self, event: &KeyEvent) -> ExplorerOutcome {
        let editing = self.mode.is_editing();
        let resolved = self.resolve_key(event);
        if editing {
            return self.dispatch_edit(&resolved, event);
        }
        match resolved {
            Resolved::Command(id) => self.dispatch_browse(&id),
            // A half-typed sequence has run nothing yet, and a key that
            // abandoned one was typed as a chord rather than as text — neither
            // belongs in the query.
            // ⛔ `Suppressed` belongs here and not with `Unclaimed`: the key was
            // claimed by a binding that names nothing, so sending it on would
            // filter — or rename — by a character the user unbound.
            Resolved::Pending | Resolved::Abandoned | Resolved::Suppressed => {
                ExplorerOutcome::Handled
            },
            Resolved::Unclaimed => self.unclaimed_browse_key(event),
        }
    }

    /// Runs a command resolved while the panel is browsing.
    ///
    /// The two toggles are answered here rather than in [`Verb`] because they
    /// are the editor's commands, not the panel's; the panel names them only
    /// because it is modal and would otherwise swallow them before the face
    /// could act.
    fn dispatch_browse(&mut self, id: &CommandId) -> ExplorerOutcome {
        if id == &EXPLORER_TOGGLE_PANEL {
            return ExplorerOutcome::Closed;
        }
        if id == &EXPLORER_TOGGLE_SIDEBAR {
            return ExplorerOutcome::ToggleSidebar;
        }
        let Some(verb) = Verb::from_id(id) else {
            // A user's layer may name a command this panel has never heard of.
            // Modal: swallowed, not passed to the document.
            return ExplorerOutcome::Handled;
        };
        match verb {
            // A query is the first thing this takes back, and the panel the
            // second. Closing a panel someone has just typed into throws away
            // the narrowing and the panel in one press, and the second press
            // costs nothing.
            Verb::Dismiss => {
                if self.is_filtering() {
                    self.clear_query();
                    ExplorerOutcome::Handled
                } else {
                    ExplorerOutcome::Dismissed
                }
            },
            Verb::Activate => self.activate(),
            Verb::BeginEdit => self.begin_editing(),
            Verb::MoveUp => {
                self.move_up();
                ExplorerOutcome::Handled
            },
            Verb::MoveDown => {
                self.move_down();
                ExplorerOutcome::Handled
            },
            Verb::MoveToFirst => {
                self.move_to_first();
                ExplorerOutcome::Handled
            },
            Verb::MoveToLast => {
                self.move_to_last();
                ExplorerOutcome::Handled
            },
            // ⚠️ The filtered guard is a property of the *screen*, not of the
            // binding, so it stays here rather than becoming a second mode. A
            // filtered view has no expansion state to walk — every folder in it
            // is already open — so these are swallowed there rather than
            // repurposed into something the same key does one keystroke
            // earlier.
            Verb::Expand => {
                if !self.is_filtering() {
                    self.expand_or_descend();
                }
                ExplorerOutcome::Handled
            },
            Verb::Collapse => {
                if !self.is_filtering() {
                    self.tree.move_left();
                }
                ExplorerOutcome::Handled
            },
            Verb::RootAtSelection => self.root_at_selection(),
            Verb::RootAbove => self.root_above(),
            Verb::ToggleHidden => self.toggle_hidden(),
            Verb::QueryBackspace => {
                if self.query.backspace() {
                    self.requery();
                }
                ExplorerOutcome::Handled
            },
            // The editing verbs cannot be reached from the browse screen: they
            // are bound in the editing modes, and `screen_mode` was browse.
            // Swallowed rather than run, because running one would act on a
            // buffer that does not exist.
            _ => ExplorerOutcome::Handled,
        }
    }

    /// What a key nothing claimed means while browsing: it edits the query.
    ///
    /// ⛔ **The fall-through, kept as a fall-through.** Turning it into
    /// twenty-six bindings would make a keymap that cannot express a *field*.
    /// A user who binds a bare letter to a command takes that letter out of the
    /// query, which is the honest consequence of what they asked for — and is
    /// why resolution runs first.
    fn unclaimed_browse_key(&mut self, event: &KeyEvent) -> ExplorerOutcome {
        if let KeyCode::Char(character) = event.key
            && types_text(event.modifiers)
            && self.query.insert(character)
        {
            self.requery();
        }
        ExplorerOutcome::Handled
    }

    /// What a press on composed row `row` does.
    ///
    /// ⭐ **Through [`activate`](Self::activate) — the `Enter` path — not
    /// beside it.** A file opens, a folder toggles, a filtered folder is
    /// revealed: those are decisions about what a row *means*, and a click
    /// that answered them itself would be a second set of them to keep in step
    /// with the first. What the click adds is only *which* row, which is the
    /// one thing a key press never has to say.
    ///
    /// # The offsets, and why they are the panel's to know
    ///
    /// `row` is an index into the rows [`content`](Self::content) composed,
    /// which is the only thing the pointer can resolve against. Row zero is
    /// the query field while browsing and the label while editing, and neither
    /// is a list row; the rest are offset by the scroll window. The face knows
    /// none of that and must not have to.
    ///
    /// A confirmation or a refusal is showing neither list, and a press on one
    /// changes nothing: both are answered with the keys that mean it, and a
    /// stray click applying a plan would be the worst possible reading of a
    /// mouse.
    pub fn click_row(&mut self, row: usize) -> ExplorerOutcome {
        if self.mode.plan().is_some() || !self.mode.refusals().is_empty() {
            return ExplorerOutcome::Handled;
        }
        let Some(offset) = row.checked_sub(1) else {
            return ExplorerOutcome::Handled;
        };
        let index = self.scroll.saturating_add(offset);
        if index >= self.row_count() {
            return ExplorerOutcome::Handled;
        }
        if self.mode.is_editing() {
            // The row being pointed at is the row being typed into. Nothing is
            // activated: these rows are text, and a click in a text field puts
            // the cursor there rather than running something.
            self.mode.seat(index);
            return ExplorerOutcome::Handled;
        }
        if !self.select_row(index) {
            return ExplorerOutcome::Handled;
        }
        self.activate()
    }

    /// Puts the selection on list row `index`, reporting whether it went.
    ///
    /// A filtered view's folders-above-a-match rows are not selectable, and a
    /// press on one leaves the selection alone: those rows are context, and
    /// moving the selection onto one would leave `Enter` pointing at a row the
    /// panel refuses to act on.
    fn select_row(&mut self, index: usize) -> bool {
        if self.is_filtering() {
            if !self
                .view
                .rows
                .get(index)
                .is_some_and(FilterRow::is_selectable)
            {
                return false;
            }
            self.filtered = index;
            return true;
        }
        if self.tree.row(index).is_none() {
            return false;
        }
        self.tree.select(index);
        true
    }

    /// Moves the window `delta` rows without touching the selection.
    ///
    /// ⭐ **Without touching the selection** is the whole of it: this is a
    /// browsed panel, and looking further down a directory is not a decision
    /// about which file `Enter` opens. The keyboard moves the selection and
    /// the window follows; the wheel moves the window and the selection stays.
    ///
    /// Clamped at the top only. The bottom is clamped by `follow_selection` on
    /// the next composition, which is the one place that knows both how many
    /// rows there are and how many the window is showing — asking here would
    /// mean the panel carried a second, staler copy of both.
    pub const fn scroll_rows(&mut self, delta: isize) {
        self.scroll = if delta < 0 {
            self.scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.scroll.saturating_add(delta.unsigned_abs())
        };
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
        // The list the window was placed against is gone, so "already
        // following the selection" would mean following an index into it.
        self.followed = None;
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
