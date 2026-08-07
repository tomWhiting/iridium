//! Which node kinds are foldable, per language.
//!
//! A static table rather than a query file: fold detection asks one question of
//! every node it visits, and a `&'static [&'static str]` membership test is the
//! cheapest honest way to answer it.
//!
//! This table is Iridium's own invention — the vendored manifests carry
//! `line_comments` and `brackets` but say nothing about which node kinds fold.
//! Whether it should instead be derived from the vendored `outline.scm` is L-6
//! in `docs/IN-FLIGHT-languages.md`, and is deliberately **not** settled here:
//! the registry work has no business quietly changing how folds are decided.
//!
//! Matched on the identifier since [`Language`] stopped being a closed enum. A
//! language with no row folds nothing, which is the right default — it is what
//! a language whose grammar Iridium cannot parse would do anyway.

use crate::Language;

/// Node types that represent foldable blocks for each language.
#[derive(Debug)]
pub struct FoldableNodeTypes {
    /// Node types that represent block structures
    pub blocks: &'static [&'static str],
    /// Node types that represent comments
    pub comments: &'static [&'static str],
    /// Node types that represent import statements
    pub imports: &'static [&'static str],
}

impl FoldableNodeTypes {
    pub fn for_language(language: Language) -> Self {
        match language.id() {
            "rust" => Self {
                blocks: &[
                    "function_item",
                    "impl_item",
                    "struct_item",
                    "enum_item",
                    "trait_item",
                    "mod_item",
                    "block",
                    "match_expression",
                    "if_expression",
                    "while_expression",
                    "for_expression",
                    "loop_expression",
                    "closure_expression",
                    "array_expression",
                    "tuple_expression",
                    "struct_expression",
                    "macro_definition",
                ],
                comments: &["block_comment", "line_comment"],
                imports: &["use_declaration"],
            },
            "python" => Self {
                blocks: &[
                    "function_definition",
                    "class_definition",
                    "if_statement",
                    "for_statement",
                    "while_statement",
                    "try_statement",
                    "with_statement",
                    "match_statement",
                    "case_clause",
                    "decorated_definition",
                    "lambda",
                    "list_comprehension",
                    "dictionary_comprehension",
                    "set_comprehension",
                    "generator_expression",
                ],
                comments: &["comment"],
                imports: &["import_statement", "import_from_statement"],
            },
            "typescript" | "javascript" | "tsx" => Self {
                blocks: &[
                    "function_declaration",
                    "function",
                    "arrow_function",
                    "class_declaration",
                    "class",
                    "method_definition",
                    "if_statement",
                    "for_statement",
                    "for_in_statement",
                    "while_statement",
                    "do_statement",
                    "switch_statement",
                    "switch_case",
                    "try_statement",
                    "catch_clause",
                    "finally_clause",
                    "object",
                    "array",
                    "object_pattern",
                    "array_pattern",
                    "statement_block",
                    "export_statement",
                ],
                comments: &["comment"],
                imports: &["import_statement", "import"],
            },
            "go" => Self {
                blocks: &[
                    "function_declaration",
                    "method_declaration",
                    "if_statement",
                    "for_statement",
                    "switch_statement",
                    "type_switch_statement",
                    "select_statement",
                    "expression_case",
                    "type_case",
                    "default_case",
                    "type_declaration",
                    "struct_type",
                    "interface_type",
                    "block",
                    "composite_literal",
                ],
                comments: &["comment"],
                imports: &["import_declaration", "import_spec"],
            },
            "c" | "cpp" => Self {
                blocks: &[
                    "function_definition",
                    "struct_specifier",
                    "class_specifier",
                    "enum_specifier",
                    "union_specifier",
                    "if_statement",
                    "for_statement",
                    "while_statement",
                    "do_statement",
                    "switch_statement",
                    "case_statement",
                    "compound_statement",
                    "namespace_definition",
                    "template_declaration",
                    "declaration_list",
                    "field_declaration_list",
                    "initializer_list",
                ],
                comments: &["comment"],
                imports: &["preproc_include"],
            },
            "json" => Self {
                blocks: &["object", "array"],
                comments: &[],
                imports: &[],
            },
            "yaml" => Self {
                blocks: &[
                    "block_mapping",
                    "block_sequence",
                    "flow_mapping",
                    "flow_sequence",
                ],
                comments: &["comment"],
                imports: &[],
            },
            "markdown" => Self {
                blocks: &[
                    "section",
                    "fenced_code_block",
                    "indented_code_block",
                    "html_block",
                    "list",
                    "block_quote",
                ],
                comments: &["html_comment"],
                imports: &[],
            },
            "css" => Self {
                blocks: &[
                    "rule_set",
                    "media_statement",
                    "keyframes_statement",
                    "at_rule",
                    "block",
                    "declaration_list",
                ],
                comments: &["comment"],
                imports: &["import_statement"],
            },
            "bash" => Self {
                blocks: &[
                    "function_definition",
                    "if_statement",
                    "for_statement",
                    "while_statement",
                    "case_statement",
                    "compound_statement",
                    "subshell",
                ],
                comments: &["comment"],
                imports: &[],
            },

            // A language with no row here folds nothing. Empty rather than a
            // guess: node kind names are grammar-specific, so there is no
            // plausible default set — `block` means something in one grammar
            // and nothing in the next. Folding no regions is visibly inert,
            // where folding the wrong ones would be a bug someone has to
            // reproduce.
            _ => Self {
                blocks: &[],
                comments: &[],
                imports: &[],
            },
        }
    }
}
