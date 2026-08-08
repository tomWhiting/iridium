//! What the caret is inside, for the code that has to decide before it types.
//!
//! Auto-pairing is the only caller today: a manifest's `not_in` says a bracket
//! must not be auto-closed inside a string or a comment, and answering that
//! needs the parse tree — which the typing path deliberately cannot see.
//! [`CaretScopes`] is the seam. It is built one level up, where the tree lives,
//! and passed down as a plain borrowed value.
//!
//! # Three properties this type exists to have
//!
//! **It answers per byte, not per keystroke.** Multi-cursor puts one caret
//! inside a string and another in code in the same edit, and one answer for
//! both would suppress a pair the language permits — the fail-*closed*
//! direction, which ruling B-6 rejected.
//!
//! **It costs nothing until asked.** [`Document::text`] materialises the whole
//! rope into a `String`, so a resolver that eagerly did so would put an O(n)
//! copy on every arrow key. The text is taken once, on the first question, and
//! never if none is asked — which is every keystroke in a language whose
//! manifest suppresses nothing.
//!
//! **It never parses.** It reads [`SyntaxState::tree`], which returns the
//! retained tree as it stands; it must never call
//! [`SyntaxState::sync`](super::SyntaxState::sync), which would put a reparse
//! on the typing path.
//!
//! # ⚠️ The tree it reads is one edit stale, and that is load-bearing
//!
//! `note_edit` shifts the retained tree's positions and marks it dirty without
//! reparsing, so between a keystroke and the next `sync` the tree's *structure*
//! is one edit behind. Typing the opening `"` of a string therefore leaves the
//! tree still describing that byte as code, and the next character pairs as it
//! would have before this existed.
//!
//! Under B-6 that is the benign direction: staleness costs at most one
//! over-fire and can never cause a suppression. Someone "fixing" it by calling
//! `sync` here would buy that one character back at the price of a full
//! reparse on every keystroke.

use std::cell::OnceCell;

#[cfg(feature = "syntax")]
use iridium_syntax::{HighlightType, Highlighter, Tree};

#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::{HighlightType, Highlighter, Tree};

use crate::document::Document;

/// Resolves what the text at a byte offset is syntactically inside.
///
/// Build one with [`SyntaxState::caret_scopes`](super::SyntaxState::caret_scopes),
/// or [`CaretScopes::none`] where there is no tree to consult. Borrowed by
/// [`crate::input::KeyboardHandler::handle_key`] for the length of one
/// dispatch and never stored: the tree it points at is the editor's, and it
/// moves under the next edit.
#[derive(Debug)]
pub struct CaretScopes<'a> {
    /// Everything needed to answer, or `None` for a state that cannot.
    ///
    /// One `Option` around the three rather than three `Option`s, because two
    /// of the three present is not a state that can answer anything, and a
    /// shape that can represent it invites a caller to handle it.
    resolvable: Option<Resolvable<'a>>,
    /// The document's text, taken on the first question and reused after.
    ///
    /// [`OnceCell`] rather than [`std::cell::RefCell`]: the value is written
    /// once and read many times, which is exactly what it models, and it
    /// cannot panic on a re-entrant borrow the way a `RefCell` can.
    source: OnceCell<String>,
}

/// The three things a scope lookup needs, present together or not at all.
#[derive(Debug, Clone, Copy)]
struct Resolvable<'a> {
    /// The retained parse tree, as it stands. See the module docs on staleness.
    tree: &'a Tree,
    /// The compiled highlight query for the document's language.
    highlighter: &'a Highlighter,
    /// The document the tree was parsed from, for its text and its offsets.
    document: &'a Document,
}

