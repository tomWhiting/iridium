//! Reading capture names out of `.scm` source text.
//!
//! # Why this exists when tree-sitter can already answer
//!
//! A compiled `tree_sitter::Query` reports its own capture names, and that is
//! the better answer wherever it is available: it is the parser's, not a
//! guess. It is only available where a **grammar is linked**, because a query
//! cannot be compiled without one — and Iridium ships eight vendored query
//! directories with no grammar behind them (`diff`, `gitcommit`, `gomod`,
//! `gowork`, `jsdoc`, `jsonc`, `markdown-inline`, `regex`).
//!
//! So any check over "every capture name Iridium vendors" is otherwise a check
//! over "every capture name in the subset of languages somebody has linked a
//! grammar for" — which reads as the first, silently shrinks when a grammar is
//! dropped, and is exactly the blind spot that let six unmapped names sit
//! behind it.
//!
//! ⚠️ **This is a lexer, not a parser, and the difference is the whole risk.**
//! An `@` in a `.scm` file is a capture *only* when it is in code position. The
//! vendored tree already contains both counter-examples:
//!
//! - `css/highlights.scm` writes `"@media"`, `"@import"` and `"@namespace"` as
//!   **string literals** — anonymous node names, not captures. A scan that took
//!   them would invent six capture names CSS does not have, one of which
//!   (`namespace`) even maps, so half the invention would be invisible.
//! - `diff/highlights.scm` writes ``;; TODO: This should eventually be
//!   `@diff.plus` `` in a **comment**. A scan that took it would report a
//!   capture the query does not contain — which is precisely how `diff.plus`
//!   and `diff.minus` came to be recorded against `diff` when they are only
//!   ever written by `gitcommit`.
//! - `python/highlights.scm:40` writes `(decorator "@" @punctuation.special)`
//!   — a string `@` and a real capture on one line, so string handling that
//!   swallows to the end of the line takes the capture with it.
//!
//! Comments and strings are therefore skipped by the scan below, and the claim
//! that it does so correctly is not left to this comment: `iridium-syntax`
//! compares this function's answer against **tree-sitter's own** for every
//! language that has a grammar, and only uses it where tree-sitter cannot
//! answer.
//!
//! # ⚠️ What that oracle does and does not reach — measured, not assumed
//!
//! **String handling it reaches.** Deleting the string arm below makes the
//! oracle fail on CSS, naming the six invented captures (`charset`, `import`,
//! `keyframes`, `media`, `namespace`, `supports`) against tree-sitter's own
//! list. Measured by mutation.
//!
//! ⛔ **Comment handling it does NOT reach.** Deleting the comment arm leaves
//! *both* the oracle and the ratchet green. The oracle cannot see it because
//! the only `highlights.scm` that writes an `@` in a comment *and* has a
//! grammar is `awl`, whose comment mentions `@string` — a name that file
//! already captures, so the set is unchanged. And the ratchet cannot see it
//! because the names a comment-blind scan invents (`diff.plus`, `diff.minus`)
//! now map, so nothing is reported unmapped.
//!
//! The comment rule's only guards are therefore
//! `an_at_sign_inside_a_comment_is_prose_not_a_capture` and
//! `multibyte_text_is_stepped_over_whole` below — both of which do fail against
//! a comment-blind scan, and the first of which uses `diff/highlights.scm`'s
//! real text rather than an invented fixture. Written down because "the oracle
//! covers the scan" is the natural thing to assume here and it is two-thirds
//! true, which is the most dangerous fraction.

/// Every capture name written in a tree-sitter query, in the order they appear.
///
/// The leading `@` is stripped, matching what `tree_sitter::Query::capture_names`
/// returns, so the two can be compared directly. Duplicates are **kept**: a
/// query naming `@keyword` in nine patterns yields it nine times, and a caller
/// wanting the set says so with a `BTreeSet`. Deduplicating here would make
/// "how many patterns use this" unaskable to save a caller one line.
///
/// Text in comments and inside string literals is skipped — see the module
/// note for the three vendored files that make that load-bearing rather than
/// theoretical.
///
/// A `Vec` rather than an iterator because the borrow-carrying lexer state
/// would be a type twice the size of the loop it replaces, and every caller —
/// a diagnostic, a gate, a face reporting unstyled tokens — wants the whole
/// answer at once over a file of a few thousand bytes. Nothing calls this per
/// frame or per keystroke; the compiled-query cache is what the render path
/// uses.
#[must_use]
pub fn capture_names(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut names = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        match bytes[index] {
            // A comment runs to the end of the line. Both `;` and `;;` are
            // written in the vendored tree; one rule covers them because the
            // second `;` is just the first character of the comment body.
            b';' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
            },

            // A string literal. `\` escapes the next byte, which is how
            // `go/brackets.scm` writes `"\""` — without this the scan would
            // treat the escaped quote as the closing one and read the rest of
            // the file in the wrong mode.
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                // Past the closing quote, or past the end of an unterminated
                // one — which cannot compile, so there is nothing to salvage.
                index += 1;
            },

            b'@' => {
                let start = index + 1;
                let mut end = start;
                while end < bytes.len() && is_name_byte(bytes[end]) {
                    end += 1;
                }
                if end > start {
                    // ASCII throughout: `@` is one byte and every name byte is
                    // one byte, so this cannot split a character.
                    names.push(&source[start..end]);
                }
                // `end` is at least `start`, which is past the `@`, so the loop
                // advances even for a bare `@` with no name after it.
                index = end;
            },

            _ => index += 1,
        }
    }

    names
}

