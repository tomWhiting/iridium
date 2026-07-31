//! Tests for the vendored text objects.
//!
//! Assertions are on the *text* a region covers rather than on node kinds or
//! offsets, for the same reason as in [`crate::navigate`]: the text is what a
//! person sees selected, and it survives a grammar renaming its internals.

use std::ops::Range;

use super::{Direction, TextObject, Variant, find, jump, regions};
use crate::{Language, SyntaxTree};

/// A parsed document, kept alive so the tree stays valid.
struct Fixture {
    language: Language,
    source: String,
    tree: SyntaxTree,
}

impl Fixture {
    fn new(language: Language, source: &str) -> Self {
        let mut tree = SyntaxTree::new(language).expect("every language has a grammar");
        tree.parse(source).expect("the source must parse");
        Self {
            language,
            source: source.to_string(),
            tree,
        }
    }

    fn tree(&self) -> &tree_sitter::Tree {
        self.tree.tree().expect("a parsed tree exists")
    }

    /// The byte range of the first occurrence of `needle`.
    fn at(&self, needle: &str) -> Range<usize> {
        let start = self
            .source
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} does not occur in the fixture"));
        start..start + needle.len()
    }

    fn text(&self, range: &Range<usize>) -> &str {
        &self.source[range.clone()]
    }

    fn find(&self, range: &Range<usize>, object: TextObject, variant: Variant) -> Option<String> {
        find(
            self.language,
            self.tree(),
            &self.source,
            range,
            object,
            variant,
        )
        .expect("the vendored query compiles")
        .map(|found| self.text(&found).to_string())
    }

    fn jump(&self, from: usize, object: TextObject, direction: Direction) -> Option<String> {
        jump(
            self.language,
            self.tree(),
            &self.source,
            from,
            object,
            direction,
        )
        .expect("the vendored query compiles")
        .map(|found| self.text(&found).to_string())
    }

    fn count(&self, object: TextObject, variant: Variant) -> usize {
        regions(self.language, self.tree(), &self.source, object, variant)
            .expect("the vendored query compiles")
            .len()
    }
}

const RUST: &str = "\
fn alpha() {
    let a = 1;
}

/// A doc comment.
struct Holder {
    field: u32,
}

fn beta() {
    let b = 2;
}
";

const PYTHON: &str = "\
class Outer:
    def method(self):
        def nested():
            return 1
        return nested()
";

#[test]
fn selecting_around_a_function_takes_the_whole_thing() {
    let fixture = Fixture::new(Language::Rust, RUST);

    let found = fixture
        .find(
            &fixture.at("let a = 1;"),
            TextObject::Function,
            Variant::Around,
        )
        .expect("the caret is inside a function");

    assert!(found.starts_with("fn alpha()"), "got {found:?}");
    assert!(found.ends_with('}'), "got {found:?}");
}

#[test]
fn selecting_inside_a_function_leaves_the_signature_behind() {
    let fixture = Fixture::new(Language::Rust, RUST);

    let inside = fixture
        .find(
            &fixture.at("let a = 1;"),
            TextObject::Function,
            Variant::Inside,
        )
        .expect("the caret is inside a function");

    assert!(
        !inside.contains("fn alpha"),
        "the inside variant kept the signature: {inside:?}"
    );
    assert!(inside.contains("let a = 1;"), "got {inside:?}");
}

#[test]
fn a_caret_outside_every_function_selects_nothing() {
    let fixture = Fixture::new(Language::Rust, RUST);

    assert_eq!(
        fixture.find(
            &fixture.at("field: u32"),
            TextObject::Function,
            Variant::Around
        ),
        None,
        "a struct field is not inside a function, and saying so is not an error"
    );
}

#[test]
fn the_innermost_function_wins_before_the_one_holding_it() {
    let fixture = Fixture::new(Language::Python, PYTHON);

    let found = fixture
        .find(
            &fixture.at("return 1"),
            TextObject::Function,
            Variant::Around,
        )
        .expect("the caret is inside the nested function");

    assert!(
        found.starts_with("def nested"),
        "selection jumped past the nested function to {found:?}"
    );
}

#[test]
fn selecting_again_walks_out_of_the_nested_function() {
    // The same rule expansion follows. Without it the second press does nothing,
    // which reads as a broken key rather than as a finished one.
    let fixture = Fixture::new(Language::Python, PYTHON);

    let inner = fixture
        .find(
            &fixture.at("return 1"),
            TextObject::Function,
            Variant::Around,
        )
        .expect("the nested function");
    let inner_range = fixture.at(&inner);

    let outer = fixture
        .find(&inner_range, TextObject::Function, Variant::Around)
        .expect("the method holding it");

    assert!(
        outer.starts_with("def method"),
        "the second press stayed on {outer:?}"
    );
}

#[test]
fn jumping_forward_reaches_the_next_function_and_then_stops() {
    let fixture = Fixture::new(Language::Rust, RUST);

    let next = fixture
        .jump(
            fixture.at("let a = 1;").start,
            TextObject::Function,
            Direction::Forward,
        )
        .expect("there is a function after the first one");
    assert!(next.starts_with("fn beta"), "got {next:?}");

    assert_eq!(
        fixture.jump(
            fixture.at("fn beta").start,
            TextObject::Function,
            Direction::Forward
        ),
        None,
        "past the last function there is nowhere to go"
    );
}

