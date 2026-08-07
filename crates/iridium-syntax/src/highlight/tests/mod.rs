//! Tests for capture-name mapping and span production.
//!
//! Split on that seam: [`capture`] asks what a capture name means, [`span`]
//! asks what the walk emits. The one helper below is shared rather than
//! duplicated into both, because every test in either half must go through
//! the same two-step the real callers do — if that shape ever changes, it
//! should change in one place.

mod capture;
mod span;

use crate::highlight::{HighlightSpan, Highlighter};
use crate::{Language, SyntaxTree};

/// Parses `source` and returns its highlight spans.
///
/// The two halves are separate on purpose — one owner for the tree, one set
/// of rules for the colours — so every test here goes through the same
/// two-step the real callers do.
fn spans_for(language: Language, source: &str) -> Vec<HighlightSpan> {
    let mut tree = SyntaxTree::new(language).expect("every language parses");
    let parsed = tree.parse(source).expect("a parse must produce a tree");
    Highlighter::new(language)
        .expect("every language has a highlights query")
        .spans_in(parsed, source)
}
