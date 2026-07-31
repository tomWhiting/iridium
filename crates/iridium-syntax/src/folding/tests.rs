//! Tests for fold-region detection.

use super::*;
use crate::SyntaxTree;

/// Parses `source` and returns its fold regions.
///
/// One owner for the tree, one set of rules for the folds — the same two steps
/// every real caller takes.
fn regions_for(language: Language, source: &str) -> Vec<FoldRegion> {
    let mut tree = SyntaxTree::new(language).expect("every language parses");
    let parsed = tree.parse(source).expect("a parse must produce a tree");
    FoldDetector::new(language).regions_in(parsed, source)
}

#[test]
fn fold_region_basics() {
    let region = FoldRegion::new(5, 10, FoldKind::Block);
    assert_eq!(region.line_count(), 6);
    assert_eq!(region.hidden_line_count(), 5);
    assert!(region.contains_line(5));
    assert!(region.contains_line(7));
    assert!(region.contains_line(10));
    assert!(!region.contains_line(4));
    assert!(!region.contains_line(11));
}

#[test]
fn fold_region_contains_region() {
    let outer = FoldRegion::new(0, 20, FoldKind::Block);
    let inner = FoldRegion::new(5, 15, FoldKind::Block);
    let overlap = FoldRegion::new(10, 25, FoldKind::Block);

    assert!(outer.contains_region(&inner));
    assert!(!inner.contains_region(&outer));
    assert!(!outer.contains_region(&overlap));
}

#[test]
fn fold_region_ordering() {
    let a = FoldRegion::new(0, 10, FoldKind::Block);
    let b = FoldRegion::new(0, 5, FoldKind::Block);
    let c = FoldRegion::new(5, 15, FoldKind::Block);

    let mut regions = [c.clone(), b.clone(), a.clone()];
    regions.sort();

    // Same start line: larger region first (a before b)
    // Different start line: earlier first (a/b before c)
    assert_eq!(regions[0], a);
    assert_eq!(regions[1], b);
    assert_eq!(regions[2], c);
}

#[test]
fn detect_rust_function() {
    let language = Language::Rust;
    let source = r#"fn main() {
    println!("Hello");
    println!("World");
}"#;

    let regions = regions_for(language, source);
    assert!(!regions.is_empty(), "Should detect function as foldable");

    // Should have at least the function body
    let has_function = regions
        .iter()
        .any(|r| r.start_line == 0 && r.kind == FoldKind::Block);
    assert!(has_function, "Should detect main function");
}

#[test]
fn detect_rust_nested_blocks() {
    let language = Language::Rust;
    let source = r#"fn main() {
    if true {
        println!("nested");
    }
}"#;

    let regions = regions_for(language, source);

    // Should detect both the function and the if block
    assert!(regions.len() >= 2, "Should detect nested blocks");
}

#[test]
fn detect_rust_imports() {
    let language = Language::Rust;
    let source = r"use std::io;
use std::fs;
use std::path::Path;

fn main() {}";

    let regions = regions_for(language, source);

    // Should detect the import group
    let has_imports = regions.iter().any(|r| r.kind == FoldKind::Import);
    assert!(has_imports, "Should detect import group");
}

#[test]
fn detect_python_function() {
    let language = Language::Python;
    let source = r#"def greet():
    print("Hello")
    print("World")"#;

    let regions = regions_for(language, source);
    assert!(!regions.is_empty(), "Should detect Python function");
}

#[test]
fn detect_typescript_class() {
    let language = Language::TypeScript;
    let source = r#"class Greeter {
    constructor() {
        console.log("init");
    }
    greet() {
        console.log("hello");
    }
}"#;

    let regions = regions_for(language, source);
    assert!(regions.len() >= 2, "Should detect class and methods");
}

#[test]
fn detect_go_function() {
    let language = Language::Go;
    let source = r#"func main() {
    fmt.Println("Hello")
}"#;

    let regions = regions_for(language, source);
    assert!(!regions.is_empty(), "Should detect Go function");
}

#[test]
fn detect_json_object() {
    let language = Language::Json;
    let source = r#"{
    "name": "test",
    "nested": {
        "value": 42
    }
}"#;

    let regions = regions_for(language, source);
    assert!(regions.len() >= 2, "Should detect JSON objects");
}

#[test]
fn folds_from_an_incrementally_parsed_tree_match_a_full_parse() {
    // Folds and highlights now come off one tree, so an incremental parse
    // that drifted would move fold markers as well as colours. Nothing else
    // here would catch that.
    let language = Language::Rust;
    let source = "fn main() {\n    println!(\"hello\");\n}";
    let mut tree = SyntaxTree::new(language).expect("rust parses");
    tree.parse(source).expect("initial parse");

    let after = "fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n}";
    let parsed = tree
        .edit_bytes(after, 34, 34, 56)
        .expect("incremental parse");

    let incremental = FoldDetector::new(language).regions_in(parsed, after);

    assert!(!incremental.is_empty(), "the function body is foldable");
    assert_eq!(
        incremental,
        regions_for(language, after),
        "the incremental parse folded differently from a full one"
    );
}

#[test]
fn single_line_not_foldable() {
    let language = Language::Rust;
    let source = "fn main() {}";

    let regions = regions_for(language, source);

    // Single-line function should not be foldable
    let has_single_line = regions
        .iter()
        .any(|r| r.start_line == r.end_line && r.kind == FoldKind::Block);
    assert!(
        !has_single_line,
        "Single-line blocks should not be foldable"
    );
}
