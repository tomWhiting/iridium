//! The file explorer panel.
//!
//! An [`iridium_tree::Tree`] over an [`iridium_explorer::FileTree`], composed
//! into the same [`PanelContent`] the palette and the undo tree use, so it
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
//! - `keys` — what a key press does.
//! - `compose` — what reaches the screen.
//! - `filter` — narrowing the rows to what a query matches, without losing
//!   the hierarchy the matches live in.
//! - `rows` — composing one row: indent, disclosure, name, error.
//! - `tests` — the panel's own suite, against real directories and the real
//!   reader thread.

mod compose;
mod filter;
mod keys;
mod panel;
mod rows;

#[cfg(test)]
mod tests;

pub use panel::{ExplorerOutcome, FileExplorer};
