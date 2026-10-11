//! Compiles the pinned generated parsers that have no published grammar crate.

use std::path::Path;

/// Grammars vendored as C, by directory name under `grammars/`.
///
/// Hand-listed rather than scanned, for the reason the language list itself is:
/// a grammar appearing or vanishing must be a visible edit, not a consequence
/// of what happens to be on disk.
const VENDORED: &[&str] = &["awl", "just", "typst", "wgsl"];

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
