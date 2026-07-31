//! Named regions — functions, classes, comments — located by vendored queries.
//!
//! Where [`crate::navigate`] walks the tree by *shape*, this module finds
//! regions by *meaning*. "The function I am in" is not a fact about tree
//! structure that holds across grammars: it is `function_item` in Rust,
//! `function_declaration` in Go and `function_definition` in Python, and the
//! `textobjects.scm` files are where that mapping already lives.
//!
//! # What the vendored queries actually offer
//!
//! Five captures, and no more — verified against the files in
//! `src/languages/queries/`, not assumed from what other editors ship:
//! `@function.inside`, `@function.around`, `@class.inside`, `@class.around`
//! and `@comment.around`. There is **no** parameter, argument, block or call
//! text object, and no `@comment.inside`. Anything richer means authoring new
//! `.scm` per language, which is a separate piece of work.
//!
//! Coverage is uneven and the gaps are real, not oversights:
//!
//! | Language | Has |
//! |---|---|
//! | rust, python, typescript, javascript, tsx, go, css, c, cpp | all five |
//! | bash | function and comment; no class |
//! | markdown | class only — and a "class" is a **section**, which is what makes jump-by-class the heading navigator for prose |
//! | json, yaml | comment only |
//!
//! So every entry point returns `Ok(None)` for a combination a language does
//! not have. That is a routine answer meaning "this file has no such thing",
//! never an error.

use std::ops::Range;

use tree_sitter::{QueryCursor, StreamingIterator, Tree};

use super::{QueryKind, compiled};
use crate::{Language, SyntaxError};

/// A kind of named region a grammar knows how to point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextObject {
    /// A function, method, closure or procedure.
    Function,
    /// A class, struct, enum, interface — or, in Markdown, a section.
    Class,
    /// A comment. Only [`Variant::Around`] exists; see [`TextObject::capture`].
    Comment,
}

/// Which part of a region to take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    /// The body alone, without the signature or the delimiters.
    Inside,
    /// The whole construct, including its signature and delimiters.
    Around,
}

/// Which way [`jump`] travels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    /// Towards the end of the document.
    Forward,
    /// Towards its start.
    Backward,
}

impl TextObject {
    /// The capture name for this object and variant, if the queries define one.
    ///
    /// `None` for `(Comment, Inside)`, which no vendored query provides. That
    /// absence is expressed here, once, rather than left for each caller to
    /// remember — asking for the inside of a comment is answerable, and the
    /// answer is "there is no such capture".
    #[must_use]
    pub const fn capture(self, variant: Variant) -> Option<&'static str> {
        match (self, variant) {
            (Self::Function, Variant::Inside) => Some("function.inside"),
            (Self::Function, Variant::Around) => Some("function.around"),
            (Self::Class, Variant::Inside) => Some("class.inside"),
            (Self::Class, Variant::Around) => Some("class.around"),
            (Self::Comment, Variant::Around) => Some("comment.around"),
            (Self::Comment, Variant::Inside) => None,
        }
    }
}

/// Returns the smallest region of `object` that covers `range`.
///
/// "The function I am in", and the verb behind *Select Function Inside/Around*.
///
/// When `range` already matches a region exactly, the next region out is
/// returned instead — so a second press walks out of a closure into the method
/// holding it rather than doing nothing. That is the same rule
/// [`crate::navigate::expand`] follows, for the same reason: a key that dies on
/// its second press reads as broken.
///
/// `Ok(None)` means there is nothing to select — the language has no such
/// capture, or the caret is not inside one.
///
/// # Errors
///
/// [`SyntaxError::QueryError`] if the vendored query does not compile against
/// its grammar. Callers driving a keypress should treat that as `None`; the
/// compile-every-query test is what is meant to catch it first.
pub fn find(
    language: Language,
    tree: &Tree,
    source: &str,
    range: &Range<usize>,
    object: TextObject,
    variant: Variant,
) -> Result<Option<Range<usize>>, SyntaxError> {
    let regions = regions(language, tree, source, object, variant)?;

    // Smallest first, so the first covering region found is the innermost one.
    let mut covering: Vec<Range<usize>> = regions
        .into_iter()
        .filter(|region| region.start <= range.start && range.end <= region.end)
        .collect();
    covering.sort_by_key(|region| (region.end - region.start, region.start));

    Ok(covering
        .iter()
        .find(|region| *region != range)
        .or_else(|| covering.first())
        .cloned())
}

/// Returns the region of `object` nearest to `from` in `direction`.
///
/// The verb behind *Next Function* / *Previous Class*. Always the `around`
/// extent: jumping to a function means arriving at the whole thing, not at the
/// inside of its body.
///
/// Strictly past `from`, so holding the key advances instead of sticking on the
/// region already under the caret. `Ok(None)` means there is nothing further in
/// that direction.
///
/// # Errors
///
/// As [`find`].
pub fn jump(
    language: Language,
    tree: &Tree,
    source: &str,
    from: usize,
    object: TextObject,
    direction: Direction,
) -> Result<Option<Range<usize>>, SyntaxError> {
    let regions = regions(language, tree, source, object, Variant::Around)?;

    Ok(match direction {
        Direction::Forward => regions
            .into_iter()
            .filter(|region| region.start > from)
            .min_by_key(|region| region.start),
        Direction::Backward => regions
            .into_iter()
            .filter(|region| region.start < from)
            .max_by_key(|region| region.start),
    })
}

/// Every region of `object`/`variant` in the tree, in no particular order.
///
/// Deduplicated: a grammar may capture the same node from more than one
/// pattern, and a caller counting regions should not see the same bytes twice.
///
/// # Errors
///
/// As [`find`].
pub fn regions(
    language: Language,
    tree: &Tree,
    source: &str,
    object: TextObject,
    variant: Variant,
) -> Result<Vec<Range<usize>>, SyntaxError> {
    let Some(capture) = object.capture(variant) else {
        return Ok(Vec::new());
    };
    let Some(query) = compiled(language, QueryKind::TextObjects)? else {
        return Ok(Vec::new());
    };

    // Resolved once per call rather than per match: the name-to-index lookup is
    // a linear scan over the query's capture table, and a large file can hold
    // thousands of matches.
    let wanted: Vec<u32> = query
        .capture_names()
        .iter()
        .enumerate()
        .filter(|(_, name)| **name == capture)
        .filter_map(|(index, _)| u32::try_from(index).ok())
        .collect();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    let mut cursor = QueryCursor::new();
    // tree-sitter 0.26 returns a `StreamingIterator`, not an `Iterator`.
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());

    while let Some(matched) = matches.next() {
        for capture in matched.captures {
            if !wanted.contains(&capture.index) {
                continue;
            }
            let range = capture.node.byte_range();
            if !range.is_empty() {
                found.push(range);
            }
        }
    }

    found.sort_by_key(|range| (range.start, range.end));
    found.dedup();
    Ok(found)
}

#[cfg(test)]
mod textobject_tests;
