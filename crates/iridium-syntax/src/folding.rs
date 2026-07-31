//! Code folding region detection.
//!
//! This module provides syntax-aware code folding by analyzing tree-sitter
//! parse trees to identify foldable regions. Fold regions are detected for:
//!
//! - Block statements (function bodies, if/else blocks, loops, etc.)
//! - Import groups (consecutive import statements)
//! - Multi-line comments (block comments and documentation)
//!
//! [`FoldKind::Region`] exists for `#region` / `#endregion` markers but nothing
//! produces it yet: recognising a region needs the *pair* of markers matched
//! across the document, which the node walk below cannot see. The variant is
//! kept so adding that later does not change the wire shape.
//!
//! The detector reads a tree it does not own — see [`crate::SyntaxTree`] — so
//! folds and highlights are always computed from the same parse.

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Tree};

use crate::Language;

/// Kind of foldable region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FoldKind {
    /// Block delimited by braces: { }
    Block,
    /// Region delimited by markers: #region / #endregion
    Region,
    /// Import statements
    Import,
    /// Multi-line comment
    Comment,
}

/// A region of code that can be folded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoldRegion {
    /// Start line (0-indexed)
    pub start_line: usize,
    /// End line (0-indexed, inclusive)
    pub end_line: usize,
    /// Kind of fold
    pub kind: FoldKind,
    /// Whether currently folded
    pub is_folded: bool,
}

impl FoldRegion {
    /// Creates a new fold region.
    #[must_use]
    pub const fn new(start_line: usize, end_line: usize, kind: FoldKind) -> Self {
        Self {
            start_line,
            end_line,
            kind,
            is_folded: false,
        }
    }

    /// Returns the number of lines in this region.
    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line) + 1
    }

    /// Returns the number of lines that would be hidden when folded.
    ///
    /// The first line is always visible (shows the fold indicator),
    /// so only lines 2..=end are hidden.
    #[must_use]
    pub const fn hidden_line_count(&self) -> usize {
        self.end_line.saturating_sub(self.start_line)
    }

    /// Returns true if the given line is within this region.
    #[must_use]
    pub const fn contains_line(&self, line: usize) -> bool {
        line >= self.start_line && line <= self.end_line
    }

    /// Returns true if this region strictly contains another region.
    #[must_use]
    pub const fn contains_region(&self, other: &Self) -> bool {
        self.start_line < other.start_line && self.end_line > other.end_line
    }
}

impl PartialOrd for FoldRegion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FoldRegion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.start_line
            .cmp(&other.start_line)
            .then_with(|| other.end_line.cmp(&self.end_line)) // Larger regions first
    }
}

/// Node types that represent foldable blocks for each language.
#[derive(Debug)]
struct FoldableNodeTypes {
    /// Node types that represent block structures
    blocks: &'static [&'static str],
    /// Node types that represent comments
    comments: &'static [&'static str],
    /// Node types that represent import statements
    imports: &'static [&'static str],
}

