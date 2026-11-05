use std::cell::Cell;
use std::fmt::Write as _;

use tree_sitter::{InputEdit, Point};

use super::{ParseOutcome, SyntaxParser, changed_ranges};
use crate::{Language, SyntaxTree};

/// Reads `text` in pieces of about `size` bytes, the way a rope hands out its
/// chunks: each piece ends on a character boundary, so one longer than `size`
/// when a multi-byte character straddles the cut.
fn pieces<'text>(text: &'text str, size: usize) -> impl FnMut(usize) -> &'text [u8] {
    move |byte| {
        let start = byte.min(text.len());
        let mut end = start.saturating_add(size).min(text.len());
        while !text.is_char_boundary(end) {
            end += 1;
        }
        &text.as_bytes()[start..end]
    }
}

fn parsed(outcome: ParseOutcome) -> tree_sitter::Tree {
    match outcome {
        ParseOutcome::Parsed(tree) => tree,
        other => panic!("expected a tree, got {other:?}"),
    }
}

#[test]
fn a_parse_read_in_pieces_matches_a_parse_of_the_whole_string() {
    // Small pieces put chunk boundaries right beside the multi-byte
    // characters below, as a rope's chunk boundaries do in real text.
    let source = "fn main() {\n    let café = \"naïve → ok\";\n    println!(\"{café}\");\n}\n";
    let mut whole = SyntaxTree::new(Language::Rust).expect("Rust is linked");
    let expected = whole
        .parse(source)
        .expect("valid Rust parses")
        .root_node()
        .to_sexp();

    let mut parser = SyntaxParser::new(Language::Rust).expect("Rust is linked");
    for size in [1, 2, 3, 7, 64, 4096] {
        let tree = parsed(parser.parse(pieces(source, size), None, || true));
        assert_eq!(
            tree.root_node().to_sexp(),
            expected,
            "pieces of {size} bytes"
        );
        assert_eq!(tree.root_node().end_byte(), source.len());
    }
}

#[test]
fn an_incremental_parse_read_in_pieces_matches_a_fresh_one() {
    let before = "fn a() {\n    let x = 1;\n}\nfn b() {}\n";
    let after = "fn a() {\n    let xy = 1;\n}\nfn b() {}\n";
    let mut parser = SyntaxParser::new(Language::Rust).expect("Rust is linked");
    let mut old = parsed(parser.parse(pieces(before, 5), None, || true));
    // `x` becomes `xy`: one byte inserted after it, at row 1, column 9.
    old.edit(&InputEdit {
        start_byte: 18,
        old_end_byte: 18,
        new_end_byte: 19,
        start_position: Point { row: 1, column: 9 },
        old_end_position: Point { row: 1, column: 9 },
        new_end_position: Point { row: 1, column: 10 },
    });
    let reparsed = parsed(parser.parse(pieces(after, 5), Some(&old), || true));
    let fresh = parsed(parser.parse(pieces(after, 5), None, || true));
    assert_eq!(reparsed.root_node().to_sexp(), fresh.root_node().to_sexp());
    // Renaming an identifier changes no structure.
    assert!(changed_ranges(&old, &reparsed).is_empty());
}

#[test]
fn a_parse_asked_to_stop_stops_and_the_next_one_starts_afresh() {
    let mut source = String::new();
    for index in 0..20_000 {
        let _ = writeln!(source, "fn f{index}() {{ let v = {index}; }}");
    }
    let mut parser = SyntaxParser::new(Language::Rust).expect("Rust is linked");
    let asked = Cell::new(0_u32);
    let outcome = parser.parse(pieces(&source, 4096), None, || {
        asked.set(asked.get() + 1);
        false
    });
    assert!(matches!(outcome, ParseOutcome::Cancelled), "{outcome:?}");
    assert!(asked.get() >= 1);

    // A different text: had the cancelled parse been resumed rather than
    // reset, the tree would describe the old one.
    let other = "fn only() {}\n";
    let tree = parsed(parser.parse(pieces(other, 4096), None, || true));
    assert_eq!(tree.root_node().end_byte(), other.len());
    assert_eq!(tree.root_node().named_child_count(), 1);
}

#[test]
fn a_language_without_a_grammar_is_refused_by_name() {
    let Some(language) = Language::all()
        .iter()
        .copied()
        .find(|language| !crate::has_grammar(*language))
    else {
        return;
    };
    let error = SyntaxParser::new(language).expect_err("no grammar is linked");
    assert!(error.to_string().contains(language.id()), "{error}");
}
