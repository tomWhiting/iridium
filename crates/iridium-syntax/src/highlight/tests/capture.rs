//! Capture-name mapping, and the two unstyled-list ratchets.

use super::spans_for;
use crate::Language;
use crate::highlight::HighlightType;

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

/// Capture names that deliberately produce no span.
///
/// A capture beginning with `_` is a predicate operand by tree-sitter
/// convention — `(#eq? @_isinstance "isinstance")` names a node so a predicate
/// can test it, and styling it was never intended. `none` is the same idea
/// spelled differently: Rust's query uses it inside `#match?`, and the name is
/// also the upstream convention for *cancelling* a highlight an earlier pattern
/// applied. `text` is markdown prose, which should render in the plain
/// foreground, and `text.jsx` is the same thing in another grammar — the prose
/// between JSX elements, captured by `(jsx_text) @text.jsx`.
///
/// `nested` is the odd one, and it earns its place for a different reason. The
/// pattern at `typescript/highlights.scm:70-99` hijacks `statement_block` to
/// colour the pseudo-TypeScript snippets an LSP returns. Inside a
/// `labeled_statement` it alternates on the body, and `(statement_block)
/// @nested` is the **recursion arm** — it exists so the alternation succeeds
/// when the body is another block, letting the outer `label:` capture fire.
/// It must never carry a colour, because it matches a whole brace-delimited
/// region: `spans_with` emits the captured node's full byte range, so mapping
/// it would paint one span over everything inside those braces. Its author
/// simply did not use the `_` convention.
///
/// Verified individually against the vendored queries rather than assumed.
const DELIBERATELY_UNSTYLED: &[&str] = &[
    "_isinstance",
    "_issubclass",
    "none",
    "text",
    "text.jsx",
    "nested",
];

// ⭐ `KNOWN_UNSTYLED_GAP` STOOD HERE AND IS GONE, EMPTIED RATHER THAN KEPT.
//
// It listed six captures — markdown headings, link text and link URIs, and all
// three CSS selector forms — that reached the screen in the plain foreground.
// They are mapped now (#70), so the list has no members, and an exception list
// with nothing in it is a rot site: it reads as "these are still broken" to
// everyone who finds it, and the next person to hit an unmapped capture has an
// obvious place to hide it.
//
// ⚠️ Its doc comment had already gone stale in the way exception lists do — it
// still claimed JSX tags render unstyled, which stopped being true when #72
// mapped `tag.jsx` to `Tag`. That is the argument for deleting rather than
// emptying: a list nobody has to maintain is a list nobody notices is wrong.
//
// The ratchet it provided is not lost. `every_vendored_capture_maps_to_a_highlight_type`
// below now has no exemption to consult, so it fails on *any* unmapped capture
// rather than on any capture outside a hand-kept list — which is strictly
// stronger.

