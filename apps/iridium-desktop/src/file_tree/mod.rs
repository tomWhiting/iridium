//! The file explorer panel, under the path this face has always used.
//!
//! ⭐ **The panel itself is [`iridium_panel::explorer`] now, all of it.** What
//! is left here is the one thing a shared panel structurally cannot say: where
//! on a window it goes. Everything else — the tree, the keys, the oil buffer,
//! the filter, the rename pipeline, the composition of every row — is one
//! implementation that the terminal face compiles from the same source, which
//! is the whole point of #112d.
//!
//! # Why this module still exists at all
//!
//! Two reasons, and both are about the seam rather than about compatibility.
//!
//! **An anchor is a statement about a window.** [`content`] below is the
//! conversion: it takes the [`PanelBody`](iridium_panel::PanelBody) the shared
//! panel composed — rows, the width they were laid out against, and where the
//! caret landed — and wraps it in this face's [`PanelContent`], which adds the
//! anchor and leaves room for the hover the painter fills in afterwards. That
//! is the only thing the desktop adds, and it is exactly one struct literal
//! long, which is the measure of how clean the seam turned out to be.
//!
//! **A free function rather than a method**, because Rust's orphan rule will
//! not let this crate write `impl FileExplorer` for a type another crate
//! defines. That constraint is worth reading as a feature: it is what stops a
//! face quietly growing panel behaviour that the other face would never see.
//! An extension trait could have restored the method syntax and would have
//! bought nothing but the dot.
//!
//! # Where to look when the panel misbehaves
//!
//! Nothing here. A wrong row, a key that does the wrong thing, a rename that
//! refuses — all of it is in [`iridium_panel::explorer`], and a defect found
//! in this face is a defect the terminal face has too.

use iridium_editor::theme::Theme;

use crate::overlay::{PanelAnchor, PanelContent, PanelFit};

pub use iridium_panel::explorer::{ExplorerOutcome, FileExplorer};

/// Composes `explorer` for painting at `anchor`.
///
/// `&mut FileExplorer` because composition is where the scroll window follows
/// the selection and the page size is learned — see
/// [`FileExplorer::body`](iridium_panel::explorer::FileExplorer::body), which
/// is where all of that happens.
///
/// `hovered` is left `None` here on purpose. The painter's `apply_hover` sets
/// it after every panel has composed, for the same reason the anchor is passed
/// in rather than asked for: a builder has no idea where a pointer is, and one
/// that had to be told would carry a mouse in its signature into a face that
/// may not have one.
pub fn content(
    explorer: &mut FileExplorer,
    theme: &Theme,
    fit: PanelFit,
    anchor: PanelAnchor,
) -> PanelContent {
    let body = explorer.body(theme, fit);
    PanelContent {
        anchor,
        content_columns: body.content_columns,
        rows: body.rows,
        caret: body.caret,
        hovered: None,
    }
}
