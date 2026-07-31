//! Tests for the structural walks.
//!
//! Almost every assertion here is written against the *text* a walk lands on
//! rather than the node kind it returns. That is on purpose: the text is what a
//! person sees selected, and it survives a grammar renaming its internals,
//! which node kinds do not.

use std::ops::Range;

use tree_sitter::Node;

use super::{
    children, expand, first_child, last_child, next_sibling, node_at, node_ending_after,
    node_starting_before, previous_sibling, shrink, siblings,
};
use crate::{Language, SyntaxTree};

/// A parsed document, kept alive so nodes borrowed from it stay valid.
struct Fixture {
    source: String,
    tree: SyntaxTree,
}

impl Fixture {
    fn new(language: Language, source: &str) -> Self {
        let mut tree = SyntaxTree::new(language).expect("every language has a grammar");
        tree.parse(source).expect("the source must parse");
        Self {
            source: source.to_string(),
            tree,
        }
    }

    fn root(&self) -> Node<'_> {
        self.tree.root().expect("a parsed tree has a root")
    }

    /// The byte range of the first occurrence of `needle`.
    fn at(&self, needle: &str) -> Range<usize> {
        let start = self
            .source
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} does not occur in the fixture"));
        start..start + needle.len()
    }

    /// An empty range immediately before the first occurrence of `needle`.
    fn before(&self, needle: &str) -> Range<usize> {
        let start = self.at(needle).start;
        start..start
    }

    /// An empty range immediately after the first occurrence of `needle`.
    fn after(&self, needle: &str) -> Range<usize> {
        let end = self.at(needle).end;
        end..end
    }

    fn text(&self, node: Node<'_>) -> &str {
        &self.source[node.byte_range()]
    }

    /// The texts of every step of expanding from `range` up to the root.
    fn expansions(&self, range: &Range<usize>) -> Vec<&str> {
        let mut steps = Vec::new();
        let mut current = range.clone();
        while let Some(node) = expand(self.root(), &current) {
            steps.push(self.text(node));
            current = node.byte_range();
            assert!(
                steps.len() < 64,
                "expansion did not reach the root in 64 steps; it is not converging"
            );
        }
        steps
    }
}

const RUST_TWO_FUNCTIONS: &str = "fn one() {\n    let a = 1;\n    let b = 2;\n}\n\nfn two() {}\n";
const JSON_NESTED: &str = "{\"items\": [{\"name\": \"first\"}, {\"name\": \"second\"}]}";

#[test]
fn a_caret_inside_a_token_selects_that_token() {
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);
    let inside = fixture.at("let a").start + 5; // between the `a` and nothing else

    let node = node_at(fixture.root(), &(inside..inside)).expect("a caret is always somewhere");
    assert_eq!(fixture.text(node), "a");
}

#[test]
fn a_caret_after_a_token_reads_the_same_as_a_caret_before_it() {
    // Tree-sitter resolves an empty range at a token's end to the token's
    // *parent*, so without the touching rule this pair disagrees: `|a` gives
    // `a` and `a|` gives the whole `let a = 1;`. A caret sitting just after the
    // word that was typed is where it spends most of its life.
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);

    let before = node_at(fixture.root(), &fixture.before("a = 1")).expect("before the token");
    let after = node_at(fixture.root(), &fixture.after("let a")).expect("after the token");

    assert_eq!(fixture.text(before), "a");
    assert_eq!(
        fixture.text(after),
        "a",
        "a caret after `a` resolved to {:?}, so expand would skip the token \
         a person had just finished typing",
        fixture.text(after)
    );
}

#[test]
fn expansion_skips_wrappers_that_add_nothing() {
    // In Python `assignment`, `expression_statement` and `block` all span
    // exactly `x = 1`. Returning the immediate parent would be three keypresses
    // that visibly do nothing.
    let fixture = Fixture::new(Language::Python, "def f():\n    x = 1\n");

    let first = expand(fixture.root(), &fixture.at("x = 1")).expect("something must be larger");

    assert_eq!(
        fixture.text(first),
        "def f():\n    x = 1",
        "expansion stopped on a wrapper with the same extent as its child"
    );
}

