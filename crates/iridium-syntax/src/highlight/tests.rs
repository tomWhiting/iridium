//! Tests for capture-name mapping and span production.

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

/// A fixture with a multi-line string literal, plus the extents of that
/// string's span — the shape every boundary question below is asked about.
fn multiline_string_fixture() -> (&'static str, Vec<HighlightSpan>, HighlightSpan) {
    let source = "fn main() {\n    let s = \"line one\n     line two\n     line three\";\n    let n = 42;\n}\n";
    let whole = spans_for(Language::Rust, source);
    let inside_string = source
        .find("line two")
        .expect("the fixture contains its own text");
    let straddler = whole
        .iter()
        .find(|span| {
            span.highlight == HighlightType::String
                && span.start < inside_string
                && span.end > inside_string
        })
        .cloned()
        .expect("the multi-line string parses as one string span");
    assert!(
        source[straddler.start..straddler.end].contains('\n'),
        "the fixture's string span must actually cross lines"
    );
    (source, whole, straddler)
}

/// Parses the fixture once and hands back everything a range query needs.
fn ranged_setup() -> (SyntaxTree, Highlighter) {
    let tree = SyntaxTree::new(Language::Rust).expect("rust parses");
    let highlighter = Highlighter::new(Language::Rust).expect("rust has highlights");
    (tree, highlighter)
}

#[test]
fn spans_in_range_over_the_whole_document_matches_spans_in() {
    let (source, whole, _) = multiline_string_fixture();
    let (mut tree, highlighter) = ranged_setup();
    let parsed = tree.parse(source).expect("the fixture parses");
    assert_eq!(
        highlighter.spans_in_range(parsed, source, 0..source.len()),
        whole,
        "an unrestricted range is the whole-document walk, span for span"
    );
}

#[test]
fn spans_in_range_yields_straddling_spans_with_full_extents() {
    let (source, whole, straddler) = multiline_string_fixture();
    let (mut tree, highlighter) = ranged_setup();
    let parsed = tree.parse(source).expect("the fixture parses");

    // A window cut through the middle of the string: both of its edges fall
    // strictly inside the straddling span.
    let window = (straddler.start + 3)..(straddler.end - 3);
    let ranged = highlighter.spans_in_range(parsed, source, window.clone());

    assert!(
        ranged.contains(&straddler),
        "a span straddling both window edges is yielded whole, unclamped"
    );
    for span in &ranged {
        assert!(
            whole.contains(span),
            "the ranged walk invents nothing: every span it yields is one \
             the whole-document walk produces"
        );
    }
    // Completeness at the seam: every whole-document span lying entirely
    // inside the window is present in the ranged answer.
    for span in whole
        .iter()
        .filter(|span| span.start >= window.start && span.end <= window.end)
    {
        assert!(
            ranged.contains(span),
            "a span wholly inside the window must not be dropped: {span:?}"
        );
    }
}

#[test]
fn spans_in_range_intersecting_matches_only_touch_the_window() {
    // The contract's second consequence, held to: matches are returned when
    // they *intersect* the window, so every yielded span belongs to a match
    // touching it — and spans of matches nowhere near the window stay out.
    let (source, _, straddler) = multiline_string_fixture();
    let (mut tree, highlighter) = ranged_setup();
    let parsed = tree.parse(source).expect("the fixture parses");

    // A window over the string's interior only: the answer must not contain
    // the `42` literal, whose match lies entirely past the window.
    let number_at = source
        .find("42")
        .expect("the fixture contains its own text");
    let window = (straddler.start + 3)..(straddler.end - 3);
    let ranged = highlighter.spans_in_range(parsed, source, window);
    assert!(
        !ranged
            .iter()
            .any(|span| span.highlight == HighlightType::Number && span.start == number_at),
        "a match wholly outside the window is not yielded"
    );
}

