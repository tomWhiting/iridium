//! Tests for the retained parse tree.
//!
//! The oracle that matters is [`an_incremental_reparse_matches_a_parse_from_scratch`]:
//! an incremental parse is only worth having if it produces the same tree the
//! slow path would, and every other guarantee in this module rests on that one.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "assertions in tests read better than error plumbing"
)]

use tree_sitter::{InputEdit, Point};

use super::{SyntaxTree, byte_point};
use crate::Language;

/// A tree for Rust, which every assertion here uses.
fn rust() -> SyntaxTree {
    SyntaxTree::new(Language::Rust).expect("rust must be parseable")
}

/// The tree's shape as text, for comparing two parses of the same document.
fn shape(tree: &SyntaxTree) -> String {
    tree.root().expect("a parsed tree has a root").to_sexp()
}

#[test]
fn a_new_tree_holds_nothing_until_it_parses() {
    let tree = rust();
    assert!(tree.tree().is_none());
    assert!(tree.root().is_none());
    assert_eq!(tree.language(), Language::Rust);
}

#[test]
fn parsing_produces_a_root_spanning_the_whole_source() {
    let source = "fn main() {\n    let x = 1;\n}\n";
    let mut tree = rust();
    tree.parse(source).expect("rust source must parse");

    let root = tree.root().expect("a parsed tree has a root");
    assert_eq!(root.start_byte(), 0);
    assert_eq!(root.end_byte(), source.len());
    assert!(!root.has_error(), "this source is valid Rust");
}

#[test]
fn an_incremental_reparse_matches_a_parse_from_scratch() {
    // Six edits of different shapes — inside a line, adding a line, deleting a
    // line, at the very start, at the very end — each checked against the
    // parse the slow path would have produced.
    let mut source = String::from("fn main() {\n    let x = 1;\n}\n");
    let mut incremental = rust();
    incremental.parse(&source).expect("initial parse");

    let edits: &[(usize, usize, &str)] = &[
        (25, 25, "23"),                   // extend a literal, mid-line
        (29, 29, "    let y = 2;\n"),     // insert a whole line
        (12, 29, ""),                     // delete a whole line
        (0, 0, "// a leading comment\n"), // insert at the very start
        (0, 0, ""),                       // an edit that changes nothing
    ];

    for &(start, old_end, text) in edits {
        let removed = source[start..old_end].to_string();
        let old_end_point = byte_point(&source, old_end);
        let start_point = byte_point(&source, start);

        source.replace_range(start..old_end, text);
        let new_end = start + text.len();

        incremental.edit(&InputEdit {
            start_byte: start,
            old_end_byte: old_end,
            new_end_byte: new_end,
            start_position: start_point,
            old_end_position: old_end_point,
            new_end_position: byte_point(&source, new_end),
        });
        incremental.reparse(&source).expect("incremental reparse");

        let mut whole = rust();
        whole.parse(&source).expect("full parse");

        assert_eq!(
            shape(&incremental),
            shape(&whole),
            "after replacing {removed:?} with {text:?} the incremental tree diverged"
        );
    }
}

#[test]
fn reparsing_without_a_retained_tree_parses_the_whole_document() {
    let source = "fn main() {}";
    let mut incremental = rust();
    incremental.reparse(source).expect("reparse with no tree");

    let mut whole = rust();
    whole.parse(source).expect("full parse");

    assert_eq!(shape(&incremental), shape(&whole));
}

#[test]
fn editing_before_the_first_parse_is_a_no_op() {
    let mut tree = rust();
    tree.edit(&InputEdit {
        start_byte: 0,
        old_end_byte: 0,
        new_end_byte: 4,
        start_position: Point { row: 0, column: 0 },
        old_end_position: Point { row: 0, column: 0 },
        new_end_position: Point { row: 0, column: 4 },
    });

    assert!(
        tree.tree().is_none(),
        "an edit with nothing to edit must not conjure a tree"
    );
}

