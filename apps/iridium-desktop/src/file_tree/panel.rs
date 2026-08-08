//! The explorer's state: what is open, what has been read, and what the
//! query is.
//!
//! The keys that change it live in [`super::keys`] and the rows that show it
//! in [`super::compose`]; the fields are `pub(super)` for exactly those two
//! and are not reachable from outside this module.

use std::path::{Path, PathBuf};

use iridium_editor::Modifiers;
use iridium_editor::pattern::Pattern;
use iridium_explorer::{FileTree, NodeId};
use iridium_tree::{Tree, TreeSource as _};

use super::filter::{FilterView, filter};
use crate::project::chosen_root;
use crate::prompt::Entry;

/// The query prompt, drawn before the field. The palette's, so the two panels
/// read as the same kind of thing.
pub(super) const PROMPT: &str = "> ";

/// How many directory reads the crawl posts per frame while a query is up.
///
/// A rate, not a total. The reads happen on the worker, so the number that
/// matters is not what this thread can afford but how many requests are worth
/// having in flight at once: enough that a project fills in over a second or
/// two of frames, few enough that the disk is not asked for a thousand
/// directories before the first answer comes back. At sixty frames a second
/// this reaches a thousand directories in about a third of a second.
const CRAWL_PER_FRAME: usize = 64;

/// What a key press did to the panel.
///
/// There is no `Ignored`: the panel is modal, and a key it does not bind is
/// swallowed rather than falling through to the document — the same rule the
/// undo tree follows, for the same reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplorerOutcome {
    /// The panel consumed the key and stays open.
    Handled,
    /// The panel was closed.
    Closed,
    /// This file should be opened in a tab. The host decides what that
    /// means; the panel does not touch the workspace.
    Open(PathBuf),
    /// The key was understood and could not be carried out, with something
    /// to say about it.
    ///
    /// The panel is modal, so every key is consumed. A key that was consumed
    /// and then did nothing at all is indistinguishable from a dead one, and
    /// re-rooting is the first thing here that can fail for a reason worth
    /// hearing.
    Failed(String),
}

/// The file explorer.
///
/// The host owns one of these, creates it when the kernel reports the
/// `explorer.togglePanel` host command, hands it every key while it is open,
/// polls it once per frame, and paints it over the composed frame.
///
/// # Two row sources, one panel
///
/// With an empty query the rows come from [`Tree`] — what the user has opened,
/// and only that. With a query they come from [`super::filter`], which walks
/// the arena and keeps the matches with their folders above them. The tree's
/// expansion state is untouched by filtering, so clearing the query restores
/// exactly the tree that was there before a character was typed.
#[derive(Debug)]
pub struct FileExplorer {
    /// The filesystem, read off the frame thread.
    pub(super) files: FileTree,
    /// What is open and what is therefore visible.
    pub(super) tree: Tree<FileTree>,
    /// The first row the window shows, counted in list rows — the query row
    /// is not part of it and never scrolls away.
    pub(super) scroll: usize,
    /// A node that was asked to open before its listing had landed, and
    /// opens as soon as it does.
    ///
    /// **This is the trap the panel exists around.** Expanding a node whose
    /// children have not arrived makes the tree record it as a leaf —
    /// `TreeSource`'s fourth rule says a node that promised children and
    /// produced none is treated as one from then on — so it would never open
    /// again. Every expansion therefore goes through
    /// [`open_row`](Self::open_row), which either opens a node that is
    /// already listed or defers to here. The root is the first user of it.
    ///
    /// One deep, not a set: a keyboard user opens one node at a time, and
    /// the intent that matters is the last one expressed.
    pub(super) wanted: Option<NodeId>,
    /// The filter query.
    ///
    /// Its caret never moves. The arrow keys in a tree panel belong to the
    /// tree, and a filter is three or four characters someone retypes rather
    /// than edits — so the field takes characters and `Backspace`, and
    /// nothing else. Giving it caret motions would cost `←` and `→`, which
    /// are how the tree is opened and closed.
    pub(super) query: Entry,
    /// [`Self::query`] read as the thing it means — fuzzy, or a regular
    /// expression after a leading `/`.
    ///
    /// **Held rather than parsed per filter, and that is load-bearing.**
    /// Compiling a regular expression costs orders of magnitude more than
    /// testing one string with it, and the rows are re-filtered every time a
    /// directory listing lands — dozens of times a second while the crawl
    /// runs. Parsing there would compile the same pattern once per read.
    /// Rebuilt by [`requery`](Self::requery), which is the only place the
    /// query text changes.
    pub(super) pattern: Pattern,
    /// The filtered rows, recomputed when the query changes and when a
    /// directory read lands. Empty whenever the query is.
    ///
    /// Cached rather than recomputed per frame: the palette re-ranks sixty
    /// commands on every composition and that is free, but this walks every
    /// node that has been read, and a large project has a great many.
    pub(super) view: FilterView,
    /// The selected row within [`Self::view`]. Meaningless when not
    /// filtering, where the selection lives in the tree.
    pub(super) filtered: usize,
    /// Whether a query may read directories nobody opened.
    ///
    /// Decided once, by [`crate::project::explorer_root`], and never here:
    /// the crawl is worth running where something bounds it — a project's
    /// `.gitignore` does — and it is worth *not* running where nothing does.
    /// A home directory has no ignore rules and its first twenty thousand
    /// directories are application support files.
    pub(super) crawl: bool,
}