#[test]
fn expansion_always_grows_the_range() {
    // The property a key binding rests on: a press either widens the selection
    // or reports that there is nothing left, and never sits still.
    for (language, source, needle) in [
        (Language::Rust, RUST_TWO_FUNCTIONS, "1"),
        (Language::Json, JSON_NESTED, "first"),
        (Language::Python, "def f():\n    x = 1\n", "x"),
        (Language::JavaScript, "const a = [1, 2, 3];\n", "2"),
    ] {
        let fixture = Fixture::new(language, source);
        let mut current = fixture.at(needle);
        let mut steps = 0;

        while let Some(node) = expand(fixture.root(), &current) {
            let next = node.byte_range();
            assert!(
                next.start <= current.start && current.end <= next.end && next != current,
                "{} expanded {current:?} to {next:?}, which is not strictly larger",
                language.id()
            );
            assert!(
                node.is_named(),
                "{} expanded onto the anonymous node {:?}",
                language.id(),
                node.kind()
            );
            current = next;
            steps += 1;
            assert!(steps < 64, "{} never stopped expanding", language.id());
        }

        assert!(steps > 0, "{} could not expand at all", language.id());
    }
}

#[test]
fn expansion_walks_a_json_value_out_to_the_whole_document() {
    // The daily driver, on the shape it is used against: pull a value out of a
    // nested object one enclosing level at a time.
    let fixture = Fixture::new(Language::Json, JSON_NESTED);

    assert_eq!(
        fixture.expansions(&fixture.before("first")),
        vec![
            "first",
            "\"first\"",
            "\"name\": \"first\"",
            "{\"name\": \"first\"}",
            "[{\"name\": \"first\"}, {\"name\": \"second\"}]",
            "\"items\": [{\"name\": \"first\"}, {\"name\": \"second\"}]",
            JSON_NESTED,
        ]
    );
}

#[test]
fn expansion_stops_at_the_outermost_node() {
    let fixture = Fixture::new(Language::Json, "[1]");
    let whole = 0..fixture.source.len();

    assert!(
        expand(fixture.root(), &whole).is_none(),
        "there is nothing larger than the document, and saying so is not an error"
    );
}

#[test]
fn an_empty_document_has_nothing_to_expand() {
    let fixture = Fixture::new(Language::Rust, "");

    assert!(expand(fixture.root(), &(0..0)).is_none());
    assert!(shrink(fixture.root(), &(0..0)).is_none());
}

#[test]
fn shrinking_undoes_expanding() {
    let fixture = Fixture::new(Language::Json, JSON_NESTED);
    let start = fixture.at("\"first\"");

    let wider = expand(fixture.root(), &start).expect("the pair encloses the string");
    assert_eq!(fixture.text(wider), "\"name\": \"first\"");

    let back = shrink(fixture.root(), &wider.byte_range()).expect("the pair has parts");
    assert_eq!(
        fixture.text(back),
        "\"name\"",
        "shrinking descends towards the start of the range, which is the key"
    );
}

#[test]
fn shrinking_descends_towards_the_start_of_the_range() {
    // A selection that does not begin at its enclosing node's first child: the
    // descent has to follow the range's start, because falling back to the
    // first child lands outside the range entirely and then finds nothing at
    // all.
    let fixture = Fixture::new(Language::Json, "[1, 22, 333]");

    let inner = shrink(fixture.root(), &fixture.at("22, 333")).expect("the array has elements");

    assert_eq!(fixture.text(inner), "22");
}

#[test]
fn shrinking_a_caret_finds_nothing() {
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);

    assert!(
        shrink(fixture.root(), &fixture.before("let a")).is_none(),
        "no node fits inside an empty range, and inventing one would move the caret"
    );
}

#[test]
fn shrinking_reaches_a_leaf_and_stops() {
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);
    let mut current = fixture.at("let a = 1;");
    let mut steps = 0;

    while let Some(node) = shrink(fixture.root(), &current) {
        let next = node.byte_range();
        assert!(
            current.start <= next.start && next.end <= current.end && next != current,
            "shrink returned {next:?}, which is not strictly inside {current:?}"
        );
        current = next;
        steps += 1;
        assert!(steps < 64, "shrink never bottomed out");
    }

    assert!(steps > 0, "shrink could not descend at all");
}

#[test]
fn the_sibling_walk_climbs_when_there_is_nothing_beside_it() {
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);
    let last_statement =
        node_at(fixture.root(), &fixture.at("let b = 2;")).expect("the second statement is a node");
    assert_eq!(fixture.text(last_statement), "let b = 2;");

    let next = next_sibling(last_statement).expect("the next function follows");
    assert_eq!(
        fixture.text(next),
        "fn two() {}",
        "the walk stopped at the end of the block instead of climbing out of it"
    );
}

