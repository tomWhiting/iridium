//! What a filter query means: fuzzy by default, regular expression on a
//! sigil.
//!
//! A file panel's query field carries two different languages, and the user
//! chooses between them with the first character they type. Plain text is a
//! fuzzy subsequence; a leading `/` makes the rest a regular expression.
//! **A sigil rather than a mode key**, so that what the field means is
//! readable in the field itself — a mode toggled by a chord is invisible one
//! second after it was pressed, and a query that silently changed language is
//! the kind of thing that reads as the editor being broken.
//!
//! # Why this is in the kernel
//!
//! For the reason [`crate::fuzzy`] is: a second implementation is a bug
//! waiting to be reported as a feel problem. The decisions here are all
//! invisible until they disagree — whether `/` is the sigil, whether a
//! pattern is tried against the name or the whole path and in which order,
//! whether it is case-sensitive, what a half-typed pattern does. A terminal
//! face and a desktop face that answered any of those differently would give
//! different results for the same keystrokes, and nobody files that.
//!
//! # The rules, in full
//!
//! - **Empty is not a filter.** So is a bare `/`: the sigil with nothing
//!   after it says only that a regular expression is coming. An empty regular
//!   expression matches every string, and showing the whole tree — plus
//!   reading the disk to fill it — the instant someone presses `/` would be
//!   the opposite of what they asked for.
//! - **The name first, then the whole path.** A pattern is tried against the
//!   final component and, failing that, against the path relative to the
//!   root. This is the same order of preference [`crate::fuzzy::PathField`]
//!   expresses as a weight, and it is what makes both `^mod` and `widgets/.*`
//!   do the obvious thing.
//! - **Case-insensitive, always.** The fuzzy half folds case, so the regular
//!   expression half does too: a query that found a file and then lost it
//!   because the user reached for `/` is indefensible. `(?-i)` turns it back
//!   on for anyone who means it, which is standard syntax rather than
//!   something invented here.
//! - **A pattern that does not compile is not a pattern that matches
//!   nothing.** It is a query still being typed — `/[` is two keystrokes into
//!   `/[a-z]`. It narrows the rows to none, so nothing false is on screen,
//!   but it does not go reading the disk, and a face can tell the two apart
//!   and say which one it is looking at.

mod hit;
mod parse;

#[cfg(test)]
mod hit_tests;
#[cfg(test)]
mod parse_tests;

pub use hit::Hit;
pub use parse::{Pattern, REGEX_SIGIL};