/// Whether a byte can appear in a capture name.
///
/// `.` is the hierarchy separator every vendored query uses
/// (`@function.method`), `_` opens the predicate-operand convention
/// (`@_isinstance`), and `-` appears in names a grammar may borrow from a
/// hyphenated language id. Anything else ends the name, which is what makes
/// `(deletion) @keyword)` and `@number,` read correctly without special cases.
const fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-')
}

#[cfg(test)]
mod tests {
    use super::capture_names;

    #[test]
    fn a_plain_capture_is_read_without_its_at_sign() {
        assert_eq!(capture_names("(comment) @comment\n"), ["comment"]);
    }

    #[test]
    fn a_hierarchical_name_keeps_its_dots() {
        assert_eq!(
            capture_names("(x) @punctuation.list_marker.markup\n"),
            ["punctuation.list_marker.markup"]
        );
    }

    /// ⭐ The CSS case, which is in the vendored tree today. Six of these
    /// would be invented capture names, and `namespace` would even map — so
    /// the ratchet would stay green while reporting on a language that does
    /// not contain them.
    #[test]
    fn an_at_sign_inside_a_string_is_an_anonymous_node_not_a_capture() {
        let source = "[\n  \"@media\"\n  \"@import\"\n  \"@namespace\"\n] @keyword\n";
        assert_eq!(capture_names(source), ["keyword"]);
    }

    /// ⭐ The `diff` case. This is the exact text that put `diff.plus` and
    /// `diff.minus` on a measured list of unmapped names for a file that never
    /// captures either.
    #[test]
    fn an_at_sign_inside_a_comment_is_prose_not_a_capture() {
        let source = "[\n  (addition)\n] @string\n\
                      ;; TODO: This should eventually be `@diff.plus` with a fallback of `@string`\n";
        assert_eq!(capture_names(source), ["string"]);
    }

    /// ⭐ `python/highlights.scm:40`, verbatim. A string and a capture on one
    /// line: skipping to the end of the line on `"` would lose the capture,
    /// and not skipping the string at all would invent one.
    #[test]
    fn a_string_and_a_capture_on_one_line_are_told_apart() {
        assert_eq!(
            capture_names("(decorator \"@\" @punctuation.special)\n"),
            ["punctuation.special"]
        );
    }

    /// `go/brackets.scm:4`, verbatim. An escaped quote must not be read as the
    /// end of the string — if it were, the scan would fall out of string mode
    /// and every `@` after it in the file would be misread.
    #[test]
    fn an_escaped_quote_does_not_end_a_string() {
        assert_eq!(
            capture_names("((\"\\\"\" @open \"\\\"\" @close) (#set! rainbow.exclude))\n"),
            ["open", "close"]
        );
    }

    /// Predicate operands are captures and must be reported as such — the
    /// convention that they carry no colour is a *mapping* decision, made
    /// against a list one layer up. Dropping them here would move that decision
    /// into a lexer where nobody would find it.
    #[test]
    fn a_predicate_operand_is_still_a_capture() {
        assert_eq!(
            capture_names("((identifier) @_isinstance (#eq? @_isinstance \"isinstance\"))\n"),
            ["_isinstance", "_isinstance"]
        );
    }

    /// Duplicates survive, in file order. A caller wanting the set says so.
    #[test]
    fn every_occurrence_is_reported_rather_than_the_set() {
        assert_eq!(
            capture_names("(a) @keyword\n(b) @keyword\n(c) @string\n"),
            ["keyword", "keyword", "string"]
        );
    }

    /// Malformed input must terminate rather than hang or panic. None of these
    /// can compile; the scan's only duty is to be harmless.
    #[test]
    fn malformed_source_terminates_without_panicking() {
        for source in [
            "@",
            "@@",
            "\"unterminated",
            "\"unterminated \\",
            ";",
            ";; a comment with no newline",
            "",
            "(x) @",
        ] {
            let _ = capture_names(source);
        }
    }

    /// Non-ASCII text must not be split mid-character. A `.scm` may carry a
    /// comment in any language, and a string literal may hold any node name a
    /// grammar defines.
    #[test]
    fn multibyte_text_is_stepped_over_whole() {
        assert_eq!(
            capture_names("; ⭐ a note about @nothing\n(\"→\") @operator\n"),
            ["operator"]
        );
    }
}