#[test]
fn editing_by_bytes_matches_a_full_parse() {
    let mut tree = rust();
    tree.parse("fn main() {}").expect("initial parse");

    let after = "fn main() { let x = 1; }";
    tree.edit_bytes(after, 12, 12, 24).expect("byte edit");

    let mut whole = rust();
    whole.parse(after).expect("full parse");

    assert_eq!(shape(&tree), shape(&whole));
}

#[test]
fn editing_by_bytes_before_the_first_parse_parses_the_whole_document() {
    let source = "fn main() {}";
    let mut tree = rust();
    tree.edit_bytes(source, 0, 0, source.len())
        .expect("byte edit with no tree");

    let mut whole = rust();
    whole.parse(source).expect("full parse");

    assert_eq!(shape(&tree), shape(&whole));
}

#[test]
fn changed_ranges_cover_the_edit_and_nothing_before_it() {
    let mut tree = rust();
    tree.parse("fn main() {\n    let x = 1;\n}\n")
        .expect("initial parse");
    let before = tree.tree().expect("a parsed tree").clone();

    let after = "fn main() {\n    let x = 12345;\n}\n";
    tree.edit_bytes(after, 24, 25, 29).expect("byte edit");

    let ranges = tree.changed_ranges(&before);
    assert!(!ranges.is_empty(), "changing a literal must change a range");
    assert!(
        ranges.iter().all(|range| range.start >= 12),
        "nothing before the edited line changed, but {ranges:?} says otherwise"
    );
}

#[test]
fn changed_ranges_are_empty_when_there_is_no_new_tree() {
    let mut parsed = rust();
    parsed.parse("fn main() {}").expect("initial parse");
    let old = parsed.tree().expect("a parsed tree").clone();

    let fresh = rust();
    assert!(
        fresh.changed_ranges(&old).is_empty(),
        "with nothing to compare against, nothing is known to have changed"
    );
}

#[test]
fn byte_point_counts_columns_in_bytes_not_characters() {
    // Three characters, seven bytes: a byte column is the only one tree-sitter
    // understands, and a character column would land the edit in the wrong place.
    let source = "let é = 1;\nlet ß = 2;";
    assert_eq!(byte_point(source, 0), Point { row: 0, column: 0 });
    assert_eq!(byte_point(source, 4), Point { row: 0, column: 4 });
    assert_eq!(
        byte_point(source, 6),
        Point { row: 0, column: 6 },
        "the two bytes of é must both count"
    );
    // é makes the first line eleven bytes wide but ten characters: the newline
    // is byte 11, and byte 12 begins the second line.
    assert_eq!(byte_point(source, 11), Point { row: 0, column: 11 });
    assert_eq!(byte_point(source, 12), Point { row: 1, column: 0 });
    assert_eq!(byte_point(source, 16), Point { row: 1, column: 4 });
    assert_eq!(
        byte_point(source, 18),
        Point { row: 1, column: 6 },
        "the two bytes of ß must both count"
    );
}

#[test]
fn byte_point_past_the_end_returns_the_end() {
    let source = "one\ntwo";
    assert_eq!(
        byte_point(source, source.len()),
        Point { row: 1, column: 3 }
    );
    assert_eq!(byte_point(source, 9_999), Point { row: 1, column: 3 });
}

#[test]
fn byte_point_of_an_empty_document_is_the_origin() {
    assert_eq!(byte_point("", 0), Point { row: 0, column: 0 });
    assert_eq!(byte_point("", 5), Point { row: 0, column: 0 });
}

#[test]
fn every_language_can_own_a_tree() {
    for &language in Language::all() {
        let mut tree = SyntaxTree::new(language)
            .unwrap_or_else(|error| panic!("{} must be parseable: {error}", language.id()));
        tree.parse("")
            .unwrap_or_else(|| panic!("{} must parse an empty document", language.id()));
    }
}