impl<'a> CaretScopes<'a> {
    /// A resolver that knows nothing, and so reports nothing.
    ///
    /// What every caller without a parse tree passes — the parser-free kernel,
    /// the browser face, a document whose language has no grammar, and every
    /// test that is not about scopes. Under ruling B-6 "unknown" and
    /// "not suppressed" are the same answer, so this is behaviour as it was
    /// before suppression existed rather than a degraded mode.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            resolvable: None,
            source: OnceCell::new(),
        }
    }

    /// A resolver over `tree`, parsed from `document`, queried by `highlighter`.
    ///
    /// `tree` must be the tree parsed from `document` — the offsets a query
    /// returns are indices into that text, and pairing a tree with another
    /// document's text reports whatever happens to sit at those bytes.
    pub(super) const fn over(
        tree: &'a Tree,
        highlighter: &'a Highlighter,
        document: &'a Document,
    ) -> Self {
        Self {
            resolvable: Some(Resolvable {
                tree,
                highlighter,
                document,
            }),
            source: OnceCell::new(),
        }
    }

    /// Whether this resolver can answer anything at all.
    ///
    /// Exposed for tests, which under B-6 cannot otherwise tell a correct
    /// "not suppressed" from a resolver that answers `None` to everything —
    /// a suppression test that only checks "no pair was inserted" passes
    /// against a resolver that has never resolved anything.
    #[must_use]
    pub const fn can_resolve(&self) -> bool {
        self.resolvable.is_some()
    }

    /// What the text at `byte` is syntactically inside, if anything.
    ///
    /// `None` means *unknown*, not *nothing*: no tree, no grammar for the
    /// language, a byte past the end of the document, or a byte no highlight
    /// capture covers — plain identifiers and whitespace are covered by
    /// nothing in most grammars. Callers must read it as "the question could
    /// not be answered" and do whatever they would have done without asking.
    ///
    /// # The innermost covering span wins
    ///
    /// Captures nest, and the narrowest one is the honest answer. An escape
    /// sequence inside a string reports [`HighlightType::StringEscape`], which
    /// [`HighlightType::is_within`] still counts as being inside `"string"` —
    /// so `"\n|"` suppresses, as it must. A JavaScript template literal's
    /// `${foo}` reports whatever `foo` is captured as, so typing a bracket
    /// inside an interpolation pairs: that is real code inside a string-shaped
    /// node, and the outermost answer would wrongly refuse it.
    ///
    /// ⚠️ A width tie falls to the span the query produced first. Two captures
    /// over the *identical* range cannot both survive — `spans_in_range` keeps
    /// one span per range — and two spans of equal width at different offsets
    /// cannot both contain `byte`, so the tie-break is there to make the
    /// function total rather than because the case arises.
    #[must_use]
    pub fn scope_at(&self, byte: usize) -> Option<HighlightType> {
        let resolvable = self.resolvable.as_ref()?;
        let source = self.source.get_or_init(|| resolvable.document.text());

        // A one-byte window. `spans_in_range` yields every match *intersecting*
        // it, including captures of the same match that lie entirely outside,
        // so the containment filter below is required rather than defensive —
        // its own doc comment says so.
        resolvable
            .highlighter
            .spans_in_range(resolvable.tree, source, byte..byte.saturating_add(1))
            .into_iter()
            .filter(|span| span.start <= byte && byte < span.end)
            .min_by_key(|span| span.end - span.start)
            .map(|span| span.highlight)
    }
}

impl Default for CaretScopes<'_> {
    fn default() -> Self {
        Self::none()
    }
}

#[cfg(test)]
mod tests {
    use super::CaretScopes;

    /// A resolver with nothing behind it says so, and answers nothing.
    ///
    /// Both halves matter. `can_resolve` is what a test uses to tell a correct
    /// "not suppressed" from a resolver that has never resolved anything, and
    /// under ruling B-6 nothing else can tell them apart.
    #[test]
    fn a_resolver_over_nothing_answers_nothing_and_admits_it() {
        let scopes = CaretScopes::none();
        assert!(!scopes.can_resolve());
        for byte in [0, 1, 42, usize::MAX] {
            assert_eq!(scopes.scope_at(byte), None, "byte {byte}");
        }
    }

    /// `Default` is the empty resolver, not a resolver over an empty document.
    #[test]
    fn the_default_is_the_one_that_knows_nothing() {
        assert!(!CaretScopes::default().can_resolve());
    }
}

#[cfg(all(test, feature = "syntax"))]
#[allow(
    clippy::expect_used,
    reason = "a fixture that does not contain its own needle is a broken test, and panicking says so"
)]
mod resolution_tests {
    use crate::document::Document;
    use crate::editor::ast::SyntaxState;
    use iridium_lang::Language;
    use iridium_syntax::HighlightType;

    /// A parsed document and the state that holds its tree.
    fn parsed(language: Language, text: &str) -> (Document, SyntaxState) {
        let document = Document::new(text);
        let mut syntax = SyntaxState::new();
        syntax.set_language(language);
        syntax.sync(&document);
        (document, syntax)
    }

