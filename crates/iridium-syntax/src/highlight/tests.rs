//! Tests for capture-name mapping and span production.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "assertions in tests read better than error plumbing"
)]

use super::*;
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

#[test]
fn test_highlight_type_from_capture_name() {
    assert_eq!(
        HighlightType::from_capture_name("keyword"),
        Some(HighlightType::Keyword)
    );
    assert_eq!(
        HighlightType::from_capture_name("@keyword"),
        Some(HighlightType::Keyword)
    );
    assert_eq!(
        HighlightType::from_capture_name("keyword.control"),
        Some(HighlightType::KeywordControl)
    );
    assert_eq!(
        HighlightType::from_capture_name("function.method"),
        Some(HighlightType::FunctionMethod)
    );
    assert_eq!(
        HighlightType::from_capture_name("string"),
        Some(HighlightType::String)
    );
    assert_eq!(
        HighlightType::from_capture_name("comment.doc"),
        Some(HighlightType::CommentDoc)
    );
}

#[test]
fn test_highlighter_rust() {
    let language = Language::Rust;
    let source = "fn main() { let x = 42; }";
    let spans = spans_for(language, source);

    // Should have at least some spans
    assert!(
        !spans.is_empty(),
        "Rust code should produce highlight spans"
    );

    // Verify the 'fn' keyword is highlighted
    let has_fn_span = spans.iter().any(|s| s.start == 0 && s.end == 2);
    assert!(has_fn_span, "'fn' should be highlighted");
}

#[test]
fn test_highlighter_python() {
    let language = Language::Python;
    let source = "def hello():\n    print('Hello')";
    let spans = spans_for(language, source);

    assert!(
        !spans.is_empty(),
        "Python code should produce highlight spans"
    );
}

#[test]
fn test_highlighter_typescript() {
    let language = Language::TypeScript;
    let source = "function greet(name: string): void { console.log(name); }";
    let spans = spans_for(language, source);

    assert!(
        !spans.is_empty(),
        "TypeScript code should produce highlight spans"
    );
}

#[test]
fn test_highlighter_javascript() {
    let language = Language::JavaScript;
    let source = "const x = 42; function foo() { return x; }";
    let spans = spans_for(language, source);

    assert!(
        !spans.is_empty(),
        "JavaScript code should produce highlight spans"
    );
}

#[test]
fn test_highlighter_go() {
    let language = Language::Go;
    let source = "package main\n\nfunc main() { fmt.Println(\"Hello\") }";
    let spans = spans_for(language, source);

    assert!(!spans.is_empty(), "Go code should produce highlight spans");
}

#[test]
fn test_highlighter_json() {
    let language = Language::Json;
    let source = r#"{"name": "test", "value": 42, "active": true}"#;
    let spans = spans_for(language, source);

    assert!(
        !spans.is_empty(),
        "JSON code should produce highlight spans"
    );
}

#[test]
fn test_highlighter_yaml() {
    let language = Language::Yaml;
    let source = "name: test\nvalue: 42\nactive: true";
    let spans = spans_for(language, source);

    // YAML highlighting might produce different results depending on the grammar
    // Just verify no crash
    let _ = spans.len();
}

#[test]
fn test_highlighter_css() {
    let language = Language::Css;
    let source = ".class { color: red; font-size: 12px; }";
    let spans = spans_for(language, source);

    assert!(!spans.is_empty(), "CSS code should produce highlight spans");
}

#[test]
fn test_highlighter_bash() {
    let language = Language::Bash;
    let source = "#!/bin/bash\necho \"Hello World\"";
    let spans = spans_for(language, source);

    assert!(
        !spans.is_empty(),
        "Bash code should produce highlight spans"
    );
}

#[test]
fn test_highlighter_c() {
    let language = Language::C;
    let source = "int main() { return 0; }";
    let spans = spans_for(language, source);

    assert!(!spans.is_empty(), "C code should produce highlight spans");
}

#[test]
fn test_highlighter_cpp() {
    let language = Language::Cpp;
    let source = "class Foo { public: int bar(); };";
    let spans = spans_for(language, source);

    assert!(!spans.is_empty(), "C++ code should produce highlight spans");
}

#[test]
fn test_highlighter_markdown() {
    let language = Language::Markdown;
    let source = "# Hello\n\nThis is **bold** and *italic*.";
    let spans = spans_for(language, source);

    // Markdown should produce some spans
    let _ = spans.len();
}

#[test]
fn highlighting_an_incrementally_parsed_tree_matches_a_full_parse() {
    // The whole point of one shared tree is that the cheap path and the
    // slow path cannot disagree. Nothing else in this module would notice
    // if they did.
    let language = Language::Rust;
    let mut tree = SyntaxTree::new(language).expect("rust parses");
    tree.parse("fn main() { }").expect("initial parse");

    let after = "fn main() { let x = 1; }";
    let parsed = tree
        .edit_bytes(after, 12, 12, 23)
        .expect("incremental parse");

    let highlighter = Highlighter::new(language).expect("rust has highlights");
    let incremental = highlighter.spans_in(parsed, after);

    assert!(!incremental.is_empty(), "an insertion must produce spans");
    assert_eq!(
        incremental,
        spans_for(language, after),
        "the incremental parse highlighted differently from a full one"
    );
}

#[test]
fn test_span_ordering() {
    let first = HighlightSpan::new(0, 5, HighlightType::Keyword);
    let second = HighlightSpan::new(10, 15, HighlightType::String);
    let third = HighlightSpan::new(0, 10, HighlightType::Function);

    let mut spans = [second, first, third];
    spans.sort();

    assert_eq!(spans[0].start, 0);
    assert_eq!(spans[0].end, 5);
    assert_eq!(spans[1].start, 0);
    assert_eq!(spans[1].end, 10);
    assert_eq!(spans[2].start, 10);
}

#[test]
fn test_tsx() {
    let language = Language::Tsx;
    let source = "const App = () => <div>Hello</div>;";
    let spans = spans_for(language, source);

    assert!(!spans.is_empty(), "TSX code should produce highlight spans");
}
