//! The arena a [`FileTree`](crate::FileTree) grows, and the ids into it.

use std::path::{Path, PathBuf};

/// A node's identity, stable for the lifetime of the tree that issued it.
///
/// An index into the arena rather than a path, for the two reasons
/// [`TreeSource::Id`](iridium_tree::TreeSource::Id) asks for: it is `Copy`
/// and one word wide, so the tree can hold one per visible row and one per
/// expanded node without a heap allocation apiece; and it is cheap to hash,
/// which the expansion set does on every row it materialises.
///
/// **Stability is the load-bearing property**, and it is why nothing is ever
/// removed from the arena. A node whose file has been deleted keeps its id
/// and is marked gone. An editable listing diffs *by id* — a rename that
/// resolved to delete-plus-create would destroy a large file's contents and
/// write them back rather than moving it — so an id that could be reused for
/// a different file is the one thing this type must never permit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub(crate) usize);

impl NodeId {
    /// The index this id addresses.
    ///
    /// Exposed so a face can key its own side tables by node without hashing
    /// a path. The value is meaningful only to the tree that issued it.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
    }
}

/// What a directory entry turned out to be.
///
/// Taken from [`std::fs::DirEntry::file_type`], which reports on the entry
/// itself and **does not follow symbolic links**. A symlink is therefore its
/// own kind and never a [`Directory`](EntryKind::Directory), which is how
/// this source satisfies the acyclicity rule the tree cannot check: a link
/// pointing at one of its own ancestors is a leaf, not an unbounded subtree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntryKind {
    /// A directory, which may be expanded.
    Directory,
    /// A regular file.
    File,
    /// A symbolic link, of whatever it points at. Never expanded.
    Symlink,
    /// A socket, a fifo, a device — anything else the platform reports.
    Other,
}

impl EntryKind {
    /// Whether a node of this kind can hold children.
    #[must_use]
    pub const fn is_expandable(self) -> bool {
        matches!(self, Self::Directory)
    }
}

/// Why a directory could not be listed.
///
/// The message is the operating system's, kept verbatim so the row can show
/// what actually went wrong rather than a category. A directory that fails
/// to list is a leaf that says why, never a silent empty one — an empty
/// directory and an unreadable one look identical otherwise, and only one of
/// them is worth telling someone about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListingError {
    /// The operating system's description of the failure.
    pub message: String,
}

/// The state of one node's directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Listing {
    /// Never asked for. A node in this state is enumerated on first request.
    Absent,
    /// Requested, and the worker has not answered yet.
    Requested,
    /// Read, and these are the children in the order they will be shown.
    Present(Vec<NodeId>),
    /// Read and failed.
    Failed(ListingError),
}

/// One entry in the arena.
#[derive(Debug, Clone)]
pub struct Node {
    /// The absolute path this node was created for.
    pub path: PathBuf,
    /// The name a face shows: the path's final component, or the whole path
    /// when it has none.
    ///
    /// Stored rather than derived, because a face reads it once per visible
    /// row per frame and deriving it allocates. Computed once, at the only
    /// moment the path is set.
    pub name: String,
    /// What the entry is, as of the listing that produced it.
    pub kind: EntryKind,
    /// The node this one was listed under, or `None` for the root.
    pub parent: Option<NodeId>,
    /// This node's own children, once they are known.
    pub listing: Listing,
    /// Whether the crawl should leave this node alone.
    ///
    /// Advice for the crawl and nothing else — see [`crate::ignores`]. An
    /// ignored directory is still listed, still drawn and still openable by
    /// hand; it is only never walked into on the tree's own initiative.
    ///
    /// The root is never ignored: it is the thing the user asked to look at.
    pub ignored: bool,
}

impl Node {
    /// A node for `path`, with its display name resolved.
    ///
    /// A root given as `/` or as `C:\` has no final component, and neither
    /// does one ending in `..`. Showing the whole path is the only honest
    /// answer for those, and it is the case a naive `file_name()` unwrap
    /// would panic on.
    pub fn new(path: PathBuf, kind: EntryKind, parent: Option<NodeId>, ignored: bool) -> Self {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        Self {
            path,
            name,
            kind,
            parent,
            listing: Listing::Absent,
            ignored,
        }
    }
}

/// A node as a face sees it.
///
/// Borrowed from the tree, so it costs nothing to produce and cannot go
/// stale while it is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeInfo<'a> {
    /// The absolute path.
    pub path: &'a Path,
    /// The final path component, or the whole path when there is none.
    pub name: &'a str,
    /// What the entry is.
    pub kind: EntryKind,
    /// Why this node's children could not be listed, if they could not be.
    pub error: Option<&'a str>,
    /// Whether the children have been asked for and not yet arrived.
    pub is_loading: bool,
}