impl FoldableNodeTypes {
    const fn for_language(language: Language) -> Self {
        match language {
            Language::Rust => Self {
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
            Language::Python => Self {
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
            Language::TypeScript | Language::JavaScript | Language::Tsx => Self {
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
            Language::Go => Self {
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
            Language::C | Language::Cpp => Self {
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
            Language::Json => Self {
                blocks: &["object", "array"],
                comments: &[],
                imports: &[],
            },
            Language::Yaml => Self {
                blocks: &[
                    "block_mapping",
                    "block_sequence",
                    "flow_mapping",
                    "flow_sequence",
                ],
                comments: &["comment"],
                imports: &[],
            },
            Language::Markdown => Self {
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
            Language::Css => Self {
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
            Language::Bash => Self {
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
        }
    }
}

/// Detects foldable regions in a parse tree.
///
/// The detector owns no parser and no tree: it holds one language's rules about
/// which node kinds are foldable, and reads a [`Tree`] someone else parsed. See
/// [`crate::SyntaxTree`] for the owner.
///
/// # Example
///
/// ```
/// use iridium_syntax::{FoldDetector, Language, SyntaxTree};
///
/// let mut tree = SyntaxTree::new(Language::Rust)?;
/// let source = "fn main() {\n    println!(\"Hello\");\n}";
/// let parsed = tree.parse(source).expect("valid Rust parses");
///
/// let detector = FoldDetector::new(Language::Rust);
/// let regions = detector.regions_in(parsed, source);
///
/// // Should find at least the function body as a foldable region
/// assert!(!regions.is_empty());
/// # Ok::<(), iridium_syntax::SyntaxError>(())
/// ```
#[derive(Debug)]
pub struct FoldDetector {
    language: Language,
    node_types: FoldableNodeTypes,
}

impl FoldDetector {
    /// Creates a fold detector for the given language.
    ///
    /// Cannot fail: the rules are a static table, and every language has one.
    #[must_use]
    pub const fn new(language: Language) -> Self {
        Self {
            language,
            node_types: FoldableNodeTypes::for_language(language),
        }
    }

    /// Returns the language this detector is configured for.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Returns the foldable regions in `tree`, sorted by start line.
    ///
    /// `source` is unused here — every fold this detector recognises is
    /// decided by node kind and position alone. It stays in the signature
    /// because the brace-matching detector that stands in when the `syntax`
    /// feature is off *does* need the text, and one signature means the call
    /// site does not need a `cfg`.
    ///
    /// Only regions spanning at least two lines survive, since a single-line
    /// construct cannot be meaningfully folded — the one exception being runs
    /// of consecutive single-line imports, which are merged into one region.
    #[must_use]
    pub fn regions_in(&self, tree: &Tree, _source: &str) -> Vec<FoldRegion> {
        let mut regions = Vec::new();
        self.collect_fold_regions(tree.root_node(), &mut regions);

        // Sort by start line (larger regions at same start line come first)
        regions.sort();

        // Remove duplicates (same start/end)
        regions.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);

        // Group consecutive imports
        merge_consecutive_imports(&mut regions);

        regions
    }

    /// Recursively collects fold regions from the parse tree.
    fn collect_fold_regions(&self, node: Node<'_>, regions: &mut Vec<FoldRegion>) {
        let node_type = node.kind();
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;

        // Only create fold regions for multi-line constructs
        if end_line > start_line {
            // Check if this is a foldable block
            if self.node_types.blocks.contains(&node_type) {
                regions.push(FoldRegion::new(start_line, end_line, FoldKind::Block));
            }
            // Check if this is a comment
            else if self.node_types.comments.contains(&node_type) {
                // For line comments, check if they form a block
                if node_type == "line_comment" || node_type == "comment" {
                    // Line comments are handled separately via grouping
                } else {
                    regions.push(FoldRegion::new(start_line, end_line, FoldKind::Comment));
                }
            }
            // Check if this is an import
            else if self.node_types.imports.contains(&node_type) {
                regions.push(FoldRegion::new(start_line, end_line, FoldKind::Import));
            }
        }

        // Single-line imports that might form a group
        if end_line == start_line && self.node_types.imports.contains(&node_type) {
            regions.push(FoldRegion::new(start_line, start_line, FoldKind::Import));
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_fold_regions(child, regions);
        }
    }
}

/// Merges consecutive single-line imports into foldable groups.
///
/// A lone import is dropped: folding one line hides nothing.
fn merge_consecutive_imports(regions: &mut Vec<FoldRegion>) {
    if regions.is_empty() {
        return;
    }

    // Find runs of consecutive single-line imports
    let mut merged = Vec::with_capacity(regions.len());
    let mut i = 0;

    while i < regions.len() {
        let region = &regions[i];

        if region.kind == FoldKind::Import && region.start_line == region.end_line {
            // Look for consecutive imports
            let mut end_idx = i;
            let mut last_line = region.start_line;

            while end_idx + 1 < regions.len() {
                let next = &regions[end_idx + 1];
                if next.kind == FoldKind::Import
                    && next.start_line == next.end_line
                    && next.start_line == last_line + 1
                {
                    last_line = next.start_line;
                    end_idx += 1;
                } else {
                    break;
                }
            }

            // If we found 2+ consecutive imports, merge them
            if end_idx > i {
                merged.push(FoldRegion::new(
                    region.start_line,
                    regions[end_idx].end_line,
                    FoldKind::Import,
                ));
                i = end_idx + 1;
            } else {
                // Single import, skip (not foldable alone)
                i += 1;
            }
        } else {
            // Non-import region, keep as-is
            merged.push(region.clone());
            i += 1;
        }
    }

    *regions = merged;
}

#[cfg(test)]
mod tests;
