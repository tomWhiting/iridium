//! Native coverage and byte offsets for mixed-language documents.

use crate::{HighlightType, Highlighter, Language, SyntaxTree, has_grammar};

#[test]
fn every_chiron_language_and_xml_has_a_native_parser() {
    let expected = [
        "bash",
        "c",
        "cpp",
        "css",
        "cypher",
        "diff",
        "gitcommit",
        "glsl",
        "go",
        "gomod",
        "gowork",
        "html",
        "java",
        "javascript",
        "jsdoc",
        "json",
        "just",
        "lua",
        "luau",
        "markdown",
        "markdown-inline",
        "nix",
        "nu",
        "proto",
        "python",
        "query",
        "regex",
        "rust",
        "sql",
        "toml",
        "tsx",
        "typescript",
        "typst",
        "wgsl",
        "yaml",
        "zig",
        "xml",
        "dtd",
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
    let tree = syntax
        .parse(source)
        .ok_or("Markdown parse produced no tree")?;
    let spans = Highlighter::new(Language::Markdown)?.spans_in(tree, source);
    let start = source.find("fn main").ok_or("fixture has no function")?;
    assert!(
        spans.iter().any(|span| {
            span.start == start && span.end == start + 2 && span.highlight == HighlightType::Keyword
        }),
        "the fenced function keyword must retain its document byte offsets"
    );
    Ok(())
}

/// Lua, Luau and Nix: a file of each is recognised by its name, parses
/// without an error node, and colours its first keyword as a keyword.
///
/// The tables elsewhere prove the queries compile and every capture maps; this
/// proves the three pieces meet on a real file. Luau's fixture carries a type
/// annotation, which plain Lua cannot parse, so it would fail if `.luau` were
/// ever parsed with the Lua grammar.
#[test]
fn lua_luau_and_nix_files_parse_and_colour_their_keywords() -> Result<(), Box<dyn std::error::Error>>
{
    let fixtures = [
        (
            "init.lua",
            Language::Lua,
            "local function greet(name)\n  return \"hi \" .. name\nend\n",
            "local",
        ),
        (
            "greet.luau",
            Language::Luau,
            "local function greet(name: string): string\n  return `hi {name}`\nend\n",
            "local",
        ),
        (
            "flake.nix",
            Language::Nix,
            "let\n  name = \"iridium\";\nin\n{ inherit name; path = ./src; }\n",
            "let",
        ),
    ];
    for (file, language, source, keyword) in fixtures {
        assert_eq!(
            Language::for_path(std::path::Path::new(file)),
            Some(language),
            "{file} is not recognised"
        );
        let mut syntax = SyntaxTree::new(language)?;
        let tree = syntax
            .parse(source)
            .ok_or_else(|| format!("{file} produced no tree"))?;
        assert!(
            !tree.root_node().has_error(),
            "{file} parsed with an error: {}",
            tree.root_node().to_sexp()
        );
        let spans = Highlighter::new(language)?.spans_in(tree, source);
        assert!(
            spans.iter().any(|span| {
                span.start == 0
                    && span.end == keyword.len()
                    && matches!(
                        span.highlight,
                        HighlightType::Keyword | HighlightType::KeywordControl
                    )
            }),
            "{file}'s leading `{keyword}` is not coloured as a keyword: {spans:?}"
        );
    }
    Ok(())
}
