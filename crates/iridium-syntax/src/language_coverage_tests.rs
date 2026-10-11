//! Native coverage and byte offsets for mixed-language documents.

use crate::{HighlightType, Highlighter, Language, SyntaxTree, has_grammar};

#[test]
fn every_chiron_language_and_xml_has_a_native_parser() {
    let expected = [
        "bash", "c", "cpp", "css", "cypher", "diff", "gitcommit", "glsl", "go",
        "gomod", "gowork", "html", "java", "javascript", "jsdoc", "json", "just",
        "markdown", "markdown-inline", "nu", "proto", "python", "query", "regex", "rust",
        "sql", "toml", "tsx", "typescript", "typst", "wgsl", "yaml", "zig", "xml", "dtd",
    ];
    let missing: Vec<_> = expected
        .into_iter()
        .filter(|id| Language::from_id(id).is_none_or(|language| !has_grammar(language)))
        .collect();
    assert!(missing.is_empty(), "missing native parsers: {missing:?}");
}

#[test]
fn markdown_fenced_code_uses_embedded_keyword_colours() -> Result<(), Box<dyn std::error::Error>> {
    let source = "Intro café\n\n```rust\nfn main() {}\n```\n";
    let mut syntax = SyntaxTree::new(Language::Markdown)?;
    let tree = syntax.parse(source).ok_or("Markdown parse produced no tree")?;
    let spans = Highlighter::new(Language::Markdown)?.spans_in(tree, source);
    let start = source.find("fn main").ok_or("fixture has no function")?;
    assert!(spans.iter().any(|span| {
        span.start == start && span.end == start + 2 && span.highlight == HighlightType::Keyword
    }), "the fenced function keyword must retain its document byte offsets");
    Ok(())
}
