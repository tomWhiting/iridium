//! Text buffer management using rope data structure.
//!
//! This module provides efficient storage and manipulation of text content
//! using the ropey crate's rope data structure, which offers O(log n)
//! performance for most operations.

mod rope;

pub use rope::Buffer;