#[test]
fn jumping_is_strict_so_a_held_key_advances() {
    // Landing on the region already under the caret would make the key stick,
    // so a region starting exactly at `from` is skipped. That has a consequence
    // worth pinning rather than discovering: jumping forward from byte zero of a
    // file whose first function starts at byte zero lands on the *second*
    // function. It is the right trade — a key that advances beats a key that
    // sometimes does nothing — but it is not what a reader would guess.
    let fixture = Fixture::new(Language::Rust, RUST);
    assert_eq!(fixture.at("fn alpha").start, 0, "the fixture starts on it");

    let from_zero = fixture
        .jump(0, TextObject::Function, Direction::Forward)
        .expect("there is a second function");
    assert!(from_zero.starts_with("fn beta"), "got {from_zero:?}");

    assert_eq!(
        fixture.jump(0, TextObject::Function, Direction::Backward),
        None,
        "nothing starts before the first byte"
    );
}

#[test]
fn jumping_backward_mirrors_jumping_forward() {
    let fixture = Fixture::new(Language::Rust, RUST);

    let back = fixture
        .jump(
            fixture.at("fn beta").start,
            TextObject::Function,
            Direction::Backward,
        )
        .expect("alpha is behind beta");

    assert!(back.starts_with("fn alpha"), "got {back:?}");
}

#[test]
fn a_comment_has_an_around_but_no_inside() {
    let fixture = Fixture::new(Language::Rust, RUST);

    assert_eq!(
        TextObject::Comment.capture(Variant::Inside),
        None,
        "no vendored query defines the inside of a comment"
    );
    assert_eq!(
        fixture.count(TextObject::Comment, Variant::Inside),
        0,
        "asking for it must be answerable, not a panic"
    );
    assert!(
        fixture.count(TextObject::Comment, Variant::Around) > 0,
        "the fixture has a doc comment"
    );
}

#[test]
fn a_markdown_section_is_a_class_which_is_what_makes_headings_navigable() {
    // Not a quirk to work around: `markdown` maps `@class` onto sections, so
    // jump-by-class is the heading navigator for prose. It is also the only
    // text object markdown has, which is why it is asserted rather than assumed.
    let fixture = Fixture::new(Language::Markdown, "# One\n\ntext\n\n# Two\n\nmore\n");

    assert_eq!(
        fixture.count(TextObject::Class, Variant::Around),
        2,
        "two headings, two sections"
    );

    let next = fixture
        .jump(
            fixture.at("text").start,
            TextObject::Class,
            Direction::Forward,
        )
        .expect("a second section follows the first");
    assert!(next.contains("Two"), "got {next:?}");

    let back = fixture
        .jump(
            // From the heading itself, not from inside the section: section Two
            // starts before its own body, so a backward jump from `more` finds
            // Two rather than One.
            fixture.at("# Two").start,
            TextObject::Class,
            Direction::Backward,
        )
        .expect("the first section is behind the second");
    assert!(back.contains("One"), "got {back:?}");

    assert_eq!(
        fixture.count(TextObject::Function, Variant::Around),
        0,
        "markdown has no function text object, and must say so rather than guess"
    );
}

#[test]
fn a_language_without_a_capture_answers_none_rather_than_failing() {
    // JSON ships only `@comment.around`. A verb bound to a key must return
    // nothing here, not an error the person cannot act on.
    let fixture = Fixture::new(Language::Json, "{\"a\": [1, 2]}");

    for object in [TextObject::Function, TextObject::Class] {
        for variant in [Variant::Inside, Variant::Around] {
            assert_eq!(
                fixture.find(&fixture.at("1"), object, variant),
                None,
                "{object:?}/{variant:?} must be absent for JSON"
            );
            assert_eq!(fixture.jump(0, object, Direction::Forward), None);
        }
    }
}

#[test]
fn every_language_answers_every_combination_without_failing() {
    // The guard that matters most: these are bound to keys, and a language whose
    // vendored query does not compile — or whose captures are named differently
    // than expected — must degrade to "nothing here", never to a panic or an
    // error surfaced at a keypress.
    for &language in Language::all() {
        let mut tree = SyntaxTree::new(language).expect("every language has a grammar");
        // Deliberately the same source for every language: most will not parse
        // it cleanly, which is the point. Navigation runs against broken trees
        // constantly.
        let source = "fn a() {}\nclass B {}\n// c\n";
        tree.parse(source).expect("parsing never fails outright");
        let Some(parsed) = tree.tree() else { continue };

        for object in [TextObject::Function, TextObject::Class, TextObject::Comment] {
            for variant in [Variant::Inside, Variant::Around] {
                regions(language, parsed, source, object, variant).unwrap_or_else(|error| {
                    panic!("{}/{object:?}/{variant:?} failed: {error}", language.id())
                });
            }
            for direction in [Direction::Forward, Direction::Backward] {
                jump(language, parsed, source, 4, object, direction).unwrap_or_else(|error| {
                    panic!("{}/{object:?}/{direction:?} failed: {error}", language.id())
                });
            }
        }
    }
}
