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
//!
//! # Two ways in, and why there are two
//!
//! [`FoldDetector::regions_in`] walks a whole tree. That is O(nodes), which on a
//! 100,000-line JSON file is millions of nodes, and paying it on every keystroke
//! measured at over half a second per typed character. It is still the right
//! answer whenever the tree is new: a file load, a language change, a parse that
//! could not reuse the old tree.
//!
//! [`FoldCache`] is the answer for a document being *edited*. It retains the
//! regions it last produced and, on each edit, recomputes only the part of the
//! tree tree-sitter reports as changed, shifting the rest. Its output is defined
//! to be identical to [`FoldDetector::regions_in`] on the same tree — not "close
//! enough", identical — and the equivalence tests in this module are what hold
//! it to that.

mod cache;
mod detector;
mod language;
mod region;

pub use cache::FoldCache;
pub use detector::FoldDetector;
pub use region::{FoldKind, FoldRegion};

// `folding` is a private module, so these stay inside the crate; the
// re-exports above are the whole of this module's public surface.
pub use language::FoldableNodeTypes;
pub use region::{TrackedRegion, publish};

#[cfg(test)]
mod tests;
