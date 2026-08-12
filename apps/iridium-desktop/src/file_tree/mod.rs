//! The file explorer panel.
//!
//! An [`iridium_tree::Tree`] over an [`iridium_explorer::FileTree`], composed
//! into the same [`PanelContent`](crate::overlay::PanelContent) the palette and
//! the undo tree use, so it
//! inherits their chrome, their placement and their focus discipline rather
//! than growing a third answer to any of it.
//!
//! # The one thing this panel does that the others do not
//!
//! **It polls.** The filesystem source never reads a directory on the thread
//! that asked for it — see [`iridium_explorer`] — so a listing arrives some
//! frames after it was wanted. [`FileExplorer::poll`] is where that arrival is
//! collected, and the host calls it once per frame while the panel is open. A
//! `true` means rows changed and a repaint is owed.
//!
//! Skipping the poll does not corrupt anything; it just leaves the panel
//! showing a directory that is permanently loading, which is the failure this
//! module's documentation exists to make findable.
//!
//! # Where the pieces live
//!
//! - `panel` — [`FileExplorer`], [`ExplorerOutcome`], and the state both of
//!   the modules below read.
//! - `keys` — what a key press does while browsing, and the one key that
//!   starts editing.
//! - `edit_keys` — what a key press does once the rows are being edited, a
//!   refusal is showing, or a confirmation is. Reached *before* `keys`'
//!   table, because that table gives every printable character to the filter.
//! - `compose` — what reaches the screen.
//! - `buffer` — the rows as loaded and as they now read, and the one place a
//!   row's origin is captured from the node it was drawn from.
//! - `mode` — whether the panel is browsing, editing or confirming, and the
//!   rule that unapplied edits are never dropped without being asked about.
//! - `session` — everything that is true *while* the rows are being edited:
//!   the cursor, the name field it carries, and the verbs that change either.
//! - `tests` — the panel's own suite, against real directories and the real
//!   reader thread.
//!
//! # What is no longer here
//!
//! `plan`, `order` and `apply` — the rename pipeline — plus `filter`, `rows`
//! and `confirm` now live in
//! [`iridium_panel::explorer`], because a rename means the same thing in both
//! faces and two copies of it could disagree about what one *does*. They are
//! re-exported below, so the modules that stayed read exactly as they did.

mod buffer;
mod compose;
mod edit_keys;
mod keys;
mod mode;
mod panel;
mod session;

#[cfg(test)]
mod buffer_tests;

#[cfg(test)]
mod edit_keys_tests;

#[cfg(test)]
mod mode_tests;

#[cfg(test)]
mod tests;

// ⭐ **Hoisted to `iridium-panel`, and re-exported here rather than
// rewritten at every call site.** A rename means the same thing in both
// faces, so the pipeline that decides what one *is* belongs in the crate both
// faces depend on. The nine modules that stayed still read `super::plan::…`
// and `super::apply::…`, which is the point: the hoist moved code, not
// meaning. See [`iridium_panel::explorer`].
pub(crate) use iridium_panel::explorer::{apply, confirm, filter, plan, rows};

pub use panel::{ExplorerOutcome, FileExplorer};
