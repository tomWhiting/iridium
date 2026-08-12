//! What the last painted frame put on the screen, and the hit test that reads
//! it back.
//!
//! ⭐ **Owned by the face, produced by the painter.** The record used to live
//! on [`OverlayPainter`](super::OverlayPainter) itself, which made it
//! unreachable from any test: the painter cannot be built without a GPU
//! device, so every question a pointer asks of the screen could only be
//! answered by looking at one. The painter now *returns* this and keeps
//! nothing, so the pointer's whole ladder is checkable on a machine with no
//! display attached — and there is exactly one owner of the answer rather than
//! a copy on each side.
//!
//! # Why the record carries the kind
//!
//! A press has to resolve to *a panel*, not to a boolean. The frame is the
//! only place that knows both which panels were composed and where each of
//! them landed, so it is the only place that can answer honestly when the
//! answer changed between the frame the user clicked on and now.

use super::content::PanelKind;
use super::geometry::PanelGeometry;
use crate::tab_strip::TabStripLayout;

/// One panel of a painted frame: which panel it was, where it landed, and how
/// many rows it drew.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintedPanel {
    /// Which panel this is.
    pub kind: PanelKind,
    /// Where it landed, or `None` for a panel the window could not hold.
    ///
    /// Kept as an entry rather than dropped so a panel that declined to draw
    /// is distinguishable from one that was never composed — the first
    /// swallows nothing and the second is not open at all.
    pub geometry: Option<PanelGeometry>,
    /// How many interior rows it composed.
    ///
    /// ⚠️ **Carried because a placement alone cannot say which row a pixel is
    /// on.** [`PanelGeometry::row_at`] needs the row count to tell the last row
    /// from the padding under it, and a caller that supplied its own count
    /// would be answering for the panel as it is *now* rather than as it was
    /// drawn — which is a row off every time a directory listing lands between
    /// the frame and the click.
    pub rows: usize,
}

/// What the pointer is on: a panel, and the row of it under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelHit {
    /// Which panel.
    pub kind: PanelKind,
    /// Where that panel landed.
    pub geometry: PanelGeometry,
    /// How many interior rows it drew.
    pub rows: usize,
    /// The interior row under the pointer, or `None` for the panel's padding.
    ///
    /// The padding is genuinely part of the panel — a press on it must not
    /// reach the document — and genuinely not a row, so it is one value with
    /// two honest halves rather than two answers to reconcile.
    pub row: Option<usize>,
}

/// Where everything the last painted frame drew landed.
///
/// The default is an empty frame: nothing painted, so nothing on screen for a
/// pointer to be on. That is the honest answer before the first frame and
/// after a frame that failed to compose, and it is what makes a press fall
/// through to the document rather than to a panel that is no longer there.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaintedFrame {
    /// Each composed panel, in paint order — later panels drew on top.
    pub panels: Vec<PaintedPanel>,
    /// The tab strip, or `None` for a frame that drew none.
    pub tabs: Option<TabStripLayout>,
}

impl PaintedFrame {
    /// What a physical pixel is on, topmost first.
    ///
    /// ⚠️ **The reverse iteration is the whole correctness of this
    /// function.** Panels are painted in order and later ones cover earlier
    /// ones, so a pixel inside two of them belongs to the one drawn last —
    /// which is exactly the one the user can see and therefore the only one
    /// they can have meant. Reading forwards would hand a click on the context
    /// menu to the sidebar it was opened over.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32) -> Option<PanelHit> {
        self.panels.iter().rev().find_map(|panel| {
            let geometry = panel.geometry?;
            geometry.contains(x, y).then(|| PanelHit {
                kind: panel.kind,
                geometry,
                rows: panel.rows,
                row: geometry.row_at(x, y, panel.rows),
            })
        })
    }

    /// Where a named panel landed on this frame, or `None` when it was not
    /// composed or the window could not hold it.
    ///
    /// Reverse order for the same reason as [`Self::hit`]: nothing composes
    /// the same panel twice, but the topmost is the right answer if anything
    /// ever does.
    #[must_use]
    pub fn geometry_of(&self, kind: PanelKind) -> Option<PanelGeometry> {
        self.panels
            .iter()
            .rev()
            .find(|panel| panel.kind == kind)
            .and_then(|panel| panel.geometry)
    }

    /// Whether the pointer is on any painted panel.
    #[must_use]
    pub fn is_on_a_panel(&self, x: f32, y: f32) -> bool {
        self.hit(x, y).is_some()
    }

    /// Whether the pointer is on the painted tab strip.
    #[must_use]
    pub fn is_on_the_tab_strip(&self, x: f32, y: f32) -> bool {
        self.tabs
            .as_ref()
            .is_some_and(|layout| layout.contains(x, y))
    }
}
