//! The file explorer, whole.
//!
//! The panel, its keys, its oil buffer, its filter, the rename pipeline that
//! turns an edited list of rows into operations and carries them out, and the
//! composition that turns any of it into rows a face can paint.
//!
//! # Why this is here and not in a face
//!
//! ⭐ **A rename means the same thing in both editors.** The desktop face and
//! the terminal face show the same directory in the same oil buffer, and the
//! only thing that differs is how the rows reach a screen. Two copies of this
//! pipeline would let the two faces disagree about what a rename *does* — and
//! the disagreement would be found by the person using them, because nothing
//! compiles both faces' copies at once.
//!
//! # The three stages, in order
//!
//! - [`plan`] — pure. Turns edited rows into validated [`Operation`]s, or into
//!   every [`Refusal`] that stops them. Touches no disk.
//! - [`order`] — pure. Puts those operations in an order that cannot destroy
//!   anything: names vacated before they are moved into, cycles broken through
//!   a temporary, deletes last.
//! - [`apply`] — the only module here that writes anything. Decides nothing;
//!   re-checks everything.
//!
//! And alongside them, two more that are about what a row *is* rather than how
//! one is drawn, and so belong on this side of the line for the same reason the
//! pipeline does:
//!
//! - [`filter`] — narrowing the rows to what a query matches without losing the
//!   hierarchy the matches live in.
//! - [`root`] — where the explorer opens when somebody picks a directory, and
//!   how far a search there may reach. ⚠️ Only the *choice*. Guessing a root
//!   from the active file, the working directory and the home directory stays
//!   in each face, because the two faces need not guess alike.
//! - [`rows`] — composing one row: indent, disclosure, name, error, and the
//!   truncation that keeps a long name inside the width it was given.
//! - [`confirm`] — what is shown before anything is applied: the operations by
//!   name, the refusals, and how much of either did not fit.
//!
//! ⚠️ **`rows` and `confirm` stop at [`PanelRow`](crate::row::PanelRow)s.**
//! They do not build a `PanelContent`, because that carries an anchor, and an
//! anchor is a statement about a window. Each face wraps these rows in its own
//! placement — which is the seam this crate exists to keep honest.
//!
//! # The panel itself
//!
//! - [`panel`] — [`FileExplorer`], [`ExplorerOutcome`], and the state the
//!   modules around it read.
//! - [`keys`] — what a key press does while browsing, and the one key that
//!   starts editing.
//! - [`edit_keys`] — what a key press does once the rows are being edited, a
//!   refusal is showing, or a confirmation is. Reached *before* [`keys`]'
//!   table, because that table gives every printable character to the filter.
//! - [`buffer`] — the rows as loaded and as they now read, and the one place a
//!   row's origin is captured from the node it was drawn from.
//! - [`mode`] — whether the panel is browsing, editing or confirming, and the
//!   rule that unapplied edits are never dropped without being asked about.
//! - [`session`] — everything true *while* the rows are being edited: the
//!   cursor, the name field it carries, and the verbs that change either.
//! - [`compose`] — what reaches the screen, as far as this side of the line
//!   can say it.
//!
//! # ⭐ What is deliberately still in a face, and it is one word: *where*
//!
//! [`compose`] returns a [`PanelBody`](crate::PanelBody) — rows, the width
//! they were composed against, and the caret. It does **not** return anything
//! carrying an anchor, because an anchor is a statement about a window and the
//! two faces' windows cannot be reconciled: the desktop's is a rounded card
//! at a pixel offset, the terminal face's is a box at a cell.
//!
//! So each face wraps a body in its own placement — the desktop in
//! `file_tree::content`, which is one struct literal long. That thinness is
//! the result worth recording: the panel that had to cross faces turned out to
//! need exactly one field's worth of translation, which is what says the seam
//! was drawn in the right place rather than merely somewhere.
//!
//! The polling is likewise the host's to drive. The filesystem source never
//! reads a directory on the thread that asked for it — see
//! [`iridium_explorer`] — so a listing arrives some frames after it was
//! wanted, and [`FileExplorer::poll`] is where that arrival is collected. A
//! face calls it once per frame while the panel is open; `true` means rows
//! changed and a repaint is owed. Skipping it corrupts nothing, it just leaves
//! the panel showing a directory that is permanently loading.

pub mod apply;
pub mod confirm;
pub mod filter;
pub mod order;
pub mod plan;
pub mod root;
pub mod rows;

mod buffer;
mod compose;
mod edit_keys;
mod keymap;
mod keys;
mod mode;
mod panel;
mod resolve;
mod session;
mod verb;

#[cfg(test)]
mod apply_tests;

#[cfg(test)]
mod buffer_tests;

#[cfg(test)]
mod confirm_tests;

#[cfg(test)]
mod edit_keys_tests;

#[cfg(test)]
mod keymap_tests;

#[cfg(test)]
mod mode_tests;

#[cfg(test)]
mod plan_tests;

#[cfg(test)]
mod tests;

pub use apply::{Failure, apply};
pub use filter::{FilterRow, FilterView, filter};
pub use keymap::{DEFAULT_BINDING_COUNT, KEYMAP_NAME, default_keymap};
pub use panel::{ExplorerOutcome, FileExplorer};
pub use plan::{EditedRow, Operation, Plan, Refusal, RowOrigin, plan};
pub use resolve::USER_LAYER_NAME;
pub use root::{ExplorerRoot, chosen_root, is_filesystem_root};
