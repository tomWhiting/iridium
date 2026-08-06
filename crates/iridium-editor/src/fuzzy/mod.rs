//! Fuzzy subsequence matching and scoring for one query against one string.
//!
//! This is the ranking primitive, and it deliberately knows nothing about what
//! it is ranking. It began life inside the command palette, scoring command
//! titles; it lives here because **a second ranking implementation is a bug
//! waiting to be reported as a feel problem**. A file finder that scored paths
//! with its own matcher would give a different top hit for the same query than
//! the palette gives, and nobody would file that — they would just find the
//! editor faintly untrustworthy.
//!
//! So callers supply strings and a weight, and the ordering they get is the
//! ordering every other caller gets.
//!
//! # What it guarantees
//!
//! - **Integer arithmetic**, so the same query ranks identically in the
//!   terminal, in a native embedder and in the browser. Floats would be
//!   deterministic here in practice; integers make it true by construction.
//! - **No recursion**, because the obvious try-every-assignment matcher is
//!   exponential. The two-pass scheme is linear in the string's length and
//!   loses only pathological cases.
//! - **No allocation**, because this runs on every keystroke against every
//!   candidate, and matched positions can never outnumber the query, which is
//!   capped at [`MAX_QUERY_CHARS`].
//! - **Character indices, never byte offsets**, so a caller underlining a
//!   match highlights the glyph the user typed rather than the middle of a
//!   multi-byte one.
//!
//! # The two passes
//!
//! A query matches when its characters appear in order, not necessarily
//! together. Many assignments of query characters to positions may exist —
//! `ln` against `Join Lines` can take the `l` of `Lines` or neither — and
//! their quality differs enormously, since consecutive and word-initial
//! characters are what a reader perceives as a good match.
//!
//! The **forward** pass takes the leftmost assignment, and doubles as the
//! canonical subsequence test: if it fails, no assignment exists at all. The
//! **backward** pass takes the rightmost. Scoring both and keeping the better
//! costs one extra linear scan, and fixes the common failure of
//! leftmost-only matchers — `de` against `Duplicate Line Down` scoring the
//! `d` of `Duplicate` and the `e` of `Line` rather than the word-initial `D`
//! and the `e` that follows it.

mod query;
mod score;

#[cfg(test)]
mod score_tests;

pub use query::{MAX_QUERY_CHARS, Query};
pub use score::{FieldMatch, match_field};
