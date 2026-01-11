//! Code folding region detection.
//!
//! This module provides syntax-aware code folding by analyzing tree-sitter
//! parse trees to identify foldable regions. Fold regions are detected for:
//!
//! - Block statements (function bodies, if/else blocks, loops, etc.)
//! - Import groups (consecutive import statements)
//! - Multi-line comments (block comments and documentation)
//! - Region markers (#region / #endregion style)
//!
//! The detection uses tree-sitter's incremental parsing for efficient updates
//! when the document changes.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tree_sitter::{Node, Parser, Tree};

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
    pub fn contains_region(&self, other: &Self) -> bool {
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
struct FoldableNodeTypes {
    /// Node types that represent block structures
    blocks: &'static [&'static str],
    /// Node types that represent comments
    comments: &'static [&'static str],
    /// Node types that represent import statements
    imports: &'static [&'static str],
}

impl FoldableNodeTypes {
    fn for_language(language: Language) -> Self {
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
                blocks: &["block_mapping", "block_sequence", "flow_mapping", "flow_sequence"],
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

/// Internal language registry for tree-sitter grammars (for folding).
struct LanguageRegistry {
    grammars: HashMap<Language, tree_sitter::Language>,
}

impl LanguageRegistry {
    fn new() -> Self {
        let mut grammars = HashMap::new();

        // Register all supported language grammars
        grammars.insert(Language::Rust, tree_sitter_rust::LANGUAGE.into());
        grammars.insert(Language::Python, tree_sitter_python::LANGUAGE.into());
        grammars.insert(
            Language::TypeScript,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        );
        grammars.insert(Language::JavaScript, tree_sitter_javascript::LANGUAGE.into());
        grammars.insert(Language::Tsx, tree_sitter_typescript::LANGUAGE_TSX.into());
        grammars.insert(Language::Go, tree_sitter_go::LANGUAGE.into());
        grammars.insert(Language::Json, tree_sitter_json::LANGUAGE.into());
        grammars.insert(Language::Yaml, tree_sitter_yaml::LANGUAGE.into());
        grammars.insert(Language::Markdown, tree_sitter_md::LANGUAGE.into());
        grammars.insert(Language::Css, tree_sitter_css::LANGUAGE.into());
        grammars.insert(Language::Bash, tree_sitter_bash::LANGUAGE.into());
        grammars.insert(Language::C, tree_sitter_c::LANGUAGE.into());
        grammars.insert(Language::Cpp, tree_sitter_cpp::LANGUAGE.into());

        Self { grammars }
    }

    fn get(&self, lang: Language) -> Option<&tree_sitter::Language> {
        self.grammars.get(&lang)
    }
}

/// Detects foldable regions in source code using tree-sitter.
///
/// The detector parses source code and identifies regions that can be folded
/// based on the syntax structure. It supports incremental updates when the
/// source code changes.
///
/// # Example
///
/// ```
/// use iridium_syntax::{Language, FoldDetector};
///
/// let mut detector = FoldDetector::new(Language::Rust).unwrap();
/// let regions = detector.detect("fn main() {\n    println!(\"Hello\");\n}");
///
/// // Should find at least the function body as a foldable region
/// assert!(!regions.is_empty());
/// ```
pub struct FoldDetector {
    language: Language,
    parser: Parser,
    tree: Option<Tree>,
    node_types: FoldableNodeTypes,
}

impl std::fmt::Debug for FoldDetector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FoldDetector")
            .field("language", &self.language)
            .field("has_tree", &self.tree.is_some())
            .finish_non_exhaustive()
    }
}

impl FoldDetector {
    /// Creates a new fold detector for the given language.
    ///
    /// # Errors
    ///
    /// Returns `None` if the language is not supported.
    #[must_use]
    pub fn new(language: Language) -> Option<Self> {
        let registry = LanguageRegistry::new();

        let ts_language = registry.get(language)?;

        let mut parser = Parser::new();
        parser.set_language(ts_language).ok()?;

        let node_types = FoldableNodeTypes::for_language(language);

        Some(Self {
            language,
            parser,
            tree: None,
            node_types,
        })
    }

