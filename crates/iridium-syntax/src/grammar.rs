//! The one table mapping a supported language to its tree-sitter grammar.
//!
//! Before this module there were two — one inside the highlighter and one
//! inside the fold detector — built as `HashMap`s at every construction. Two
//! tables is one table too many: a grammar registered in one and forgotten in
//! the other produces a language that highlights but never folds, and nothing
//! in the type system notices.
//!
//! # Why this returns an `Option`
//!
//! This match used to be exhaustive over a closed `Language` enum, and the
//! function was therefore total: every language Iridium supported had a
//! grammar, and the signature said so.
//!
//! [`Language`] is now an index into a generated registry, and the set of
//! languages is data. A grammar is not: it is a statically linked C symbol,
//! which either got linked or did not. So the two sets can differ, and the
//! signature has to admit it.
//!
//! **A language with no grammar is a supported language that cannot be
//! parsed**, which is a real and useful state rather than a defect. File
//! association, comment tokens, indent rules and auto-pairs all come from the
//! vendored manifest, and not one of them needs a parser — so `diff`, `go.mod`
//! and a git commit message can be first-class here long before anyone links a
//! grammar for them.
//!
//! Callers must degrade rather than fail. The one that matters is
//! `query::compile`, where no grammar means there is nothing to compile a query
//! against, so the query resolves to the same routine absence it already models
//! for a language shipping no `.scm` of that kind. No new error variant was
//! needed, which is the sign the model was already right.

use crate::Language;

/// Returns the tree-sitter grammar for a language, if one is linked.
///
/// The returned value is cheap to produce and cheap to copy — tree-sitter's
/// `Language` wraps a pointer to a statically linked grammar, so this is a
/// lookup rather than a load, and callers need not cache it.
///
/// `None` means no grammar is linked for that language. See the module note:
/// that is routine, not an error.
///
/// Matched on the identifier rather than on the value so that the arms name
/// what they mean — and so that a language whose id changes fails a test rather
/// than silently falling into the `None` arm.
pub fn grammar(language: Language) -> Option<tree_sitter::Language> {
    Some(match language.id() {
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "python" => tree_sitter_python::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "go" => tree_sitter_go::LANGUAGE.into(),
        "json" => tree_sitter_json::LANGUAGE.into(),
        "yaml" => tree_sitter_yaml::LANGUAGE.into(),
        "markdown" => tree_sitter_md::LANGUAGE.into(),
        "css" => tree_sitter_css::LANGUAGE.into(),
        "bash" => tree_sitter_bash::LANGUAGE.into(),
        "c" => tree_sitter_c::LANGUAGE.into(),
        "cpp" => tree_sitter_cpp::LANGUAGE.into(),
        _ => return None,
    })
}

/// Whether a grammar is linked for this language — whether it can be parsed.
///
/// The question a caller usually wants answered before deciding a
/// syntax-driven feature is available. Asking this is better than constructing
/// a [`crate::SyntaxTree`] and discarding the error, because a language with no
/// grammar is a routine state and not a failure worth producing an error value
/// for.
#[must_use]
pub fn has_grammar(language: Language) -> bool {
    grammar(language).is_some()
}

#[cfg(test)]
mod tests {
    use super::grammar;
    use crate::Language;
    use tree_sitter::Parser;

    /// The languages a grammar is linked for, written out by hand.
    ///
    /// Asserted rather than derived from the table above, because a derived
    /// list would agree with that table however wrong it was. This is the claim
    /// "Iridium can parse these", and it is the thing a reader wants to check.
    const GRAMMARS_LINKED_FOR: &[&str] = &[
        "rust",
        "python",
        "typescript",
        "javascript",
        "tsx",
        "go",
        "json",
        "yaml",
        "markdown",
        "css",
        "bash",
        "c",
        "cpp",
    ];

    #[test]
    fn a_grammar_is_linked_for_exactly_the_languages_claimed() {
        for &language in Language::all() {
            assert_eq!(
                grammar(language).is_some(),
                GRAMMARS_LINKED_FOR.contains(&language.id()),
                "{} disagrees with GRAMMARS_LINKED_FOR",
                language.id()
            );
        }

        // The other direction: a name in the list that is no longer a language
        // would otherwise sit there unchecked forever.
        for &id in GRAMMARS_LINKED_FOR {
            assert!(
                Language::from_id(id).is_some(),
                "GRAMMARS_LINKED_FOR names {id:?}, which is not a language"
            );
        }
    }

    #[test]
    fn every_linked_grammar_is_one_a_parser_will_accept() {
        for &language in Language::all() {
            let Some(grammar) = grammar(language) else {
                continue;
            };
            let mut parser = Parser::new();
            parser.set_language(&grammar).unwrap_or_else(|error| {
                panic!(
                    "the {} grammar is incompatible with this tree-sitter: {error}",
                    language.id()
                )
            });
            let tree = parser.parse("", None).expect("an empty parse must succeed");
            assert_eq!(
                tree.root_node().start_byte(),
                0,
                "{} parsed an empty document into something that does not start at 0",
                language.id()
            );
        }
    }

    #[test]
    fn distinct_languages_do_not_share_one_grammar() {
        // TSX and TypeScript come from the same crate but are different
        // grammars; JavaScript and TypeScript are frequently confused when a
        // table is written by hand. Comparing every pair catches a copy-paste
        // that would silently highlight one language with another's rules.
        //
        // Only linked pairs are compared: two languages with no grammar both
        // answer `None`, and that is agreement about absence rather than a
        // shared grammar.
        for &left in Language::all() {
            for &right in Language::all() {
                if left == right {
                    continue;
                }
                let (Some(left_grammar), Some(right_grammar)) = (grammar(left), grammar(right))
                else {
                    continue;
                };
                assert_ne!(
                    left_grammar,
                    right_grammar,
                    "{} and {} resolve to the same grammar",
                    left.id(),
                    right.id()
                );
            }
        }
    }
}
