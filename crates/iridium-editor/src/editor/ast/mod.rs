//! Syntax-aware navigation, and the tree it reads.
//!
//! The editor owns one parse tree for the document. Folds, structural walks,
//! named text objects, expansion history and selection descriptions read this
//! shared state. The parser-free build keeps the same command surface and
//! reports no structural selection or syntax label.

#[cfg(all(feature = "syntax", not(target_arch = "wasm32")))]
mod background;
#[cfg(not(all(feature = "syntax", not(target_arch = "wasm32"))))]
#[path = "background_stub.rs"]
mod background;
mod delta;
mod expand;
mod navigation;
mod scope;
mod state;
#[cfg(feature = "syntax")]
mod textobject;
#[cfg(feature = "syntax")]
mod walk;

pub use background::LandedFolds;
pub use delta::SyntaxDelta;
pub use expand::ExpandStack;
pub use scope::CaretScopes;
pub(super) use state::PreparedSyntaxEdit;
pub use state::{SyntaxState, SyntaxWake};
