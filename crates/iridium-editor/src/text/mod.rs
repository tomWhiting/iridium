//! Pure text transformations, independent of documents and cursors.
//!
//! Everything here is a function from `&str` to `String` (or to a `Cow` when
//! the input is already in the target form). Nothing in this module knows what
//! a document, a cursor or a command is — which is what lets the interesting
//! decisions be tested directly, rather than through an editing verb that has
//! its own multi-cursor and undo behaviour to get wrong at the same time.
//!
//! The editing verbs that use these live in
//! [`crate::input::keyboard`], and are what the `transform.*` commands run.

pub mod case;
pub mod lines;