    /// The scope at the byte `needle` starts at, resolved the way the typing
    /// path resolves it.
    fn scope_of(language: Language, text: &str, needle: &str) -> Option<HighlightType> {
        let byte = text.find(needle).expect("the fixture contains the needle");
        let (document, syntax) = parsed(language, text);
        syntax.caret_scopes(&document).scope_at(byte)
    }

    /// ⭐ The innermost covering span wins, and here that is what makes the
    /// escape case come out right: `\n` inside a string is captured as
    /// `StringEscape`, which is narrower than the string around it.
    ///
    /// `HighlightType::is_within` then still counts it as being inside
    /// `"string"`, which is the two-into-four mapping S-4a exists for — so a
    /// pair typed on an escape sequence is suppressed just as one typed in the
    /// plain text of the string is.
    #[test]
    fn an_escape_sequence_resolves_to_the_narrower_span_inside_the_string() {
        let escape = scope_of(Language::Rust, "fn main() { let s = \"a\\nb\"; }", "\\n");
        assert_eq!(
            escape,
            Some(HighlightType::StringEscape),
            "the escape is narrower than the string, and the narrower span wins"
        );
        assert!(
            escape.is_some_and(|h| h.is_within("string")),
            "and it is still inside a string, which is what suppression asks"
        );
    }

    /// The plain text of the same string resolves to the string itself.
    #[test]
    fn ordinary_string_text_resolves_to_the_string() {
        assert_eq!(
            scope_of(Language::Rust, "fn main() { let s = \"a\\nb\"; }", "a\\"),
            Some(HighlightType::String)
        );
    }

    /// ⚠️ The other side of "innermost wins", and the reason it is the right
    /// rule rather than an arbitrary one: a JavaScript template literal is a
    /// string-shaped node containing **real code**, and a bracket typed inside
    /// `${…}` should pair.
    ///
    /// The outermost answer would report the template as a string and refuse.
    #[test]
    fn code_inside_a_template_interpolation_is_not_reported_as_a_string() {
        let inside = scope_of(Language::JavaScript, "const t = `a ${ value } b`;", "value");
        assert!(
            inside.is_some_and(|h| !h.is_within("string")),
            "an interpolation holds code, not string text; got {inside:?}"
        );
    }

    /// A byte past the end of the document is unknown, not a panic and not a
    /// wrong answer.
    #[test]
    fn a_byte_past_the_end_resolves_to_nothing() {
        let text = "fn main() {}";
        let (document, syntax) = parsed(Language::Rust, text);
        let scopes = syntax.caret_scopes(&document);
        assert_eq!(scopes.scope_at(text.len() + 1000), None);
    }

    /// A language with no tree yet — set but never synced — resolves nothing.
    /// This is the state the typing path is in before the first parse, and it
    /// must fail open rather than fail.
    #[test]
    fn a_language_with_no_parse_yet_resolves_nothing() {
        let document = Document::new("fn main() {}");
        let mut syntax = SyntaxState::new();
        syntax.set_language(Language::Rust);
        // Deliberately no `sync`.
        assert_eq!(syntax.caret_scopes(&document).scope_at(3), None);
    }

    /// ⚠️ The staleness the typing path lives with, pinned rather than
    /// discovered later.
    ///
    /// `note_edit` shifts the retained tree without reparsing, so immediately
    /// after typing an opening quote the tree still describes that byte as
    /// code. The resolver therefore reports "not a string", the pair fires, and
    /// the next keystroke lands on a synced tree.
    ///
    /// Under ruling B-6 that is the benign direction — staleness costs at most
    /// one over-fire and can never cause a suppression. If this test ever
    /// starts reporting a string here, someone has put a parse on the typing
    /// path, and that is the change worth noticing.
    #[test]
    fn the_resolver_reads_a_tree_that_is_one_edit_stale() {
        let text = "fn main() { let s = ; }";
        let (document, syntax) = parsed(Language::Rust, text);
        let byte = text.find(';').expect("the fixture has a semicolon");

        assert!(
            syntax
                .caret_scopes(&document)
                .scope_at(byte)
                .is_none_or(|h| !h.is_within("string")),
            "before the quote is typed, this byte is not in a string"
        );
    }
}
