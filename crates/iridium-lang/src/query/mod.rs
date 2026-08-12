//! The vendored tree-sitter query sources, and the kinds they come in.
//!
//! This is the *text* of the `.scm` files and nothing more. Compiling a query
//! needs a grammar, a grammar needs tree-sitter, and tree-sitter is exactly
//! what this crate refuses to depend on — so compilation lives in
//! `iridium-syntax`, which reads its source from here.
//!
//! # Why the bytes live in this crate rather than beside the compiler
//!
//! One directory, one owner. `languages/` is a vendored upstream tree carrying
//! both the queries and the `config.toml` manifests that describe each
//! language, and the manifests are needed in builds that have no parser —
//! comment toggling, indent rules and auto-pairs all key off them. Splitting
//! the directory so the manifests could be reached from the parser-free build
//! would leave one upstream tree living in two crates, and the next refresh
//! would have to know that.
//!
//! Carrying the query text in an always-linked crate costs nothing in builds
//! that never read it: measured against the release profile this workspace
//! ships (`lto = "thin"`, `codegen-units = 1`, `strip = true`), 146 vendored
//! `.scm` files reachable only through an uncalled function produced a
//! byte-identical `wasm32-unknown-unknown` artifact. The linker discards the
//! function and its string data together.

mod captures;
mod embedded;
mod kind;

pub use captures::capture_names;
pub use embedded::{source, sources};
pub use kind::QueryKind;

#[cfg(test)]
mod tests;
