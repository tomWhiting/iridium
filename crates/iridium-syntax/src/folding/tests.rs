//! Tests for fold-region detection.

use super::*;
use crate::{Language, SyntaxTree};

/// Parses `source` and returns its fold regions.
///
/// One owner for the tree, one set of rules for the folds — the same two steps
/// every real caller takes.
fn regions_for(language: Language, source: &str) -> Vec<FoldRegion> {
    let mut tree = SyntaxTree::new(language).expect("every language parses");
    let parsed = tree.parse(source).expect("a parse must produce a tree");
    FoldDetector::new(language).regions_in(parsed)
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

    let incremental = FoldDetector::new(language).regions_in(parsed);

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

// ==========================================================================
// The incremental cache
//
// `FoldCache` exists to make a keystroke cost work proportional to the edit
// rather than to the document. That is only worth anything if the list it
// produces is the same list a full walk would have produced, so the tests below
// are almost entirely about equality with `FoldDetector::regions_in`, driven
// through real grammars and real reparses.
// ==========================================================================

use std::fmt::Write as _;

use tree_sitter::InputEdit;

use crate::byte_point;

/// Replaces `old_len` bytes at `start` with `insert`, returning the new text and
/// the edit that describes the change.
///
/// The old-end position is measured against the text *before* the splice and the
/// new-end against the text *after*, because those are two different documents
/// and tree-sitter is told so. Deriving both from the same string is the classic
/// way to mis-position every multi-line edit.
fn splice(source: &str, start: usize, old_len: usize, insert: &str) -> (String, InputEdit) {
    let old_end = start + old_len;
    let mut text = String::with_capacity(source.len() + insert.len());
    text.push_str(&source[..start]);
    text.push_str(insert);
    text.push_str(&source[old_end..]);
    let new_end = start + insert.len();

    let edit = InputEdit {
        start_byte: start,
        old_end_byte: old_end,
        new_end_byte: new_end,
        start_position: byte_point(source, start),
        old_end_position: byte_point(source, old_end),
        new_end_position: byte_point(&text, new_end),
    };
    (text, edit)
}

/// A tree, a cache and the text all three describe, kept in step.
struct Incremental {
    language: Language,
    tree: SyntaxTree,
    cache: FoldCache,
    text: String,
}

impl Incremental {
    fn new(language: Language, source: &str) -> Self {
        let mut tree = SyntaxTree::new(language).expect("every language parses");
        let parsed = tree.parse(source).expect("a parse must produce a tree");
        let mut cache = FoldCache::new(language);
        cache.rebuild(parsed);
        Self {
            language,
            tree,
            cache,
            text: source.to_owned(),
        }
    }

    /// Applies one edit exactly as the editor does, then checks the invariant.
    ///
    /// The invariant is equality with a full walk of the very same tree — not a
    /// similar tree, not a fresh parse. A fresh parse is checked too, one
    /// assertion further down, because the two claims fail for different
    /// reasons and a single message could not say which.
    ///
    /// Returns true if the update was genuinely incremental — that is, if it
    /// examined fewer nodes than the tree has. Without that the caller cannot
    /// tell an equality that means something from one where every edit quietly
    /// fell back to a full rebuild and the comparison compared a full walk with
    /// itself.
    fn edit(&mut self, start: usize, old_len: usize, insert: &str) -> bool {
        let (text, edit) = splice(&self.text, start, old_len, insert);

        // Shift the retained tree first, and only then take the copy that
        // `changed_ranges` will be compared against. Comparing an *unedited*
        // tree to the new one reports the whole document as changed — which is
        // safe, and therefore silent, and would have left this test asserting
        // that a full recompute equals a full recompute.
        self.tree.edit(&edit);
        let previous = self
            .tree
            .tree()
            .cloned()
            .expect("the fixture is parsed before it is edited");
        self.tree.reparse(&text);
        let changed = self.tree.changed_ranges(&previous);
        let parsed = self.tree.tree().expect("a reparse must produce a tree");

        let before = self.cache.nodes_visited();
        self.cache.update(parsed, &edit, &changed);
        let examined = self.cache.nodes_visited() - before;
        let total = u64::try_from(parsed.root_node().descendant_count()).unwrap_or(u64::MAX);
        self.text = text;

        let full = FoldDetector::new(self.language).regions_in(parsed);
        assert_eq!(
            self.cache.regions(),
            full.as_slice(),
            "{:?}: the cache disagreed with a full walk of the same tree after \
             replacing {old_len} bytes at {start} with {insert:?}\n--- text ---\n{}",
            self.language,
            self.text
        );
        assert_eq!(
            self.cache.regions(),
            regions_for(self.language, &self.text).as_slice(),
            "{:?}: the cache disagreed with a fresh parse after replacing \
             {old_len} bytes at {start} with {insert:?}",
            self.language
        );

        examined < total
    }
}

/// A small deterministic generator, so the edit sequence is varied but the
/// failure is always the same failure.
struct Rng(u64);

impl Rng {
    const fn next(&mut self) -> u64 {
        // A 64-bit xorshift. Any full-period generator would do; what matters is
        // that it is seeded, not that it is good.
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            return 0;
        }
        usize::try_from(self.next() % bound as u64).unwrap_or(0)
    }
}

