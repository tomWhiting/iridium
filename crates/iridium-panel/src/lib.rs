//! The vocabulary every Iridium panel is composed in, and the arithmetic that
//! assembles one row of it.
//!
//! # Why this is a crate rather than a module in a face
//!
//! ⭐ **A panel's *content* is not a property of the face that paints it.** The
//! command palette, the undo tree, the search bar and the file explorer all
//! compose the same thing — rows of coloured character runs, laid out against a
//! character budget, with a caret in the one being typed into. What differs is
//! the painting: the GPU faces place glyphs at `char × char_width` against a
//! signed-distance chrome, and the terminal face writes graphemes into a cell
//! buffer with its own width model.
//!
//! That split was already true when all of this lived in the desktop face; what
//! made it a crate is the file explorer, which has to exist in the terminal
//! face too. Writing its oil buffer, its filter and its rename planning a
//! second time would mean the two editors could disagree about what a rename
//! does — and the disagreement would be found by the person using them, not by
//! a test, because nothing compiles both faces' copies at once.
//!
//! # What is deliberately **not** here
//!
//! - **Where a panel lands.** An anchor is a statement about a window, and a
//!   window is a face's business. The desktop's `PanelAnchor` and the terminal
//!   face's `FloatingBox` are the same idea in units that cannot be reconciled,
//!   and reconciling them would produce a type neither face wants.
//! - **How wide a character is.** [`PanelFit`] counts characters and rows. Turning
//!   those into pixels or cells is the face's, and it is the one conversion each
//!   face already owns.
//! - **Any renderer, device, or window.** Nothing in this crate can fail to
//!   compile for want of a GPU.

pub mod line;
pub mod row;

pub use line::{LineBuilder, highlighted_spans, match_color, skip_chars};
pub use row::{
    EXPLORER_MAX_VISIBLE_ROWS, PANEL_MAX_VISIBLE_ROWS, PanelCaret, PanelFit, PanelRow, Span,
};