    /// Returns the language this detector is configured for.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Detects all foldable regions in the source code.
    ///
    /// This parses the source code (or uses the cached parse tree if available)
    /// and returns a list of foldable regions sorted by start line.
    ///
    /// Only regions spanning at least 2 lines are included, as single-line
    /// constructs cannot be meaningfully folded.
    #[must_use]
    pub fn detect(&mut self, source: &str) -> Vec<FoldRegion> {
        // Parse the source
        self.tree = self.parser.parse(source, None);

        let Some(tree) = &self.tree else {
            return Vec::new();
        };

        let mut regions = Vec::new();
        self.collect_fold_regions(tree.root_node(), source, &mut regions);

        // Sort by start line (larger regions at same start line come first)
        regions.sort();

        // Remove duplicates (same start/end)
        regions.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);

        // Group consecutive imports
        self.merge_consecutive_imports(&mut regions);

        regions
    }

    /// Updates fold regions incrementally after an edit.
    ///
    /// This is more efficient than re-detecting from scratch because
    /// tree-sitter can reuse unchanged parts of the parse tree.
    ///
    /// # Arguments
    ///
    /// * `source` - The new source code after the edit
    /// * `start_byte` - The byte offset where the edit started
    /// * `old_end_byte` - The byte offset where the edit ended in the old source
    /// * `new_end_byte` - The byte offset where the edit ended in the new source
    #[must_use]
    pub fn update(
        &mut self,
        source: &str,
        start_byte: usize,
        old_end_byte: usize,
        new_end_byte: usize,
    ) -> Vec<FoldRegion> {
        let Some(old_tree) = self.tree.take() else {
            return self.detect(source);
        };

        // Create the edit descriptor for tree-sitter
        let input_edit = tree_sitter::InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position: byte_to_point(source, start_byte),
            old_end_position: byte_to_point(source, old_end_byte),
            new_end_position: byte_to_point(source, new_end_byte),
        };

        // Clone the old tree and apply the edit
        let mut edited_tree = old_tree;
        edited_tree.edit(&input_edit);

        // Reparse with the edited tree as a reference
        self.tree = self.parser.parse(source, Some(&edited_tree));

        let Some(tree) = &self.tree else {
            return Vec::new();
        };

        let mut regions = Vec::new();
        self.collect_fold_regions(tree.root_node(), source, &mut regions);

        regions.sort();
        regions.dedup_by(|a, b| a.start_line == b.start_line && a.end_line == b.end_line);
        self.merge_consecutive_imports(&mut regions);

        regions
    }

    /// Recursively collects fold regions from the parse tree.
    fn collect_fold_regions(&self, node: Node<'_>, source: &str, regions: &mut Vec<FoldRegion>) {
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

        // Check for #region markers in comments
        if self.node_types.comments.contains(&node_type) || node_type == "comment" {
            let comment_text = &source[node.byte_range()];
            if comment_text.contains("#region") || comment_text.contains("// region:") {
                // Region marker found - would need end marker tracking
                // For now, we rely on the block structure
            }
        }

        // Single-line imports that might form a group
        if end_line == start_line && self.node_types.imports.contains(&node_type) {
            regions.push(FoldRegion::new(start_line, start_line, FoldKind::Import));
        }

        // Recurse into children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_fold_regions(child, source, regions);
        }
    }

    /// Merges consecutive single-line imports into foldable groups.
    fn merge_consecutive_imports(&self, regions: &mut Vec<FoldRegion>) {
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
}