#[test]
fn the_sibling_walk_moves_within_a_block_before_climbing() {
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);
    let first = node_at(fixture.root(), &fixture.at("let a = 1;")).expect("a node");

    let next = next_sibling(first).expect("a second statement follows");
    assert_eq!(fixture.text(next), "let b = 2;");

    let back = previous_sibling(next).expect("the first statement precedes it");
    assert_eq!(fixture.text(back), "let a = 1;");
}

#[test]
fn the_backwards_walk_climbs_on_the_same_terms() {
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);
    let second = node_at(fixture.root(), &fixture.at("fn two() {}")).expect("a node");

    let back = previous_sibling(second).expect("the first function precedes it");
    assert_eq!(
        fixture.text(back),
        "fn one() {\n    let a = 1;\n    let b = 2;\n}"
    );
}

#[test]
fn a_walk_off_either_end_of_the_tree_stops() {
    let fixture = Fixture::new(Language::Json, "[1]");
    let root = fixture.root();

    assert!(next_sibling(root).is_none());
    assert!(previous_sibling(root).is_none());
}

#[test]
fn children_are_the_named_ones_in_source_order() {
    let fixture = Fixture::new(Language::Json, "[1, 22, 333]");
    let array = node_at(fixture.root(), &fixture.at("[1, 22, 333]")).expect("the array");

    let listed: Vec<&str> = children(array)
        .into_iter()
        .map(|node| fixture.text(node))
        .collect();

    assert_eq!(
        listed,
        vec!["1", "22", "333"],
        "the commas and brackets holding the elements apart are not elements"
    );
}

#[test]
fn siblings_include_the_node_they_start_from() {
    let fixture = Fixture::new(Language::Json, "[1, 22, 333]");
    let middle = node_at(fixture.root(), &fixture.at("22")).expect("the second element");

    let listed: Vec<&str> = siblings(middle)
        .into_iter()
        .map(|node| fixture.text(node))
        .collect();

    assert_eq!(
        listed,
        vec!["1", "22", "333"],
        "a cursor-on-every-sibling command that drops the cursor it started \
         from would lose the caret the person was using"
    );
}

#[test]
fn a_node_without_a_parent_is_its_own_only_sibling() {
    let fixture = Fixture::new(Language::Json, "[1]");
    let listed = siblings(fixture.root());

    assert_eq!(listed.len(), 1);
    assert_eq!(fixture.text(listed[0]), "[1]");
}

#[test]
fn first_and_last_child_skip_the_punctuation_around_them() {
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);
    let block = node_at(
        fixture.root(),
        &fixture.at("{\n    let a = 1;\n    let b = 2;\n}"),
    )
    .expect("the function body");

    assert_eq!(
        fixture.text(first_child(block).expect("a first statement")),
        "let a = 1;"
    );
    assert_eq!(
        fixture.text(last_child(block).expect("a last statement")),
        "let b = 2;"
    );
}

#[test]
fn a_leaf_has_no_children() {
    let fixture = Fixture::new(Language::Json, "[1]");
    let number = node_at(fixture.root(), &fixture.at("1")).expect("the element");

    assert!(first_child(number).is_none());
    assert!(last_child(number).is_none());
    assert!(children(number).is_empty());
}

#[test]
fn a_range_reaching_past_the_tree_is_clamped_rather_than_refused() {
    // The tree can lag the document by an edit. Refusing to navigate then would
    // take the feature away at exactly the moment it is being used.
    let fixture = Fixture::new(Language::Json, "[1]");
    let past_the_end = 1..9_999;

    let node = node_at(fixture.root(), &past_the_end).expect("clamping leaves a real range");
    assert!(fixture.source.get(node.byte_range()).is_some());

    // Expansion is where an unclamped range does real damage: nothing in the
    // tree can contain a range that runs past its end, so every `ast.*` key
    // would go dead until the parse caught up.
    let wider = expand(fixture.root(), &past_the_end)
        .expect("a range beyond the tree still has something to expand to");
    assert_eq!(fixture.text(wider), "[1]");
}

#[test]
fn a_reversed_range_reads_the_same_as_a_forward_one() {
    let fixture = Fixture::new(Language::Json, JSON_NESTED);
    let forward = fixture.at("\"first\"");
    let reversed = forward.end..forward.start;

    assert_eq!(
        expand(fixture.root(), &reversed).map(|node| node.byte_range()),
        expand(fixture.root(), &forward).map(|node| node.byte_range()),
        "a selection dragged backwards describes the same text as one dragged \
         forwards, and must navigate the same"
    );
}