impl FileExplorer {
    /// Opens an explorer rooted at `root`.
    ///
    /// The root opens itself on the first [`poll`](Self::poll) that brings
    /// its listing, not here: a panel showing one collapsed row has told the
    /// user nothing, but expanding before the children exist would mark the
    /// root a leaf permanently.
    ///
    /// `crawl` says whether a query here may read past what is open. It is
    /// the caller's judgment because only the caller knows what the root
    /// *is*; see [`Self::crawl`].
    ///
    /// # Errors
    ///
    /// Returns the operating system's message when the directory reader
    /// cannot be started.
    pub fn open(root: PathBuf, crawl: bool) -> Result<Self, String> {
        let mut files = FileTree::open(root)?;
        let root_id = files.root();
        let mut tree = Tree::new(&mut files);
        tree.select(0);
        Ok(Self {
            files,
            tree,
            scroll: 0,
            wanted: Some(root_id),
            query: Entry::new(),
            pattern: Pattern::Unfiltered,
            view: FilterView::default(),
            filtered: 0,
            crawl,
        })
    }

    /// Points the panel at `root`, throwing away everything that was about
    /// the old one.
    ///
    /// **A whole new arena and a whole new reader thread**, rather than
    /// re-projecting the tree from a node inside the one it has. Two reasons,
    /// and the second is the load-bearing one. A [`NodeId`] is an index into
    /// *this* arena, so every id the panel is holding — the selection, the
    /// deferred expansion, every filter row — belongs to the tree being
    /// replaced. And the ignore rules were compiled relative to the old root,
    /// so a re-projection would keep applying a `.gitignore` from above a
    /// place the user has now left.
    ///
    /// Going up therefore costs exactly what going down does, which is one
    /// thread and one directory read. That uniformity is worth more than the
    /// milliseconds: a panel with two notions of "root" is a panel where half
    /// the operations are correct.
    ///
    /// Whether searching may read past the new root is
    /// [`crate::project::chosen_root`]'s answer, not this function's — a
    /// directory somebody walked into is a directory somebody bounded.
    pub(super) fn reroot(&mut self, root: &Path) -> ExplorerOutcome {
        let chosen = chosen_root(root.to_path_buf());
        let opened = match Self::open(chosen.path, chosen.crawl) {
            Ok(opened) => opened,
            // The reader thread would not start. Nothing has been touched at
            // this point, so the old root is still on screen and still works
            // — which is the right thing to leave behind.
            Err(error) => return ExplorerOutcome::Failed(format!("the file explorer: {error}")),
        };
        *self = opened;
        ExplorerOutcome::Handled
    }