/// Moves `byte` to the nearest character boundary at or below it.
fn floor_boundary(text: &str, byte: usize) -> usize {
    let mut byte = byte.min(text.len());
    while byte > 0 && !text.is_char_boundary(byte) {
        byte -= 1;
    }
    byte
}

/// Rust source with enough shapes to produce folds of every kind this detector
/// recognises: imports, block comments, nested blocks, arrays, closures.
fn rust_fixture() -> String {
    String::from(
        r#"use std::collections::HashMap;
use std::fmt;
use std::io;

/**
 * A block comment that spans
 * several lines, so it folds.
 */
pub struct Record {
    pub id: u64,
    pub tags: Vec<String>,
}

impl Record {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            tags: vec![
                String::from("one"),
                String::from("two"),
            ],
        }
    }

    pub fn describe(&self) -> String {
        let mut out = String::new();
        for tag in &self.tags {
            if tag.is_empty() {
                continue;
            }
            out.push_str(tag);
        }
        match self.id {
            0 => String::from("zero"),
            other => format!("{other}"),
        }
    }
}
"#,
    )
}

/// TypeScript, for a grammar whose fold kinds nest differently from Rust's.
fn typescript_fixture() -> String {
    String::from(
        r#"import { readFile } from "fs";
import { join } from "path";

/* a block
   comment */
export class Loader {
    private cache = new Map<string, string>();

    async load(name: string): Promise<string> {
        if (this.cache.has(name)) {
            return this.cache.get(name)!;
        }
        try {
            const text = await readFile(join("root", name), "utf8");
            this.cache.set(name, text);
            return text;
        } catch (error) {
            switch (typeof error) {
                case "string":
                    return error;
                default:
                    return "";
            }
        }
    }
}

const table = [
    { id: 1, name: "one" },
    { id: 2, name: "two" },
];
"#,
    )
}

/// JSON in the shape that first exposed the cost: one record per line, so the
/// document is wide rather than deep.
fn json_fixture() -> String {
    let mut out = String::from("[\n");
    for i in 0..40 {
        // Writing into a `String` is infallible; the result is discarded rather
        // than unwrapped so no failure path is invented for a case that has none.
        let _ = writeln!(
            out,
            "  {{ \"id\": {i}, \"name\": \"record-{i}\", \"nested\": {{\n    \"deep\": true\n  }} }},"
        );
    }
    out.push_str("  null\n]\n");
    out
}

/// Python, whose folds are decided by indentation rather than braces.
fn python_fixture() -> String {
    String::from(
        r"import os
import sys
from typing import List

class Loader:
    def __init__(self, root: str):
        self.root = root
        self.cache = {}

    def load(self, name: str) -> str:
        if name in self.cache:
            return self.cache[name]
        with open(os.path.join(self.root, name)) as handle:
            text = handle.read()
        for line in text.splitlines():
            if not line:
                continue
            print(line)
        return text
",
    )
}

/// Every language, every kind of edit, checked against a full walk each time.
///
/// This is the test the incremental cache exists to satisfy. It drives 200 edits
/// per language — inserts, deletions, replacements, newlines, and the structural
/// characters that make a grammar reinterpret whole spans — and after every
/// single one asserts that the retained-and-patched list is byte-for-byte the
/// list a full walk of the same tree produces.
#[test]
fn cache_matches_a_full_recompute_across_long_edit_sequences() {
    // Insertions that a real editor produces, including the ones that change how
    // a grammar reads everything after them.
    const INSERTS: &[&str] = &[
        "x",
        " ",
        "\n",
        "\n\n",
        "}",
        "{",
        "\"",
        ")",
        "(",
        ",",
        "//",
        "/*",
        "*/",
        "fn f() {\n    g();\n}\n",
        "        ",
        ";",
        "]",
        "[",
    ];

    let fixtures = [
        (Language::Rust, rust_fixture()),
        (Language::TypeScript, typescript_fixture()),
        (Language::Json, json_fixture()),
        (Language::Python, python_fixture()),
    ];

    for (language, source) in fixtures {
        let mut rng = Rng(0x5DEE_CE66_D1CE_4B9D);
        let mut state = Incremental::new(language, &source);
        let mut incremental_steps = 0_u32;

        for step in 0..200 {
            let len = state.text.len();
            let start = floor_boundary(&state.text, rng.below(len + 1));

            // Alternate between growing and shrinking so the document neither
            // runs away nor collapses, and so deletions are exercised as hard as
            // insertions.
            let deleting = step % 3 == 2 && start < len;
            let old_len = if deleting {
                let end = floor_boundary(&state.text, start + 1 + rng.below(12));
                end.saturating_sub(start)
            } else {
                0
            };
            let insert = if deleting {
                ""
            } else {
                INSERTS[rng.below(INSERTS.len())]
            };

            if state.edit(start, old_len, insert) {
                incremental_steps += 1;
            }
        }

        // Many of these edits are deliberately destructive — a lone `"` or `{`
        // makes a grammar reinterpret everything after it, and re-walking is
        // then the right answer — so this is not a demand that every step be
        // incremental. It is a demand that the incremental path be what the
        // equality above is mostly checking.
        assert!(
            incremental_steps >= 100,
            "{language:?}: only {incremental_steps} of 200 edits took the \
             incremental path, so the equality above mostly compared a full \
             walk with itself"
        );
    }
}

