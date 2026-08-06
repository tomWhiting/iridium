//! A filesystem hierarchy for Iridium's tree views, read off the frame
//! thread.
//!
//! [`iridium_tree`] owns what a tree *does* — which nodes are open, what is
//! therefore visible, where the selection is — and knows nothing about where
//! the hierarchy came from. This crate is the filesystem's answer to its
//! [`TreeSource`](iridium_tree::TreeSource): an arena of directory entries
//! that hands back what it already has and reads the rest in the background.
//!
//! # Why this is its own crate
//!
//! For the reason its two neighbours are what they are. `iridium-tree` has
//! no dependencies on purpose, so that a tree cannot learn whether it is
//! showing files or syntax nodes; `iridium-file` has none on purpose, so
//! that the file layer cannot learn what a face does with the bytes.
//! Folding this code into either would teach one of them the thing it is
//! documented not to know. And a browser has no directory to read, so this
//! is native-only in a way neither neighbour is.
//!
//! # The frame-by-frame shape
//!
//! The rule this crate exists to keep is stated in `iridium-tree`'s own
//! documentation: **a source must not block on I/O.** The tree calls
//! [`children`](iridium_tree::TreeSource::children) on whatever thread the
//! face's frame runs on, and a `readdir` on a cold or networked directory is
//! tens of milliseconds against a budget of eight.
//!
//! So the loop for a face is three lines, once per frame:
//!
//! ```no_run
//! # use std::path::PathBuf;
//! # use iridium_explorer::FileTree;
//! # use iridium_tree::Tree;
//! # fn frame(files: &mut FileTree, tree: &mut Tree<FileTree>) {
//! if files.drain() {
//!     tree.refresh(files);
//! }
//! # }
//! ```
//!
//! [`drain`](FileTree::drain) collects the reads that finished since the last
//! frame and says whether any did; a `true` is exactly when
//! [`Tree::refresh`](iridium_tree::Tree::refresh) is owed a call. The common
//! answer is `false`, and it costs one non-blocking channel poll.
//!
//! **A cold expand therefore shows an empty directory for one frame.** That
//! is the visible price of not stalling, and it is the right one; a face that
//! would rather draw a spinner can tell a loading node from an empty one with
//! [`NodeInfo::is_loading`].
//!
//! # Identity
//!
//! [`NodeId`] is an index into an arena that **never shrinks**, so an id is
//! never reused for a different file. That is not a detail: an editable
//! listing — the oil.nvim-shaped buffer this tree is being built for — works
//! by diffing rows *by id*. Match rows by name instead and a rename reads as
//! a delete plus a create, which for a large file means destroying its
//! contents and writing them back rather than moving it.
//!
//! # Symbolic links
//!
//! A symlink is [`EntryKind::Symlink`], never [`EntryKind::Directory`], so it
//! is a leaf and is never followed. `iridium-tree` requires an acyclic
//! hierarchy and says it has no way to see a cycle coming; a link pointing at
//! one of its own ancestors is exactly that cycle, and not following one is
//! how this source keeps the rule.
//!
//! # Panics
//!
//! None. Every id is bounds-checked against the arena and an unknown one is
//! a no-op returning `None`, `false` or an empty listing — a face holds ids
//! across mutations, and a tree source is not a place to discover that a
//! click raced a refresh.

mod node;
mod tree;
mod worker;

#[cfg(test)]
mod tests;

pub use node::{EntryKind, ListingError, NodeId, NodeInfo};
pub use tree::FileTree;
