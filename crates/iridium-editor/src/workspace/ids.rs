//! The two opaque identifiers, and why there are two of them.
//!
//! See the [module documentation](super) for the full argument. In short: a
//! [`DocumentId`] names a buffer and a [`NodeId`] names a place, so one file
//! can occupy two places without being loaded twice.
//!
//! Both are newtypes over a monotonic counter and are **never reused**, so a
//! stale id resolves to `None` rather than silently naming whatever was
//! allocated next.
//!
//! The inner counter is `pub(super)` — reachable from the workspace's own
//! files, which have to allocate and to name a deliberately-stale id in a
//! test, and from nowhere else. Outside this module both types are opaque,
//! which is what stops a caller inventing an id or doing arithmetic on one.

/// Identifies an open buffer.
///
/// Opaque and never reused; see the [module documentation](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub(super) u64);

/// Identifies a place in the organisation — a tab or a group.
///
/// Opaque and never reused; see the [module documentation](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub(super) u64);