/// The point of the whole exercise: an edit must not look at the document.
///
/// The fixture is deliberately wide — a JSON array whose root has thousands of
/// direct children — because that is the shape the full walk was slowest on.
/// Two sizes are compared rather than one bound asserted, so the test says "does
/// not grow with the document" rather than "is currently under some number I
/// picked".
///
/// The edit lands inside a string literal on purpose. Typing a character that
/// makes the document ill-formed genuinely restructures everything after it,
/// tree-sitter says so, and re-walking the lot is then the correct answer rather
/// than a regression — so measuring the claim requires an edit that does not do
/// that.
#[test]
fn an_edit_examines_a_constant_number_of_nodes() {
    fn nodes_for_one_edit(records: usize) -> u64 {
        let mut source = String::from("[\n");
        for i in 0..records {
            let _ = writeln!(source, "  {{ \"id\": {i}, \"name\": \"record-{i}\" }},");
        }
        source.push_str("  null\n]\n");

        // Inside the first record's name, which keeps the document well formed.
        let at = source
            .find("record-0")
            .expect("the generated fixture contains its own first record")
            + 3;

        let mut state = Incremental::new(Language::Json, &source);
        // One edit first, so the count excludes anything only a first edit pays.
        state.edit(at, 0, "a");
        let before = state.cache.nodes_visited();
        state.edit(at, 0, "b");
        state.cache.nodes_visited() - before
    }

    let small = nodes_for_one_edit(500);
    let large = nodes_for_one_edit(20_000);

    assert_eq!(
        small, large,
        "fold detection after an edit examined {small} nodes on a 500-record \
         document and {large} on a 20,000-record one; it must not depend on the \
         document at all"
    );
    assert!(
        large < 64,
        "an edit examined {large} nodes, which is too many to be explained by \
         the edit site alone"
    );
}

/// A full rebuild must also stay off the per-node cursor allocation that made it
/// cost half a second, but it is still a walk — so the claim here is only that
/// it visits every node once.
#[test]
fn a_full_rebuild_visits_every_node_once() {
    let source = rust_fixture();
    let mut tree = SyntaxTree::new(Language::Rust).expect("rust parses");
    let parsed = tree.parse(&source).expect("the fixture parses");
    let expected = u64::try_from(parsed.root_node().descendant_count()).unwrap_or(u64::MAX);

    let mut cache = FoldCache::new(Language::Rust);
    cache.rebuild(parsed);

    assert_eq!(
        cache.nodes_visited(),
        expected,
        "a full rebuild must visit each of the tree's nodes exactly once"
    );
}

/// An update against a cache that has never seen a tree must rebuild.
///
/// Otherwise it would retain nothing, re-walk only the edit site, and publish
/// that fragment as the document's complete fold list.
#[test]
fn updating_a_cache_that_never_saw_a_tree_rebuilds_it() {
    let source = rust_fixture();
    let (text, edit) = splice(&source, 0, 0, "x");

    let mut tree = SyntaxTree::new(Language::Rust).expect("rust parses");
    let parsed = tree.parse(&text).expect("the fixture parses");

    let mut cache = FoldCache::new(Language::Rust);
    assert!(!cache.is_primed(), "a new cache has seen no tree");
    cache.update(parsed, &edit, &[]);

    assert!(cache.is_primed(), "the update must have rebuilt");
    assert_eq!(
        cache.regions(),
        FoldDetector::new(Language::Rust)
            .regions_in(parsed)
            .as_slice(),
        "an update with nothing retained must produce the whole document's folds"
    );
}

/// A same-length replacement can leave the two trees structurally identical, so
/// `changed_ranges` reports nothing at all. The regions covering the edit were
/// still discarded, and something has to put them back.
#[test]
fn an_edit_that_changes_no_structure_keeps_its_regions() {
    let source = "fn main() {\n    let alpha = 1;\n    println!(\"{alpha}\");\n}\n";
    let mut state = Incremental::new(Language::Rust, source);
    let before = state.cache.regions().to_vec();
    assert!(!before.is_empty(), "the fixture folds");

    // `alpha` -> `omega`: same length, same node kinds, same extents.
    let at = source
        .find("alpha")
        .expect("the fixture contains the identifier");
    state.edit(at, 5, "omega");

    assert_eq!(
        state.cache.regions(),
        before.as_slice(),
        "a rename that changes no structure must not change any fold"
    );
}
