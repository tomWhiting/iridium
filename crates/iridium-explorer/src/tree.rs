//! The arena, and the [`TreeSource`] it presents.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use iridium_tree::TreeSource;

use crate::node::{EntryKind, Listing, ListingError, Node, NodeId, NodeInfo};
use crate::worker::Worker;

/// A filesystem hierarchy, read off the frame thread.
///
/// Hand it to an [`iridium_tree::Tree`] as the source and it answers with
/// whatever it already knows, posting a read for anything it does not. See
/// the crate documentation for the frame-by-frame shape that implies.
#[derive(Debug)]
pub struct FileTree {
    /// Every node ever seen, in creation order. **Never shrinks** — see
    /// [`NodeId`] for why an id must not be reusable.
    nodes: Vec<Node>,
    /// The reverse map, so a path that reappears under a refresh keeps the
    /// id it had rather than becoming a new node beside the old one.
    by_path: HashMap<PathBuf, NodeId>,
    /// The root's id, which is also `NodeId(0)`; named so nothing has to
    /// know that.
    root: NodeId,
    /// How many directory reads have been posted and not yet collected.
    ///
    /// Kept as a counter rather than derived by scanning the arena, because
    /// a face reads it every frame to decide whether another frame is owed:
    /// an O(nodes) scan on a fully expanded repository, sixty times a
    /// second, would cost more than the reads it is waiting for.
    pending: usize,
    worker: Worker,
}

impl FileTree {
    /// Opens a hierarchy rooted at `root`.
    ///
    /// The root's own listing is requested immediately, so the first frame
    /// after the first [`drain`](Self::drain) has something to show. `root`
    /// is stored as given: a relative path stays relative, and every child
    /// path is built from it, so a face that wants absolute paths should
    /// canonicalize before calling.
    ///
    /// # Errors
    ///
    /// Returns the operating system's message when the reader thread cannot
    /// be spawned. Nothing is read on the calling thread as a fallback —
    /// see [`crate::FileTree`] on why a stall is worse than a refusal.
    pub fn open(root: PathBuf) -> Result<Self, String> {
        let worker = Worker::start()?;
        let node = Node::new(root.clone(), EntryKind::Directory, None);
        let mut by_path = HashMap::new();
        by_path.insert(root, NodeId(0));
        let mut tree = Self {
            nodes: vec![node],
            by_path,
            root: NodeId(0),
            pending: 0,
            worker,
        };
        tree.request(NodeId(0));
        Ok(tree)
    }

