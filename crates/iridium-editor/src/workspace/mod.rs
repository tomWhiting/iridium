//! Many open documents, and the nested groups that organise them.
//!
//! See `docs/WORKSPACE-DESIGN.md` for why this owns whole [`Editor`]s
//! rather than splitting [`EditorState`](crate::editor::EditorState) into
//! per-document and per-window halves, and what that choice costs.
//!
//! # The two id spaces
//!
//! A [`DocumentId`] names a *buffer*: its text, cursors, undo history,
//! folds, parse tree and search state. A [`NodeId`] names a *place in the
//! organisation*: a tab, or a group of tabs.
//!
//! They are separate because **one document may appear in several groups**.
//! A file can sit in both "current work" and "the auth refactor" without
//! being opened twice, without two undo histories, and without edits in one
//! place failing to show up in the other. Collapsing the two ids would make
//! that impossible, and the impossibility would only surface after the API
//! had callers.
//!
//! # Ids are never reused
//!
//! Both counters only ever increase. A stale id therefore resolves to
//! `None` rather than silently naming whatever was allocated next — the
//! difference between a face that draws nothing and a face that draws the
//! wrong document.

mod dispatch;
mod ids;
mod model;
mod nav;
mod node;
mod tree_source;

#[cfg(test)]
mod nav_tests;
#[cfg(test)]
mod tree_source_tests;
#[cfg(test)]
mod workspace_tests;

pub use dispatch::WorkspaceCommandError;
pub use ids::{DocumentId, NodeId};
pub use model::Workspace;
pub use node::Node;
