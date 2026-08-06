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
//!
//! # Crossing a language boundary
//!
//! A host that draws tabs must be able to hand an id back — "activate this
//! one" — so [`as_raw`](NodeId::as_raw) and [`from_raw`](NodeId::from_raw)
//! exist for the bindings crates to serialize with. `from_raw` is not a
//! promise that the id names anything: every lookup on a
//! [`Workspace`](super::Workspace) resolves an unknown id to `None`, which
//! is precisely why reconstruction is safe. It is *not* an invitation to do
//! arithmetic; the counters are an implementation detail and the only
//! supported operations remain equality and lookup.
//!
//! What the round trip cannot defend against is a host confusing the two
//! spaces — both count from the same place, so document 3 and node 3 both
//! exist and a swap would resolve to a real but wrong object. That is a
//! wire-format problem, and the bindings crate solves it there by tagging
//! each id with its space rather than emitting a bare number.

/// Identifies an open buffer.
///
/// Opaque and never reused; see the [module documentation](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DocumentId(pub(super) u64);

impl DocumentId {
    /// The underlying counter, for a host that must serialize this id.
    #[must_use]
    pub const fn as_raw(self) -> u64 {
        self.0
    }

    /// Rebuilds an id from a value a host previously received.
    ///
    /// Does not check that the id names a live document — nothing here
    /// could, and [`Workspace::editor`](super::Workspace::editor) already
    /// answers `None` for anything it does not hold.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}

/// Identifies a place in the organisation — a tab or a group.
///
/// Opaque and never reused; see the [module documentation](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub(super) u64);

impl NodeId {
    /// The underlying counter, for a host that must serialize this id.
    #[must_use]
    pub const fn as_raw(self) -> u64 {
        self.0
    }

    /// Rebuilds an id from a value a host previously received.
    ///
    /// Does not check that the id names a live node — nothing here could,
    /// and [`Workspace::node`](super::Workspace::node) already answers
    /// `None` for anything it does not hold.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }
}
