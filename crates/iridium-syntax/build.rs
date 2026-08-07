//! Compiles the grammars vendored as C source under `grammars/`.
//!
//! Most of Iridium's grammars arrive as published crates — `tree-sitter-rust`
//! and its dozen siblings — which carry their own build scripts and need
//! nothing here. This exists for grammars that have no published crate.
//!
//! AWL is the first and currently the only one. Its generated `parser.c` is
//! vendored from `github.com/ablative-io/aion` at rev `8d4d63e`, and there is
//! no crate to depend on instead.
//!
//! # Why vendoring the C is the right shape rather than a workaround
//!
//! tree-sitter's crate declares `links = "tree-sitter"`, which forbids two
//! versions of it in one dependency graph. A grammar pulled from elsewhere
//! that happened to want a different tree-sitter would therefore fail to
//! resolve, and no feature flag can rescue that. A vendored `parser.c` links
//! against *this* build's tree-sitter runtime by construction, so the question
//! cannot arise.
//!
//! It also means the grammar is pinned to a revision that is reviewed and in
//! the tree, rather than to whatever a registry serves.
//!
//! # ABI
//!
//! `parser.c` declares `LANGUAGE_VERSION 15`, which this workspace's
//! tree-sitter 0.26 accepts. A grammar regenerated against an incompatible ABI
//! fails at `Parser::set_language` rather than here, and the test module in
//! `grammar.rs` sets every linked language on a parser precisely so that shows
//! up as a failing test rather than as a language that silently stops
//! highlighting.

use std::path::Path;

/// Grammars vendored as C, by directory name under `grammars/`.
///
/// Hand-listed rather than scanned, for the reason the language list itself is:
/// a grammar appearing or vanishing must be a visible edit, not a consequence
/// of what happens to be on disk.
const VENDORED: &[&str] = &["awl"];

fn main() {
    for grammar in VENDORED {
        compile(grammar);
    }
}

/// Compiles one vendored grammar into a static library named after it.
///
/// The two warnings silenced are endemic to tree-sitter's generated parsers —
/// they are machine-written state machines, not code anyone is going to tidy —
/// and leaving them on would bury a real warning from a future grammar in
/// thousands of lines of noise.
fn compile(grammar: &str) {
    let directory = Path::new("grammars").join(grammar);
    let parser = directory.join("parser.c");

    println!("cargo:rerun-if-changed={}", parser.display());

    let mut build = cc::Build::new();
    build
        .std("c11")
        .include(&directory)
        .flag_if_supported("-Wno-unused-parameter")
        .flag_if_supported("-Wno-unused-but-set-variable")
        .file(&parser);

    // A grammar with an external scanner has one; AWL does not. Checked rather
    // than assumed, so the next vendored grammar that does carry one needs no
    // change here.
    let scanner = directory.join("scanner.c");
    if scanner.exists() {
        println!("cargo:rerun-if-changed={}", scanner.display());
        build.file(&scanner);
    }

    build.compile(grammar);
}