#[test]
fn spans_in_range_with_an_empty_range_yields_nothing() {
    let (source, _, _) = multiline_string_fixture();
    let (mut tree, highlighter) = ranged_setup();
    let parsed = tree.parse(source).expect("the fixture parses");
    assert!(
        highlighter.spans_in_range(parsed, source, 5..5).is_empty(),
        "an empty range yields no spans"
    );
    // Spelled as struct syntax: an inverted range *literal* is itself a lint,
    // and rightly so — this test exists to pin what happens when one arrives
    // anyway.
    let inverted = std::ops::Range { start: 10, end: 2 };
    assert!(
        highlighter
            .spans_in_range(parsed, source, inverted)
            .is_empty(),
        "an inverted range is empty, never the whole document"
    );
}

#[test]
fn test_tsx() {
    let language = Language::Tsx;
    let source = "const App = () => <div>Hello</div>;";
    let spans = spans_for(language, source);

    assert!(!spans.is_empty(), "TSX code should produce highlight spans");
}

/// A `.jsonc` document is parsed by the JSON grammar, and its comments survive.
///
/// This is the fact that made `Language::from_extension("jsonc")` resolve to
/// [`Language::Json`], and it is a property of the vendored grammar rather than
/// of anything in this repository: tree-sitter-json lists `comment` in its
/// `extras`, so both `//` and `/* */` are legal wherever whitespace is. A
/// grammar bump that dropped the rule would turn every commented configuration
/// file into an error tree with no warning anywhere else, which is what this
/// test exists to prevent.
#[test]
fn jsonc_is_the_json_grammar_and_its_comments_parse() {
    const SOURCE: &str =
        "{\n  // a line comment\n  \"a\": 1,\n  /* a block\n     comment */\n  \"b\": [2]\n}\n";

    let mut tree = SyntaxTree::new(Language::Json).expect("json parses");
    let parsed = tree.parse(SOURCE).expect("a parse must produce a tree");

    assert!(
        !parsed.root_node().has_error(),
        "a commented JSON document must parse cleanly, not recover: {}",
        parsed.root_node().to_sexp()
    );

    let comments: Vec<&str> = Highlighter::new(Language::Json)
        .expect("json ships a highlights query")
        .spans_in(parsed, SOURCE)
        .iter()
        .filter(|span| span.highlight == HighlightType::Comment)
        .map(|span| &SOURCE[span.start..span.end])
        .collect();

    assert_eq!(
        comments,
        vec!["// a line comment", "/* a block\n     comment */"],
        "both comment forms must highlight, and cover exactly their own text"
    );
}

/// A trailing comma is recovered from locally, and never mis-colours anything.
///
/// Trailing commas are the other half of what people mean by JSONC, and the
/// JSON grammar does *not* accept them — unlike comments, which it does. The
/// question that mattered was not whether the tree is clean (it is not) but
/// whether the damage is contained, because wrong highlighting is worse than
/// none. It is contained: tree-sitter's recovery marks the missing value and
/// carries on, so every real token either side still highlights as itself.
#[test]
fn a_trailing_comma_costs_an_error_node_and_nothing_else() {
    const SOURCE: &str = "{\n  \"a\": 1,\n  \"b\": [2, 3,],\n}\n";

    let mut tree = SyntaxTree::new(Language::Json).expect("json parses");
    let parsed = tree.parse(SOURCE).expect("a parse must produce a tree");
    assert!(
        parsed.root_node().has_error(),
        "the JSON grammar rejects trailing commas; if this ever passes, the \
         grammar gained JSON5-style tolerance and this test should say so"
    );

    let spans = Highlighter::new(Language::Json)
        .expect("json ships a highlights query")
        .spans_in(parsed, SOURCE);

    // The keys and the numbers either side of the offending commas are still
    // themselves. Recovery that swallowed them would show as a missing span.
    for (text, highlight) in [
        ("\"a\"", HighlightType::Property),
        ("\"b\"", HighlightType::Property),
    ] {
        let start = SOURCE.find(text).expect("the fixture contains it");
        assert!(
            spans.iter().any(|span| span.start == start
                && span.end == start + text.len()
                && span.highlight == highlight),
            "{text} lost its {highlight:?} span to error recovery"
        );
    }

    assert!(
        spans
            .iter()
            .all(|span| span.start < span.end && span.end <= SOURCE.len()),
        "recovery must not produce a degenerate or out-of-bounds span"
    );
}