    /// The directory the selection names: itself if it is one, and the folder
    /// holding it otherwise.
    ///
    /// A file is a perfectly sensible thing to have selected when you press
    /// "go in here", and the folder it sits in is the only reading of that
    /// which does anything. Refusing would make the key look broken half the
    /// time it was pressed.
    pub(super) fn selected_directory(&self) -> Option<PathBuf> {
        let id = self.selected_node()?;
        let info = self.files.info(id)?;
        if info.kind.is_expandable() {
            return Some(info.path.to_path_buf());
        }
        info.path.parent().map(Path::to_path_buf)
    }

    /// The directory holding the current root, if there is one above it.
    pub(super) fn parent_of_root(&self) -> Option<PathBuf> {
        let info = self.files.info(self.files.root())?;
        info.path.parent().map(Path::to_path_buf)
    }

    /// Whether the rows come from the filter rather than from the tree.
    ///
    /// Asked of the *pattern*, not of the text: `/` on its own is a keystroke
    /// saying a regular expression is coming, and the tree stays on screen
    /// until one arrives.
    pub(super) const fn is_filtering(&self) -> bool {
        self.pattern.is_narrowing()
    }

    /// Collects any directory reads that finished, and reports whether the
    /// rows changed.
    ///
    /// Call once per frame while the panel is open, and repaint on `true`.
    /// The common answer is `false` and costs one non-blocking channel poll.
    ///
    /// A query in the field also drives the crawl from here — a bounded batch
    /// of directory reads per frame, so a search reaches past what the user
    /// has opened without one keystroke queueing the whole disk. See
    /// [`CRAWL_PER_FRAME`], and [`Self::crawl`] for when it runs at all.
    pub fn poll(&mut self) -> bool {
        // `can_match`, not `is_filtering`: a pattern still being typed —
        // `/[` on the way to `/[a-z]` — narrows the rows to none but cannot
        // match anything, and reading the disk on its behalf is work whose
        // result is discarded by construction.
        let crawling =
            self.crawl && self.pattern.can_match() && self.files.crawl(CRAWL_PER_FRAME) > 0;
        if !self.files.drain() {
            // A posted read is a reason to come back even though nothing has
            // landed yet: `is_waiting` will now say so, and the host repaints
            // while it does.
            return crawling;
        }
        self.tree.refresh(&mut self.files);
        self.open_what_was_wanted();
        if self.is_filtering() {
            // Keep the selection on the node it was on. A read landing
            // elsewhere in the project can change which row scores best, and
            // a selection that jumped because a background read finished
            // would move the target out from under a keypress already in
            // flight.
            let held = self.selected_node();
            self.refilter(held);
        }
        true
    }

    /// Opens the node a keypress asked for, once its listing has arrived.
    ///
    /// A listing that *failed* clears the intent rather than holding it
    /// forever: the node stays closed, its row carries the error, and a
    /// second press is free to ask again.
    fn open_what_was_wanted(&mut self) {
        let Some(id) = self.wanted else {
            return;
        };
        if self.files.is_listed(id) {
            if let Some(index) = self.tree.index_of(&id) {
                self.tree.expand(&mut self.files, index);
            }
            self.wanted = None;
        } else if self.files.info(id).is_some_and(|info| info.error.is_some()) {
            self.wanted = None;
        }
    }

    /// Opens the row at `index`, now or as soon as its listing lands.
    ///
    /// The gate is [`FileTree::is_listed`]: expanding a node whose children
    /// have not arrived would mark it a leaf permanently. Asking the source
    /// for the children is what posts the read.
    pub(super) fn open_row(&mut self, index: usize) {
        let Some(id) = self.tree.row(index).map(|row| row.id) else {
            return;
        };
        if self.files.is_listed(id) {
            self.tree.expand(&mut self.files, index);
            return;
        }
        // Posts the read as a side effect, which is exactly what
        // `TreeSource::children` is documented to do for a listing it does
        // not have.
        drop(self.files.children(Some(&id)));
        self.wanted = Some(id);
    }

