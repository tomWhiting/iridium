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
//! - `keys` — what a key press does.
//! - `compose` — what reaches the screen.
//! - `filter` — narrowing the rows to what a query matches, without losing
//!   the hierarchy the matches live in.
//! - `apply` — carrying a plan out against the filesystem. The only module
//!   here that writes anything.
//! - `plan` — turning an edited list of rows into a validated, ordered list
//!   of filesystem operations. Pure; touches no disk.
//! - `buffer` — the rows as loaded and as they now read, and the one place a
//!   row's origin is captured from the node it was drawn from.
//! - `confirm` — what is shown before any of it happens.
//! - `mode` — whether the panel is browsing, editing or confirming, and the
//!   rule that unapplied edits are never dropped without being asked about.
//! - `rows` — composing one row: indent, disclosure, name, error.
//! - `tests` — the panel's own suite, against real directories and the real
//!   reader thread.

mod apply;
mod buffer;
mod compose;
mod confirm;
mod filter;
mod keys;
// ⚠️ NOT YET REACHABLE, and this allow is the record of why.
//
// `mode` is step 4b of `docs/IN-FLIGHT-oil.md` — complete, and covered by 19
// tests in `mode_tests`. What it is missing is a caller, and the caller is
// `keys`, which cannot be written until three bindings are ruled on: what
// enters edit mode, how a row is marked deleted and how one is created, and
// what applies. Those are named in the doc and have been asked twice.
//
// The alternative was to guess the three keys and wire it anyway. That is the
// worse trade here: the logic above is the part where a mistake loses
// somebody's files, and it is finished and proven either way, while a guessed
// binding is a table row that would be rewritten the moment the ruling lands.
//
// **This allow is temporary and self-removing**: the moment `keys` calls into
// the module, the lint stops firing and this comment stops being true, so it
// goes with the same change.
#[allow(
    dead_code,
    reason = "step 4b's logic is finished and tested; its caller is blocked on \
              three key rulings, see docs/IN-FLIGHT-oil.md"
)]
mod mode;
mod panel;
mod plan;
mod rows;

#[cfg(test)]
mod apply_tests;

#[cfg(test)]
mod buffer_tests;

#[cfg(test)]
mod confirm_tests;

#[cfg(test)]
mod mode_tests;

#[cfg(test)]
mod plan_tests;

#[cfg(test)]
mod tests;

pub use panel::{ExplorerOutcome, FileExplorer};
