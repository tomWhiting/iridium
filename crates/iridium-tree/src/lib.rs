//! Expansion state and a virtualised row projection for hierarchical views.
//!
//! This crate is the shared half of every tree-shaped sidebar Iridium will
//! grow: a filesystem tree, a document outline from the syntax tree, and
//! anything else with parents and children. It owns **which nodes are open,
//! what is therefore visible, and where the selection is** — and nothing
//! else. It does not know what a node is, cannot draw one, and has no
//! opinion about where the hierarchy came from.
//!
//! # Why this is a crate and not a widget
//!
//! "Sidebar" names three different features in this project, and only one of
//! them can work in a browser: a filesystem tree is permanently native-only
//! because the web has no filesystem, while an outline works wherever the
//! syntax tree does. If the tree's *behaviour* lived in a face, those three
//! would each grow their own answer to questions like *what happens to the
//! selection when I collapse its parent* — and the answers would differ
//! between the browser and the desktop, invisibly, exactly as two copies of
//! a pixel-to-index conversion did before 2026-08-06.
//!
//! So the split is behaviour versus pixels, not native versus web. Every
//! rule below is in Rust, compiled into whichever face needs it. A face
//! supplies a [`TreeSource`] and renders [`Row`]s; that is its whole job.
//!
//! **A crate, not a separate module at runtime.** Being its own crate is a
//! compile-time boundary and costs nothing: calls are direct, memory is
//! shared, and the optimiser inlines straight through. Shipping a tree as a
//! *separate wasm module* would be the other thing entirely — two modules do
//! not share linear memory, so every interaction would serialise through
//! JavaScript. That is the one packaging choice that would spend the
//! responsiveness this editor exists to have.
//!
//! # The shape of the model
//!
//! Expansion is authoritative; the rows are derived from it.
//!
//! - **[`Tree::expanded`] is a set of node ids.** Collapsing a node removes
//!   only that node from the set, so its descendants keep their state and
//!   re-expanding restores the subtree exactly as it was. A rows-only model
//!   would forget, and forgetting is the wrong default — a collapse is a
//!   glance away, not a reset.
//! - **[`Tree::rows`] is a flat `Vec` carrying a depth per row.** Flat
//!   because rendering wants a window (`rows[first..first + count]`) and a
//!   nested structure cannot answer that without a walk. Depth because
//!   indentation is the only thing a renderer needs that a flat list loses.
//! - **Expanding and collapsing splice**, they do not rebuild. Expanding
//!   costs the visible descendants it inserts; collapsing costs the ones it
//!   removes. Neither is a function of the size of the tree, which is what
//!   keeps a large repository responsive.
//!
//! # Laziness, and why the source takes `&mut self`
//!
//! [`TreeSource::children`] receives `&mut self` so a source can read a
//! directory on demand and cache it. The tree calls it **only** when a node
//! is expanded, so an unexpanded subtree is never enumerated.
//!
//! That is also the load-bearing performance rule for a filesystem source,
//! and this crate cannot enforce it: **a source must not block on I/O.** The
//! tree runs on whatever thread the face's frame does, so a `readdir` inside
//! [`TreeSource::children`] is a stall in the middle of a frame. A
//! filesystem source should hand back what it already has and arrange for the
//! rest to arrive later — see [`Tree::refresh`], which exists for exactly
//! that arrival.
//!
//! # Panics
//!
//! None. Every index this crate accepts is a row index that may be stale,
//! because a face holds indices across mutations; each is bounds-checked and
//! an out-of-range index is a no-op returning `false` or `None`. A tree
//! model is not a place to discover that a click raced a refresh.

#![forbid(unsafe_code)]

mod nav;
mod row;
mod source;
#[cfg(test)]
mod test_source;
mod tree;

pub use row::Row;
pub use source::TreeSource;
pub use tree::Tree;
