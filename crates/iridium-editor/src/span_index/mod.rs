//! Span indexing for efficient viewport-based syntax highlighting queries.
//!
//! This module provides an interval tree data structure for O(log n + k) queries
//! of highlight spans overlapping a given byte range. This enables viewport-aware
//! rendering where only visible spans are processed each frame.
//!
//! # Architecture
//!
//! The [`SpanIndex`] wraps a [`rust_lapper::Lapper`] interval tree, which is
//! optimized for querying overlapping intervals. When highlights change, a new
//! index is built (O(n log n)), but per-frame queries are O(log n + k) where
//! k is the number of overlapping spans.
//!
//! # Example
//!
//! ```ignore
//! use iridium_editor::span_index::SpanIndex;
//! use iridium_syntax::{HighlightSpan, HighlightType};
//!
//! // Build index from parser output
//! let spans = vec![
//!     HighlightSpan::new(0, 3, HighlightType::Keyword),
//!     HighlightSpan::new(4, 10, HighlightType::Variable),
//! ];
//! let index = SpanIndex::new(spans);
//!
//! // Query for viewport (bytes 0-100)
//! let visible: Vec<_> = index.query(0, 100).collect();
//! ```

mod interval_tree;

pub use interval_tree::SpanIndex;

// Re-export the span types for convenience
#[cfg(feature = "syntax")]
pub use iridium_syntax::{HighlightSpan, HighlightType};
