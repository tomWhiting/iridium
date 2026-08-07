//! Syntax highlighting: parse tree in, coloured spans out.
//!
//! The rules come from the bundled query files (.scm) vendored from the Zed
//! editor; the tree comes from [`crate::SyntaxTree`], which this module reads
//! and never owns.
//!
//! Three concerns, one per file. [`capture`] collapses the queries' fine
//! capture vocabulary onto the categories a theme can paint, [`span`] is the
//! region those categories attach to, and [`highlighter`] runs the compiled
//! query and resolves its captures into spans.

mod capture;
mod highlighter;
mod span;

pub use capture::HighlightType;
pub use highlighter::Highlighter;
pub use span::HighlightSpan;

#[cfg(test)]
mod tests;
