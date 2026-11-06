//! Embedded captures retain document byte offsets and respect the paint window.
//!
//! The outer document is read through a text callback, piece by piece, so a
//! rope never has to be built into one string to find what it embeds. Only
//! each injected range is copied out: the embedded grammar parses it, and a
//! parser needs its source contiguous.
use std::borrow::Cow;
use std::ops::Range;

use tree_sitter::{Node, Parser, QueryCursor, StreamingIterator, Tree};

use super::{HighlightSpan, Highlighter};
use crate::{
    Language, SyntaxError,
    grammar::grammar,
    query::{self, QueryKind},
};

/// Appends the highlights of what `source` embeds within `window`.
pub(super) fn append(
    language: Language,
    tree: &Tree,
    source: &str,
    window: Range<usize>,
    spans: &mut Vec<HighlightSpan>,
) -> Result<(), SyntaxError> {
    append_at(language, tree, source, window, spans, 0)
}

/// [`append`] over text held in pieces: `text(bytes)` yields, in order, the
/// pieces of that byte range of the text `tree` was parsed from.
pub(super) fn append_chunked<'text, F, C>(
    language: Language,
    tree: &Tree,
    text: &F,
    window: Range<usize>,
    spans: &mut Vec<HighlightSpan>,
) -> Result<(), SyntaxError>
where
    F: Fn(Range<usize>) -> C,
    C: Iterator<Item = &'text str>,
{
    let length = tree.root_node().end_byte();
    append_with(language, tree, text, length, window, spans, 0)
}

/// [`append`] at nesting `depth`, over a source already held whole: the
/// embedded text of the level above.
fn append_at(
    language: Language,
    tree: &Tree,
    source: &str,
    window: Range<usize>,
    spans: &mut Vec<HighlightSpan>,
    depth: usize,
) -> Result<(), SyntaxError> {
    let whole = |bytes: Range<usize>| source.get(bytes).into_iter();
    append_with(language, tree, &whole, source.len(), window, spans, depth)
}

/// The walk itself, reading the outer text through `text`, which covers
/// `length` bytes.
///
/// Not called with itself as the reader: a nested level goes through
/// [`append_at`], so the reader's type is fixed one level down and the
/// recursion has a finite set of instances.
fn append_with<'text, F, C>(
    language: Language,
    tree: &Tree,
    text: &F,
    length: usize,
    window: Range<usize>,
    spans: &mut Vec<HighlightSpan>,
    depth: usize,
) -> Result<(), SyntaxError>
where
    F: Fn(Range<usize>) -> C,
    C: Iterator<Item = &'text str>,
{
    let Some(query) = query::compiled(language, QueryKind::Injections)? else {
        return Ok(());
    };
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(window.clone());
    let names = query.capture_names();
    let mut matches = cursor.matches(query, tree.root_node(), |node: Node<'_>| {
        text(node.byte_range()).map(str::as_bytes)
    });
    while let Some(matched) = matches.next() {
        let mut label = query
            .property_settings(matched.pattern_index)
            .iter()
            .find(|property| property.key.as_ref() == "injection.language")
            .and_then(|property| property.value.as_deref())
            .map(Cow::Borrowed);
        for capture in matched.captures {
            if names.get(capture.index as usize) == Some(&"injection.language") {
                label = Some(Cow::Owned(text(capture.node.byte_range()).collect()));
            }
        }
        let Some(label) = label else {
            continue;
        };
        let label = label.trim_matches(['\'', '"', '`']);
        let embedded = if label.eq_ignore_ascii_case("jsonc") {
            Some(Language::Json)
        } else {
            Language::from_id(label)
        };
        let Some(embedded) = embedded else {
            continue;
        };
        let Some(grammar) = grammar(embedded) else {
            continue;
        };
        let highlighter = Highlighter::new(embedded)?;
        for capture in matched.captures {
            if names.get(capture.index as usize) != Some(&"injection.content") {
                continue;
            }
            let range = capture.node.byte_range();
            if range.is_empty() || range.end.min(window.end) <= range.start.max(window.start) {
                continue;
            }
            if depth >= 16 || (embedded == language && range == (0..length)) {
                return Err(SyntaxError::QueryError {
                    message: format!("Cyclic or excessively nested {} injection", embedded.id()),
                });
            }
            let content: String = text(range.clone()).collect();
            if content.len() != range.len() {
                return Err(SyntaxError::ParseError {
                    message: "Injection range is outside UTF-8 source".to_owned(),
                });
            }
            let mut parser = Parser::new();
            parser
                .set_language(&grammar)
                .map_err(|error| SyntaxError::ParseError {
                    message: format!("Cannot load {} injection: {error}", embedded.id()),
                })?;
            let parsed = parser
                .parse(&content, None)
                .ok_or_else(|| SyntaxError::ParseError {
                    message: format!("{} injection parse produced no tree", embedded.id()),
                })?;
            let local =
                window.start.saturating_sub(range.start)..window.end.min(range.end) - range.start;
            let mut query_cursor = QueryCursor::new();
            query_cursor.set_byte_range(local.clone());
            let mut injected = highlighter.spans_with(query_cursor, &parsed, content.as_bytes());
            append_at(embedded, &parsed, &content, local, &mut injected, depth + 1)?;
            // The outer raw-code capture must not hide the embedded tokens.
            spans.retain(|span| !(span.start <= range.start && span.end >= range.end));
            spans.extend(injected.into_iter().map(|span| {
                HighlightSpan::new(
                    span.start + range.start,
                    span.end + range.start,
                    span.highlight,
                )
            }));
        }
    }
    Ok(())
}