    /// Whether more rows are still on their way.
    ///
    /// Two sources, and the host needs one answer covering both: a directory
    /// read in flight, or a query whose crawl has directories left to ask
    /// for. Reporting only the first would stall the crawl — the frame that
    /// drained the last outstanding read still has a queue, and if nothing
    /// asks for another frame there is nobody left to empty it.
    ///
    /// The host asks for another frame while this is `true`: the reads land
    /// on a worker thread and nothing wakes winit when they do, so a window
    /// that stopped repainting would leave a directory permanently loading.
    ///
    /// The crawl half is gated on [`Self::crawl`] and must stay that way. A
    /// panel that will never crawl still has a frontier — the root's own
    /// subdirectories were queued when they were interned — so an ungated
    /// `is_fully_crawled` would answer `false` forever, and the window would
    /// repaint at full rate for as long as a query was in the field.
    #[must_use]
    pub fn is_waiting(&self) -> bool {
        self.files.is_waiting()
            || (self.crawl && self.pattern.can_match() && !self.files.is_fully_crawled())
    }

    /// The node under the selection, from whichever list is showing.
    pub(super) fn selected_node(&self) -> Option<NodeId> {
        if self.is_filtering() {
            return self.view.rows.get(self.filtered).map(|row| row.id);
        }
        self.tree.selected_id().copied()
    }

    /// The path of the row under the selection, if there is one.
    #[must_use]
    pub fn selected_path(&self) -> Option<&Path> {
        let id = self.selected_node()?;
        self.files.info(id).map(|info| info.path)
    }

    /// Re-reads the query text and rebuilds the view from it.
    ///
    /// The only place a regular expression is compiled, and the only place
    /// [`Self::pattern`] is *derived* from the query. `clear_query` also
    /// writes it, but writes the one value that needs no query to know —
    /// [`Pattern::Unfiltered`] — while dropping the query text in the same
    /// breath, so the two cannot disagree about what was asked.
    ///
    /// Called from the key handlers that touch the field, never from
    /// [`poll`](Self::poll): a directory landing changes the rows, not what
    /// the user asked for.
    ///
    /// The selection is not held across it. A different query is a different
    /// question, and keeping the previous answer selected under it would put
    /// the selection somewhere the new query did not put it.
    pub(super) fn requery(&mut self) {
        self.pattern = Pattern::parse(self.query.text());
        self.refilter(None);
    }

    /// Rebuilds the filtered view, landing the selection on `held` if that
    /// node is still a result and on the best match otherwise.
    pub(super) fn refilter(&mut self, held: Option<NodeId>) {
        self.view = filter(&self.files, &self.pattern);
        self.filtered = held
            .and_then(|id| {
                self.view
                    .rows
                    .iter()
                    .position(|row| row.id == id && row.is_selectable())
            })
            .or(self.view.best)
            .unwrap_or(0);
    }
}

/// The modifier shapes this panel distinguishes.
///
/// Shift is not among them: it decides which character a printable key
/// produced, and the key translation has already resolved that.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Chord {
    /// No modifier that changes the meaning of the key.
    Plain,
    /// Control alone — the `Ctrl+N` / `Ctrl+P` movement pair, and the
    /// non-mac spelling of the re-rooting pair.
    Ctrl,
    /// Meta alone — `⌘↑` and `⌘↓`, which are what macOS itself uses for
    /// "enclosing folder" and "open this one".
    Meta,
    /// Control and Alt together — the toggle chord that closes the panel.
    CtrlAlt,
    /// Meta and Alt together — the mac spelling of the same toggle.
    MetaAlt,
    /// Anything else, which the modal panel swallows.
    Other,
}

/// The chord one modifier set names.
pub(super) const fn chord(modifiers: Modifiers) -> Chord {
    match (modifiers.ctrl, modifiers.alt, modifiers.meta) {
        (false, false, false) => Chord::Plain,
        (true, false, false) => Chord::Ctrl,
        (false, false, true) => Chord::Meta,
        (true, true, false) => Chord::CtrlAlt,
        (false, true, true) => Chord::MetaAlt,
        _ => Chord::Other,
    }
}
