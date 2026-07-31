//! Syntax-aware navigation, and the tree it reads.
//!
//! The editor owns one parse tree for the document it is editing; everything
//! structural — folds today, node selection and text objects next — reads that
//! one tree rather than parsing its own.
//!
//! [`SyntaxState`] is the whole of this module for now. The verbs that navigate
//! by node arrive on top of it.

mod expand;
mod state;
#[cfg(feature = "syntax")]
mod textobject;
#[cfg(feature = "syntax")]
mod walk;

pub use expand::ExpandStack;
pub use state::SyntaxState;