#[test]
fn every_vendored_capture_maps_to_a_highlight_type() {
    // The bug this exists to catch is silent: an unmapped capture produces no
    // span, so the text renders in the plain foreground and nothing anywhere
    // reports a problem. It is invisible unless you know what colour the token
    // was supposed to be.
    //
    // Checked against the compiled queries rather than by scanning the `.scm`
    // text, so predicate arguments and comments cannot be mistaken for
    // captures.
    let mut unmapped: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();

    for &language in Language::all() {
        let Some(query) = crate::query::compiled(language, crate::query::QueryKind::Highlights)
            .expect("a vendored highlights.scm must compile")
        else {
            continue;
        };
        for name in query.capture_names() {
            if HighlightType::from_capture_name(name).is_none()
                && !DELIBERATELY_UNSTYLED.contains(name)
            {
                unmapped.entry(name).or_default().push(language.id());
            }
        }
    }

    assert!(
        unmapped.is_empty(),
        "these capture names produce no highlight at all, so the tokens they \
         match render unstyled:\n{}",
        unmapped
            .iter()
            .map(|(name, languages)| format!("  @{name} — used by {}", languages.join(", ")))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// The three CSS selector categories #70 ruled, pinned to the ruling rather
/// than to a comment.
///
/// `every_vendored_capture_maps_to_a_highlight_type` only insists these map to
/// *something* — it would stay green if `selector.pseudo` silently became
/// `Comment` and every `:hover` started rendering grey and recessive. This says
/// which, so a later refactor that collapses arms has to disagree with the
/// decision out loud instead of by accident.
///
/// ⚠️ `selector.class` and `selector.id` differ deliberately. Unifying them
/// reads as tidier and hides the specificity distinction a stylesheet author is
/// actively reasoning about, so if a future change makes these two equal, this
/// test is the thing that should stop it.
///
/// ⭐ **#70 ruled six, and this pins three.** The other three —
/// `title.markup`, `link_text.markup`, `link_uri.markup` on `Keyword`,
/// `Function` and `String` — were superseded on 12 Aug 2026, out loud, exactly
/// as this test was built to force. They were *category* rulings that read as
/// *colour* rulings, which was harmless while a capture could carry nothing but
/// a colour and became a defect the moment it could also carry weight. The
/// colours those three render survived the change untouched, and
/// `the_markup_slots_render_exactly_as_they_did_before_they_had_slots` in
/// `iridium_editor::syntax` is what proves that half; what follows below is
/// what replaced the category half.
#[test]
fn the_ruled_selector_categories_are_the_ones_that_were_ruled() {
    for (name, expected) in [
        ("selector.class", HighlightType::Type),
        ("selector.id", HighlightType::Constant),
        ("selector.pseudo", HighlightType::Attribute),
    ] {
        assert_eq!(
            HighlightType::from_capture_name(name),
            Some(expected),
            "@{name} no longer carries the category #70 ruled for it"
        );
    }

    assert_ne!(
        HighlightType::from_capture_name("selector.class"),
        HighlightType::from_capture_name("selector.id"),
        "a class and an id must not render identically — that is the \
         specificity distinction the ruling deliberately kept"
    );
}

/// Every capture name the vendored markdown queries write, and the category it
/// was ruled onto.
///
/// Both spellings appear: Zed's markdown queries suffix the family
/// (`title.markup`) and the vendored `gitcommit` query uses the nvim/helix
/// prefix form (`markup.heading`). Two names for one concept must reach one
/// category or a heading in a commit message and a heading in a README style
/// differently, which no theme author would ever think to look for.
#[test]
fn every_markup_capture_carries_the_category_it_was_ruled_onto() {
    for (name, expected) in [
        ("title.markup", HighlightType::MarkupHeading),
        ("markup.heading", HighlightType::MarkupHeading),
        ("emphasis.markup", HighlightType::MarkupEmphasis),
        ("emphasis.strong.markup", HighlightType::MarkupStrong),
        ("strikethrough.markup", HighlightType::MarkupStrikethrough),
        ("text.literal.markup", HighlightType::MarkupCode),
        ("link_text.markup", HighlightType::MarkupLink),
        ("link_uri.markup", HighlightType::MarkupUrl),
        ("markup.link.url", HighlightType::MarkupUrl),
        ("punctuation.list_marker.markup", HighlightType::MarkupList),
        ("punctuation.markup", HighlightType::MarkupPunctuation),
        ("punctuation.embedded.markup", HighlightType::MarkupFence),
    ] {
        assert_eq!(
            HighlightType::from_capture_name(name),
            Some(expected),
            "@{name} no longer carries the category ruled for it"
        );
    }
}

/// ⭐ The property the split exists for, asserted as a property rather than as
/// a list of pairs.
///
/// The defect was not that markup wore the wrong colours — it wore reasonable
/// ones. It was that markup captures *were* code captures: `title.markup` and
/// `keyword` resolved to one value, so "headings are bold" could not be said in
/// a theme without bolding every `fn` and `let` in every language. That is
/// invisible in review, ships silently, and gets blamed on the theme author six
/// weeks later.
///
/// Written as "no markup capture shares a category with any code capture"
/// rather than as twelve equalities, so it keeps holding when a category is
/// added: a new markup capture quietly given `Keyword` fails here without
/// anyone remembering to extend a list.
#[test]
fn the_markup_captures_have_slots_no_code_capture_shares() {
    const MARKUP: &[&str] = &[
        "title.markup",
        "markup.heading",
        "emphasis.markup",
        "emphasis.strong.markup",
        "strikethrough.markup",
        "text.literal.markup",
        "link_text.markup",
        "link_uri.markup",
        "markup.link.url",
        "punctuation.list_marker.markup",
        "punctuation.markup",
        "punctuation.embedded.markup",
    ];

    // Ordinary code captures, one per family, spelled as the grammars spell
    // them. These are the values a markup category must never be equal to.
    const CODE: &[&str] = &[
        "keyword",
        "keyword.control",
        "string",
        "string.escape",
        "number",
        "boolean",
        "comment",
        "comment.doc",
        "function",
        "function.method",
        "variable",
        "type",
        "operator",
        "punctuation.bracket",
        "punctuation.delimiter",
        "punctuation.special",
        "property",
        "constant",
        "lifetime",
        "attribute",
        "tag",
        "embedded",
        "error",
    ];

    for &markup in MARKUP {
        let category = HighlightType::from_capture_name(markup)
            .unwrap_or_else(|| panic!("@{markup} must map to a category"));
        for &code in CODE {
            assert_ne!(
                Some(category),
                HighlightType::from_capture_name(code),
                "@{markup} and @{code} are the same category again, so a theme \
                 cannot style one without styling the other — which is the \
                 whole defect the markup split removed"
            );
        }
    }
}

#[test]
fn the_unstyled_lists_name_only_captures_that_are_still_unstyled_and_still_used() {
    // Without this, the list rots in the two ways an exception list can:
    // a name that gets mapped stays listed as an exception forever, and a
    // capture dropped by a vendor refresh leaves a row nothing checks. Either
    // way the list stops describing the tree it claims to describe.
    //
    // ⭐ This is the test that proved #70's shrink was real rather than
    // asserted: mapping the six and leaving them listed would fail here, so
    // the fix and the bookkeeping could not come apart.
    let mut vendored: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for &language in Language::all() {
        if let Some(query) = crate::query::compiled(language, crate::query::QueryKind::Highlights)
            .expect("a vendored highlights.scm must compile")
        {
            vendored.extend(query.capture_names().iter().copied());
        }
    }

    for &name in DELIBERATELY_UNSTYLED {
        assert!(
            vendored.contains(name),
            "@{name} is listed as unstyled but no vendored query captures it"
        );
        assert!(
            HighlightType::from_capture_name(name).is_none(),
            "@{name} now maps to a highlight type, so it must come off the \
             unstyled list — and if that was the fix, the list is how anyone \
             knows what is left"
        );
    }
}

/// `<div>` and `<Foo>` must both reach the screen coloured.
///
/// The TSX query splits JSX element names by case. Lowercase names — the
/// intrinsic HTML elements — are captured as `@tag.jsx` behind a
/// `(#match? "^[a-z][^.]*$")` predicate at `tsx/highlights.scm:385-387`;
/// uppercase names fall to the unpredicated `@type` pattern at `:389`. Only
/// the second was mapped, so a component was coloured and an intrinsic element
/// was not, in the same file and often on the same line.
///
/// Asserted end-to-end through the real query rather than as a
/// `from_capture_name` equality, because the predicate is what decides which
/// of the two patterns fires and a mapping test cannot see it.
#[test]
fn jsx_intrinsic_elements_are_coloured_like_components_are() {
    const SOURCE: &str = "const a = <div><Foo /></div>;\n";
    let spans = spans_for(Language::Tsx, SOURCE);

    let at = |needle: &str| {
        let start = SOURCE.find(needle).expect("needle is in the source");
        spans
            .iter()
            .find(|span| span.start == start && span.end == start + needle.len())
            .map(|span| span.highlight)
    };

    assert_eq!(
        at("div"),
        Some(HighlightType::Tag),
        "the intrinsic element `div` rendered unstyled while `Foo` did not"
    );
    assert_eq!(
        at("Foo"),
        Some(HighlightType::Type),
        "the component arm is the control: it was already styled and must stay so"
    );
}

/// No byte range may carry two highlights.
///
/// This is the invariant `spans_with` exists to hold, and it cannot be checked
/// downstream: `HighlightSpan`'s `Ord` compares `(start, end)` only, so a pair
/// of spans over one range is invisible to any sort or `dedup` a consumer
/// applies — and the desktop resolver's `sort_unstable` would pick between
/// them arbitrarily. Asserted over every language rather than TSX alone,
/// because the shape that produces it — a predicated pattern above a broad
/// fallback — is a general grammar idiom, not a TSX quirk.
#[test]
fn no_byte_range_carries_two_different_highlights() {
    // One source per language would be a large fixture set for a property that
    // holds structurally, so this drives the languages whose queries are known
    // to stack a predicated pattern over a fallback on the same node.
    const CASES: &[(Language, &str)] = &[
        (Language::Tsx, "const a = <div><Foo x={1} /></div>;\n"),
        (
            Language::TypeScript,
            "function f(a: string): number { return a.length; }\n",
        ),
        (Language::JavaScript, "const x = { a: 1 }; foo(x.a);\n"),
        (Language::Css, "a.b#c:hover { color: red; }\n"),
        (Language::Rust, "fn main() { let x: u32 = 1; }\n"),
    ];

    let mut conflicts: Vec<String> = Vec::new();
    for &(language, source) in CASES {
        let spans = spans_for(language, source);
        for pair in spans.windows(2) {
            let (left, right) = (&pair[0], &pair[1]);
            if left.start == right.start && left.end == right.end {
                conflicts.push(format!(
                    "  {} {}..{} {:?} — {:?} vs {:?}",
                    language.id(),
                    left.start,
                    left.end,
                    &source[left.start..left.end],
                    left.highlight,
                    right.highlight
                ));
            }
        }
    }

    assert!(
        conflicts.is_empty(),
        "{} byte range(s) carry more than one highlight:\n{}",
        conflicts.len(),
        conflicts.join("\n")
    );
}

#[test]
fn a_namespace_capture_is_styled_rather_than_falling_through() {
    // The regression that motivated the sweep above. AWL, C++, CSS and Go all
    // capture @namespace, and it reached the screen unstyled.
    assert_eq!(
        HighlightType::from_capture_name("namespace"),
        Some(HighlightType::Type)
    );
    assert_eq!(
        HighlightType::from_capture_name("@namespace"),
        Some(HighlightType::Type)
    );
    assert_eq!(
        HighlightType::from_capture_name("module"),
        Some(HighlightType::Type)
    );
}