#[test]
fn navigation_still_works_inside_a_broken_document() {
    // Source under active editing is incomplete more often than not.
    let fixture = Fixture::new(Language::Rust, "fn f() {\n    let value = ;\n");

    let node = node_at(fixture.root(), &fixture.at("value")).expect("a caret is still somewhere");
    assert_eq!(fixture.text(node), "value");
    assert!(
        expand(fixture.root(), &fixture.at("value")).is_some(),
        "an unparseable tail must not make the rest of the file unnavigable"
    );
}

#[test]
fn a_caret_inside_a_token_finds_that_token_in_both_directions() {
    let fixture = Fixture::new(Language::Json, JSON_NESTED);
    let word = fixture.at("first");
    let middle = word.start + 2..word.start + 2;

    let start = node_starting_before(fixture.root(), &middle).expect("something begins earlier");
    let end = node_ending_after(fixture.root(), &middle).expect("something ends later");

    assert_eq!(start.start_byte(), word.start);
    assert_eq!(end.end_byte(), word.end);
}

#[test]
fn a_caret_already_on_a_boundary_moves_out_rather_than_standing_still() {
    // The whole point of "strictly before". Without it the second press of a
    // jump-to-node-start key does nothing, and a key that does nothing on every
    // second press reads as broken rather than as finished.
    let fixture = Fixture::new(Language::Json, JSON_NESTED);
    let root = fixture.root();

    let mut caret = fixture.at("first").start;
    let mut starts = Vec::new();
    while let Some(node) = node_starting_before(root, &(caret..caret)) {
        assert!(
            node.start_byte() < caret,
            "the walk must strictly decrease or it will not terminate"
        );
        caret = node.start_byte();
        starts.push(caret);
        assert!(starts.len() < 64, "the walk is not converging on the root");
    }

    assert_eq!(
        starts,
        vec![
            // Not `first` itself: the caret already sits on its start, which is
            // exactly the case this rule exists for. The opening quote one byte
            // earlier is the first thing that genuinely begins before the caret.
            fixture.at("\"first\"").start,
            fixture.at("\"name\": \"first\"").start,
            fixture.at("{\"name\": \"first\"}").start,
            fixture.at("[").start,
            fixture.at("\"items\"").start,
            0,
        ],
        "each press should leave the caret one structural level further out"
    );
}

#[test]
fn the_end_walk_climbs_the_same_ladder_the_other_way() {
    let fixture = Fixture::new(Language::Json, JSON_NESTED);
    let root = fixture.root();

    let mut caret = fixture.at("first").end;
    let mut ends = Vec::new();
    while let Some(node) = node_ending_after(root, &(caret..caret)) {
        assert!(
            node.end_byte() > caret,
            "the walk must strictly increase or it will not terminate"
        );
        caret = node.end_byte();
        ends.push(caret);
        assert!(ends.len() < 64, "the walk is not converging on the root");
    }

    assert_eq!(
        ends.last().copied(),
        Some(JSON_NESTED.len()),
        "walking outward from any point must reach the end of the document"
    );
    assert!(
        ends.windows(2).all(|pair| pair[0] < pair[1]),
        "every step must be further right than the last"
    );
}

#[test]
fn the_edges_of_the_document_have_nowhere_further_to_go() {
    let fixture = Fixture::new(Language::Json, JSON_NESTED);
    let end = JSON_NESTED.len();

    assert!(
        node_starting_before(fixture.root(), &(0..0)).is_none(),
        "nothing begins before the first byte"
    );
    assert!(
        node_ending_after(fixture.root(), &(end..end)).is_none(),
        "nothing ends after the last byte"
    );
}

#[test]
fn the_boundary_walks_step_out_of_a_whole_selection() {
    // A selection that already covers a node exactly: its own start is not
    // strictly before itself, so the answer has to come from the parent.
    let fixture = Fixture::new(Language::Rust, RUST_TWO_FUNCTIONS);
    let statement = fixture.at("let a = 1;");

    let start = node_starting_before(fixture.root(), &statement)
        .expect("a statement sits inside something");
    assert!(
        start.start_byte() < statement.start,
        "a selection on a node boundary must move out, not report where it already is"
    );
    assert!(
        fixture.text(start).starts_with('{'),
        "the enclosing block is the next thing that begins earlier, but the walk \
         landed on {:?}",
        fixture.text(start)
    );
}
