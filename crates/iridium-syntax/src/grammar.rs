//! The one table mapping a supported language to its tree-sitter grammar.
//!
//! Before this module there were two — one inside the highlighter and one
//! inside the fold detector — built as `HashMap`s at every construction. Two
//! tables is one table too many: a grammar registered in one and forgotten in
//! the other produces a language that highlights but never folds, and nothing
//! in the type system notices.
//!
//! The match below is exhaustive over [`Language`], so adding a variant without
//! a grammar is a compile error rather than a runtime `None`. That is why the
//! function is total: every language Iridium claims to support has a grammar,
//! and the signature now says so.

use crate::Language;

/// Returns the tree-sitter grammar for a language.
///
/// The returned value is cheap to produce and cheap to copy — tree-sitter's
/// `Language` wraps a pointer to a statically linked grammar, so this is a
/// lookup rather than a load, and callers need not cache it.
pub fn grammar(language: Language) -> tree_sitter::Language {
    match language {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
        Language::Go => tree_sitter_go::LANGUAGE.into(),
        Language::Json => tree_sitter_json::LANGUAGE.into(),
        Language::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        Language::Markdown => tree_sitter_md::LANGUAGE.into(),
        Language::Css => tree_sitter_css::LANGUAGE.into(),
        Language::Bash => tree_sitter_bash::LANGUAGE.into(),
        Language::C => tree_sitter_c::LANGUAGE.into(),
        Language::Cpp => tree_sitter_cpp::LANGUAGE.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::grammar;
    use crate::Language;
    use tree_sitter::Parser;

    #[test]
    fn every_language_has_a_grammar_a_parser_will_accept() {
        for &language in Language::all() {
            let mut parser = Parser::new();
            parser
                .set_language(&grammar(language))
                .unwrap_or_else(|error| {
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
        for &left in Language::all() {
            for &right in Language::all() {
                if left == right {
                    continue;
                }
                assert_ne!(
                    grammar(left),
                    grammar(right),
                    "{} and {} resolve to the same grammar",
                    left.id(),
                    right.id()
                );
            }
        }
    }
}
