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
//! - `filter` — narrowing the rows to what a query matches, without losing
//!   the hierarchy the matches live in.
//! - `apply` — carrying a plan out against the filesystem. The only module
//!   here that writes anything.
//! - `plan` — turning an edited list of rows into validated operations, or
//!   into every reason it will not. Pure; touches no disk.
//! - `order` — putting those operations in an order that cannot destroy
//!   anything on the way: names vacated before they are moved into, cycles
//!   through a temporary, deletes last.
//! - `buffer` — the rows as loaded and as they now read, and the one place a
//!   row's origin is captured from the node it was drawn from.
//! - `confirm` — what is shown before any of it happens.
//! - `mode` — whether the panel is browsing, editing or confirming, and the
//!   rule that unapplied edits are never dropped without being asked about.
//! - `session` — everything that is true *while* the rows are being edited:
//!   the cursor, the name field it carries, and the verbs that change either.
//! - `rows` — composing one row: indent, disclosure, name, error.
//! - `tests` — the panel's own suite, against real directories and the real
//!   reader thread.

mod apply;
mod buffer;
mod compose;
mod confirm;
mod edit_keys;
mod filter;
mod keys;
mod mode;
mod order;
mod panel;
mod plan;
mod rows;
mod session;

#[cfg(test)]
mod apply_tests;

#[cfg(test)]
mod buffer_tests;

#[cfg(test)]
mod confirm_tests;

#[cfg(test)]
mod edit_keys_tests;

#[cfg(test)]
mod mode_tests;

#[cfg(test)]
mod plan_tests;

#[cfg(test)]
mod tests;

pub use panel::{ExplorerOutcome, FileExplorer};
