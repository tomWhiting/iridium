//! The thread that reads directories, so the frame does not.
//!
//! [`TreeSource::children`](iridium_tree::TreeSource::children) is called
//! from whatever thread the face's frame runs on, and `iridium-tree`'s own
//! documentation names the rule this module exists to keep: **a source must
//! not block on I/O**. A `readdir` on a cold directory over a network mount
//! is tens of milliseconds; the frame budget is eight.
//!
//! So a request to list a directory is posted here and answered later. The
//! tree is told the listing is not ready yet, the face draws a frame without
//! it, and the answer is collected by
//! [`FileTree::drain`](crate::FileTree::drain) on some later frame.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvError, Sender, channel};
use std::thread::{Builder, JoinHandle};

use crate::ignores::Ignores;
use crate::node::{EntryKind, ListingError};

/// One directory to read, identified by the arena index that asked for it.
///
/// The index travels rather than a borrow, because the arena is on the other
/// thread and will have moved on by the time this comes back.
pub struct Request {
    pub node: usize,
    pub path: PathBuf,
}

/// One entry a directory read produced.
pub struct Entry {
    pub path: PathBuf,
    pub kind: EntryKind,
    /// Whether the crawl should leave this entry alone.
    ///
    /// Decided here because deciding it needs to read `.gitignore` files, and
    /// this is the thread where reading happens. It is advice for the crawl
    /// and nothing else: the entry is still listed, still drawn, and still
    /// openable by hand.
    pub ignored: bool,
}

/// A directory read, finished.
pub struct Response {
    pub node: usize,
    pub outcome: Result<Vec<Entry>, ListingError>,
}

/// The reader thread and the two ends of the conversation with it.
pub struct Worker {
    /// `None` only while dropping, which is what ends the reader's `recv`.
    requests: Option<Sender<Request>>,
    responses: Receiver<Response>,
    /// Kept so the thread is joined when the tree is dropped rather than
    /// detached. A detached reader outlives the arena it was reading for,
    /// and on a directory big enough to matter it keeps a disk busy for a
    /// window that has already closed.
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts the reader thread.
    ///
    /// `root` bounds the walk the ignore rules do: a `.gitignore` above it
    /// belongs to a project this tree is not part of.
    ///
    /// # Errors
    ///
    /// Returns the operating system's message when the thread cannot be
    /// spawned. There is no silent fallback to reading on the calling
    /// thread: that would trade a failure someone can see for a stall
    /// nobody can explain.
    pub fn start(root: PathBuf) -> Result<Self, String> {
        let (request_tx, request_rx) = channel::<Request>();
        let (response_tx, response_rx) = channel::<Response>();

        let handle = Builder::new()
            .name("iridium-explorer".to_owned())
            .spawn(move || run(&request_rx, &response_tx, &mut Ignores::new(root)))
            .map_err(|error| error.to_string())?;

        Ok(Self {
            requests: Some(request_tx),
            responses: response_rx,
            handle: Some(handle),
        })
    }

    /// Posts a directory to read.
    ///
    /// Returns `false` when the reader thread has gone — which cannot happen
    /// while this `Worker` is alive and holding the only sender, but is
    /// reported rather than ignored so the caller can mark the listing
    /// failed instead of leaving it pending forever.
    pub fn request(&self, node: usize, path: PathBuf) -> bool {
        self.requests
            .as_ref()
            .is_some_and(|requests| requests.send(Request { node, path }).is_ok())
    }

    /// Every finished read available right now, without waiting for any.
    pub fn collect(&self) -> Vec<Response> {
        self.responses.try_iter().collect()
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Dropping the sender ends the loop's `recv`; the join then waits
        // out at most the one read already in flight.
        drop(self.requests.take());
        if let Some(handle) = self.handle.take() {
            drop(handle.join());
        }
    }
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Worker")
            .field("running", &self.handle.is_some())
            // The two channel ends have no useful `Debug`, and a tree's
            // `Debug` is read for its arena. Whether the reader is still up
            // is the one thing about this half worth printing.
            .finish_non_exhaustive()
    }
}

/// The reader loop: one directory per request, until the sender is dropped.
fn run(requests: &Receiver<Request>, responses: &Sender<Response>, ignores: &mut Ignores) {
    loop {
        match requests.recv() {
            Ok(request) => {
                let outcome = read_directory(&request.path, ignores);
                // A closed response channel means the tree is gone; there is
                // nobody left to read for.
                if responses
                    .send(Response {
                        node: request.node,
                        outcome,
                    })
                    .is_err()
                {
                    return;
                }
            },
            Err(RecvError) => return,
        }
    }
}

/// One directory, sorted the way it will be shown.
///
/// **Directories first, then by name, case-insensitively.** That ordering is
/// policy, and `iridium-tree` says policy belongs to whoever knows what the
/// nodes are — which is here. It is done on this thread rather than the
/// face's for the same reason the read is: a directory with ten thousand
/// entries sorts in milliseconds, and milliseconds are the budget.
///
/// An entry whose type cannot be determined is kept, as
/// [`EntryKind::Other`](crate::EntryKind::Other). Dropping it would make a
/// file invisible for a reason that has nothing to do with the file.
fn read_directory(
    path: &std::path::Path,
    ignores: &mut Ignores,
) -> Result<Vec<Entry>, ListingError> {
    let reader = std::fs::read_dir(path).map_err(|error| ListingError {
        message: error.to_string(),
    })?;

    let mut entries: Vec<Entry> = Vec::new();
    for entry in reader {
        // One unreadable entry does not spoil the listing: the directory was
        // opened, so the rest of it is real and worth showing.
        let Ok(entry) = entry else { continue };
        // `file_type` reports on the entry itself and does not follow links,
        // which is what keeps a link to an ancestor from becoming an
        // unbounded subtree.
        let kind = entry.file_type().map_or(EntryKind::Other, |file_type| {
            if file_type.is_dir() {
                EntryKind::Directory
            } else if file_type.is_symlink() {
                EntryKind::Symlink
            } else if file_type.is_file() {
                EntryKind::File
            } else {
                EntryKind::Other
            }
        });
        let path = entry.path();
        let skip = ignores.is_ignored(&path, kind.is_expandable());
        entries.push(Entry {
            path,
            kind,
            ignored: skip,
        });
    }

    // `sort_by_cached_key`, not `sort_by`: the key allocates two strings, and
    // a comparison-time key would build them O(n log n) times instead of
    // once each. On a directory of ten thousand entries that is the
    // difference between a sort and a stall.
    entries.sort_by_cached_key(|entry| {
        // Directories first — `false` sorts before `true`, so the flag is
        // inverted here rather than in a second comparison.
        (!entry.kind.is_expandable(), sort_key(&entry.path))
    });
    Ok(entries)
}

/// The key entries are ordered by: the name lowercased, then the name as it
/// is.
///
/// Lowercased first so `Cargo.toml` and `apps/` interleave the way a person
/// reads them rather than the way ASCII does, and the exact name second so
/// the order is *total* — `README` and `readme` can coexist on a
/// case-sensitive filesystem, and an ordering that called them equal would
/// let them swap between reads and violate the stability the tree splices on.
fn sort_key(path: &std::path::Path) -> (String, String) {
    let name = path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    (name.to_lowercase(), name)
}