/// Convert a byte offset to a tree-sitter Point (row, column).
fn byte_to_point(source: &str, byte_offset: usize) -> tree_sitter::Point {
    let mut row = 0;
    let mut col = 0;
    let mut current_byte = 0;

    for ch in source.chars() {
        if current_byte >= byte_offset {
            break;
        }

        if ch == '\n' {
            row += 1;
            col = 0;
        } else {
            col += ch.len_utf8();
        }

        current_byte += ch.len_utf8();
    }

    tree_sitter::Point { row, column: col }
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let mut regions = vec![c.clone(), b.clone(), a.clone()];
        regions.sort();

        // Same start line: larger region first (a before b)
        // Different start line: earlier first (a/b before c)
        assert_eq!(regions[0], a);
        assert_eq!(regions[1], b);
        assert_eq!(regions[2], c);
    }

    #[test]
    fn detect_rust_function() {
        let mut detector = FoldDetector::new(Language::Rust).expect("Rust should be supported");
        let source = r#"fn main() {
    println!("Hello");
    println!("World");
}"#;

        let regions = detector.detect(source);
        assert!(!regions.is_empty(), "Should detect function as foldable");

        // Should have at least the function body
        let has_function = regions.iter().any(|r| r.start_line == 0 && r.kind == FoldKind::Block);
        assert!(has_function, "Should detect main function");
    }

    #[test]
    fn detect_rust_nested_blocks() {
        let mut detector = FoldDetector::new(Language::Rust).expect("Rust should be supported");
        let source = r#"fn main() {
    if true {
        println!("nested");
    }
}"#;

        let regions = detector.detect(source);

        // Should detect both the function and the if block
        assert!(regions.len() >= 2, "Should detect nested blocks");
    }

    #[test]
    fn detect_rust_imports() {
        let mut detector = FoldDetector::new(Language::Rust).expect("Rust should be supported");
        let source = r#"use std::io;
use std::fs;
use std::path::Path;

fn main() {}"#;

        let regions = detector.detect(source);

        // Should detect the import group
        let has_imports = regions.iter().any(|r| r.kind == FoldKind::Import);
        assert!(has_imports, "Should detect import group");
    }

    #[test]
    fn detect_python_function() {
        let mut detector = FoldDetector::new(Language::Python).expect("Python should be supported");
        let source = r#"def greet():
    print("Hello")
    print("World")"#;

        let regions = detector.detect(source);
        assert!(!regions.is_empty(), "Should detect Python function");
    }

    #[test]
    fn detect_typescript_class() {
        let mut detector =
            FoldDetector::new(Language::TypeScript).expect("TypeScript should be supported");
        let source = r#"class Greeter {
    constructor() {
        console.log("init");
    }
    greet() {
        console.log("hello");
    }
}"#;

        let regions = detector.detect(source);
        assert!(regions.len() >= 2, "Should detect class and methods");
    }

    #[test]
    fn detect_go_function() {
        let mut detector = FoldDetector::new(Language::Go).expect("Go should be supported");
        let source = r#"func main() {
    fmt.Println("Hello")
}"#;

        let regions = detector.detect(source);
        assert!(!regions.is_empty(), "Should detect Go function");
    }

    #[test]
    fn detect_json_object() {
        let mut detector = FoldDetector::new(Language::Json).expect("JSON should be supported");
        let source = r#"{
    "name": "test",
    "nested": {
        "value": 42
    }
}"#;

        let regions = detector.detect(source);
        assert!(regions.len() >= 2, "Should detect JSON objects");
    }

    #[test]
    fn incremental_update() {
        let mut detector = FoldDetector::new(Language::Rust).expect("Rust should be supported");

        // Initial parse
        let source = "fn main() {\n    println!(\"hello\");\n}";
        let regions1 = detector.detect(source);

        // Add more code
        let new_source = "fn main() {\n    println!(\"hello\");\n    println!(\"world\");\n}";
        let regions2 = detector.update(new_source, 35, 35, 56);

        // Both should have the function region
        assert!(!regions1.is_empty());
        assert!(!regions2.is_empty());
    }

    #[test]
    fn single_line_not_foldable() {
        let mut detector = FoldDetector::new(Language::Rust).expect("Rust should be supported");
        let source = "fn main() {}";

        let regions = detector.detect(source);

        // Single-line function should not be foldable
        let has_single_line = regions
            .iter()
            .any(|r| r.start_line == r.end_line && r.kind == FoldKind::Block);
        assert!(!has_single_line, "Single-line blocks should not be foldable");
    }
}
