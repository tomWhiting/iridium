//! Embedded captures retain document byte offsets and respect the paint window.
use std::ops::Range;

use tree_sitter::{Parser, QueryCursor, StreamingIterator, Tree};

use crate::{Language, SyntaxError, grammar::grammar, query::{self, QueryKind}};
use super::{Highlighter, HighlightSpan};

pub(super) fn append(language: Language, tree: &Tree, source: &str, window: Range<usize>, spans: &mut Vec<HighlightSpan>) -> Result<(), SyntaxError> {
    append_at(language, tree, source, window, spans, 0)
}

fn append_at(language: Language, tree: &Tree, source: &str, window: Range<usize>, spans: &mut Vec<HighlightSpan>, depth: usize) -> Result<(), SyntaxError> {
    let Some(query) = query::compiled(language, QueryKind::Injections)? else { return Ok(()); };
    let mut cursor = QueryCursor::new();
    cursor.set_byte_range(window.clone());
    let names = query.capture_names();
    let mut matches = cursor.matches(query, tree.root_node(), source.as_bytes());
    while let Some(matched) = matches.next() {
        let mut label = query.property_settings(matched.pattern_index).iter()
            .find(|property| property.key.as_ref() == "injection.language")
            .and_then(|property| property.value.as_deref());
        for capture in matched.captures {
            if names.get(capture.index as usize) == Some(&"injection.language") {
                label = source.get(capture.node.byte_range());
            }
        }
        let Some(label) = label else { continue; };
        let label = label.trim_matches(['\'', '"', '`']);
        let embedded = if label.eq_ignore_ascii_case("jsonc") { Some(Language::Json) } else { Language::from_id(label) };
        let Some(embedded) = embedded else { continue; };
        let Some(grammar) = grammar(embedded) else { continue; };
        let highlighter = Highlighter::new(embedded)?;
        for capture in matched.captures {
            if names.get(capture.index as usize) != Some(&"injection.content") { continue; }
            let range = capture.node.byte_range();
            if range.start >= range.end || range.end <= window.start || range.start >= window.end { continue; }
            if depth >= 16 || (embedded == language && range == (0..source.len())) {
                return Err(SyntaxError::QueryError { message: format!("Cyclic or excessively nested {} injection", embedded.id()) });
            }
            let text = source.get(range.clone()).ok_or_else(|| SyntaxError::ParseError { message: "Injection range is outside UTF-8 source".to_owned() })?;
            let mut parser = Parser::new();
            parser.set_language(&grammar).map_err(|error| SyntaxError::ParseError { message: format!("Cannot load {} injection: {error}", embedded.id()) })?;
            let parsed = parser.parse(text, None).ok_or_else(|| SyntaxError::ParseError { message: format!("{} injection parse produced no tree", embedded.id()) })?;
            let local = window.start.saturating_sub(range.start)..window.end.min(range.end) - range.start;
            let mut query_cursor = QueryCursor::new();
            query_cursor.set_byte_range(local.clone());
            let mut injected = highlighter.spans_with(query_cursor, &parsed, text);
            append_at(embedded, &parsed, text, local, &mut injected, depth + 1)?;
            // The outer raw-code capture must not hide the embedded tokens.
            spans.retain(|span| !(span.start <= range.start && span.end >= range.end));
            spans.extend(injected.into_iter().map(|span| HighlightSpan::new(span.start + range.start, span.end + range.start, span.highlight)));
        }
    }
    Ok(())
}