    /// The root node's id.
    #[must_use]
    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// What a face needs to draw `node`, or `None` for an id this tree never
    /// issued.
    ///
    /// An unknown id returns `None` rather than panicking, because a face
    /// holds ids across mutations for the same reason `iridium-tree` says a
    /// row index may be stale.
    #[must_use]
    pub fn info(&self, node: NodeId) -> Option<NodeInfo<'_>> {
        let entry = self.nodes.get(node.0)?;
        Some(NodeInfo {
            path: entry.path.as_path(),
            name: entry.name.as_str(),
            kind: entry.kind,
            error: match &entry.listing {
                Listing::Failed(error) => Some(error.message.as_str()),
                _ => None,
            },
            is_loading: matches!(entry.listing, Listing::Requested),
        })
    }

    /// Whether any directory read is still outstanding.
    ///
    /// A face uses this to decide whether to ask for another frame: the reads
    /// land on a worker thread and nothing wakes an event loop when they do,
    /// so a window that stopped repainting while one was in flight would show
    /// a directory that is permanently loading.
    #[must_use]
    pub const fn is_waiting(&self) -> bool {
        self.pending > 0
    }

    /// Whether `node`'s children are known.
    ///
    /// The question a face must ask before expanding a node: expanding one
    /// whose listing has not landed makes the tree record it as a leaf —
    /// [`TreeSource`]'s fourth rule — and it never opens again. `false` for a
    /// node that has not been asked for, one whose read is outstanding, one
    /// whose read failed, and an id this tree never issued.
    #[must_use]
    pub fn is_listed(&self, node: NodeId) -> bool {
        self.nodes
            .get(node.0)
            .is_some_and(|entry| matches!(entry.listing, Listing::Present(_)))
    }

    /// The id already issued for `path`, if there is one.
    #[must_use]
    pub fn node_at(&self, path: &Path) -> Option<NodeId> {
        self.by_path.get(path).copied()
    }

    /// The node `node` was listed under, or `None` for the root and for an id
    /// this tree never issued.
    ///
    /// The chain this walks is what lets a face reveal a node it found by
    /// searching: every ancestor of a node that appears in a listing has itself
    /// been listed, so a walk up from a search hit only ever meets nodes whose
    /// children are known.
    #[must_use]
    pub fn parent(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.get(node.0).and_then(|entry| entry.parent)
    }

    /// `node`'s children as they are already known, **posting nothing**.
    ///
    /// [`TreeSource::children`] is the frame path and requests what it does not
    /// have, which is right for a node someone opened. It is wrong for anything
    /// that *walks*: a filter sweeping the arena would post a read for every
    /// unlisted directory it touched, turning one keystroke into a crawl of the
    /// whole disk. This is the accessor for walking — empty for a node that has
    /// not been read, one whose read is outstanding, one whose read failed, and
    /// an id this tree never issued.
    ///
    /// Borrowed rather than cloned, because a walk visits every node and the
    /// clone in `children` exists only to satisfy that trait's signature.
    #[must_use]
    pub fn listed_children(&self, node: NodeId) -> &[NodeId] {
        match self.nodes.get(node.0).map(|entry| &entry.listing) {
            Some(Listing::Present(children)) => children,
            _ => &[],
        }
    }

    /// Collects every directory read that has finished, and reports whether
    /// any did.
    ///
    /// **Call this once per frame, before the tree is drawn**, and pass the
    /// answer on: `true` means some node's children changed, which is
    /// exactly when [`iridium_tree::Tree::refresh`] is owed a call. `false`
    /// is the common case and costs one non-blocking channel poll.
    ///
    /// Reads are applied in the order they finished, which is not the order
    /// they were posted — a small directory beats a large one. That is
    /// invisible to the tree, because each response replaces one node's
    /// listing and touches nothing else.
    pub fn drain(&mut self) -> bool {
        let responses = self.worker.collect();
        if responses.is_empty() {
            return false;
        }
        for response in responses {
            self.pending = self.pending.saturating_sub(1);
            // A response for a node this tree never issued cannot happen —
            // ids come from here and are never removed — so this is not a
            // guard so much as a refusal to be able to panic about it.
            if self.nodes.get(response.node).is_none() {
                continue;
            }
            let listing = match response.outcome {
                Err(error) => Listing::Failed(error),
                Ok(entries) => {
                    let mut children = Vec::with_capacity(entries.len());
                    for entry in entries {
                        children.push(self.intern(entry.path, entry.kind, NodeId(response.node)));
                    }
                    Listing::Present(children)
                },
            };
            if let Some(node) = self.nodes.get_mut(response.node) {
                node.listing = listing;
            }
        }
        true
    }

    /// Discards what is known about `node`'s children and asks again.
    ///
    /// The children themselves keep their ids: a file that is still there
    /// after the refresh is the same node it was, which is what lets an
    /// editable listing tell a rename from a delete and a create. Only the
    /// *order and membership* are replaced.
    ///
    /// A node that is not a directory is left alone — there is nothing to
    /// re-read — and an unknown id is a no-op.
    pub fn reload(&mut self, node: NodeId) {
        if self
            .nodes
            .get(node.0)
            .is_none_or(|entry| !entry.kind.is_expandable())
        {
            return;
        }
        if let Some(entry) = self.nodes.get_mut(node.0) {
            entry.listing = Listing::Absent;
        }
        self.request(node);
    }

    /// Interns `path`, returning the id it already had or a fresh one.
    ///
    /// The reuse is what makes [`reload`](Self::reload) non-destructive, and
    /// it also updates the kind: a path that was a file and is now a
    /// directory is the same node with a new shape, not a second node.
    fn intern(&mut self, path: PathBuf, kind: EntryKind, parent: NodeId) -> NodeId {
        if let Some(&existing) = self.by_path.get(&path) {
            if let Some(node) = self.nodes.get_mut(existing.0) {
                node.kind = kind;
                node.parent = Some(parent);
            }
            return existing;
        }
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node::new(path.clone(), kind, Some(parent)));
        self.by_path.insert(path, id);
        id
    }

    /// Posts a read for `node`, marking it pending — or failed, if the
    /// reader has gone.
    fn request(&mut self, node: NodeId) {
        let Some(path) = self.nodes.get(node.0).map(|entry| entry.path.clone()) else {
            return;
        };
        let posted = self.worker.request(node.0, path);
        if posted {
            self.pending = self.pending.saturating_add(1);
        }
        if let Some(entry) = self.nodes.get_mut(node.0) {
            entry.listing = if posted {
                Listing::Requested
            } else {
                Listing::Failed(ListingError {
                    message: "the directory reader has stopped".to_owned(),
                })
            };
        }
    }
}

impl TreeSource for FileTree {
    type Id = NodeId;

    /// The children of `parent`, or the root when it is `None`.
    ///
    /// **Never blocks.** A listing that has not been read yet is requested
    /// and an empty slice returned, which the tree reads as "no children
    /// yet" and draws as an open, empty node. The read lands on a later
    /// frame, [`drain`](FileTree::drain) reports it, and the face calls
    /// [`iridium_tree::Tree::refresh`] — at which point this returns the
    /// real children and the rows splice in.
    ///
    /// That is a visible flicker of an empty directory on a cold expand, and
    /// it is the honest trade: the alternative is the frame waiting on a
    /// disk. A face that would rather show a spinner has
    /// [`NodeInfo::is_loading`](crate::NodeInfo::is_loading) to tell the two
    /// empties apart.
    fn children(&mut self, parent: Option<&Self::Id>) -> Vec<Self::Id> {
        let Some(&node) = parent else {
            return vec![self.root];
        };
        match self.nodes.get(node.0).map(|entry| &entry.listing) {
            Some(Listing::Present(children)) => children.clone(),
            Some(Listing::Absent) => {
                self.request(node);
                Vec::new()
            },
            // Requested, failed, or an id this tree never issued: nothing to
            // show and nothing new to ask for. A failed listing is not
            // retried on every frame the node is open — `reload` is the way
            // back, and it is a deliberate act.
            _ => Vec::new(),
        }
    }

    /// Whether `node` should be drawn as openable.
    ///
    /// Answered from the entry's own kind, which came back with the listing
    /// that produced it — no syscall, which is what this method's cost
    /// contract requires. Optimistic in exactly the way rule 4 permits: a
    /// directory that turns out to be empty, or that cannot be read, claimed
    /// children here and produces none, and the tree treats it as a leaf
    /// from then on.
    fn has_children(&mut self, node: &Self::Id) -> bool {
        self.nodes
            .get(node.0)
            .is_some_and(|entry| entry.kind.is_expandable())
    }
}
