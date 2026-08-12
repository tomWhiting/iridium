//! The file explorer's shared half: the rename pipeline that turns an edited
//! list of rows into operations, orders them so nothing is destroyed on the
//! way, and carries them out.
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
//!
//! # What is deliberately still in the desktop face
//!
//! The rows as loaded (`buffer`), what a key does to them (`edit_keys`), what
//! is shown before any of it happens (`confirm`), and the panel itself. Those
//! follow; this slice is the part that could move without a single import
//! rewrite, which is what makes it the honest first proof that the hoist's
//! mechanics work.

pub mod apply;
pub mod filter;
pub mod order;
pub mod plan;
pub mod root;

#[cfg(test)]
mod apply_tests;

#[cfg(test)]
mod plan_tests;

pub use apply::{Failure, apply};
pub use filter::{FilterRow, FilterView, filter};
pub use plan::{EditedRow, Operation, Plan, Refusal, RowOrigin, plan};
pub use root::{ExplorerRoot, chosen_root, is_filesystem_root};
