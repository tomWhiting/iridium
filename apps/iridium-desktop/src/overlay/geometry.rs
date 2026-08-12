//! Where a composed panel lands on the window, as a pure function of the
//! window, the grid and the anchor.
//!
//! Tested without a GPU, which is the whole reason it is separate from the
//! painter: placement is arithmetic, and arithmetic that can only be checked
//! by looking at a screen is arithmetic nobody checks.

use iridium_panel::{PanelCaret, PanelFit};

use super::content::PanelAnchor;
use super::metrics::{
    CARET_WIDTH, HAIRLINE, PAD_X, PAD_Y, PANEL_MAX_COLUMNS, PANEL_MIN_COLUMNS, PANEL_RADIUS,
    ROW_INSET, ROW_RADIUS, SIDEBAR_COLUMNS, SIDEBAR_MAX_FRACTION, TOP_ANCHOR_FRACTION,
};
use crate::units::{index_to_f32, pixel_to_index, pixels_to_cells};

/// The text grid the chrome is placed against: the renderer's measured cell
/// plus the window scale factor that converts the chrome's logical
/// measurements to physical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridMetrics {
    /// One character cell's width, in physical pixels.
    pub char_width: f32,
    /// The row pitch — the text renderer's line height, in physical pixels.
    pub line_height: f32,
    /// The window scale factor.
    pub scale: f32,
}

impl GridMetrics {
    /// Whether the grid can place anything at all: a font that has not
    /// measured yet, or a scale no display can report, places nothing.
    ///
    /// Reachable from [`crate::tab_strip`], which places against the same
    /// grid and must decline on the same inputs.
    pub(crate) fn is_degenerate(&self) -> bool {
        self.char_width <= 0.0
            || self.line_height <= 0.0
            || self.scale <= 0.0
            || self.scale.is_nan()
    }
}

/// An axis-aligned chrome rectangle with its corner radius, in physical
/// pixels — [`PanelGeometry`]'s vocabulary, colourless by design so the
/// geometry stays a pure placement statement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelRect {
    /// The left edge.
    pub x: f32,
    /// The top edge.
    pub y: f32,
    /// The width.
    pub width: f32,
    /// The height.
    pub height: f32,
    /// The corner radius; zero is a sharp rectangle.
    pub radius: f32,
}

impl PanelRect {
    /// Whether a physical pixel lies on the rectangle.
    ///
    /// Half-open on both axes — the right and bottom edges belong to
    /// whatever is next — so two rectangles that share an edge never both
    /// claim the same pixel. The corners are treated as square for the same
    /// reason [`PanelGeometry::contains`] treats them so: half an arc of
    /// slack at four corners cannot change which control a press meant.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Where a composed panel lands on the window, in physical pixels.
///
/// Computed by [`panel_geometry`] as a pure function of the window, the text
/// grid and the panel's grid dimensions, so placement is testable without a
/// GPU; the painter only turns it into instances.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelGeometry {
    /// The panel's exterior rectangle and corner radius.
    pub exterior: PanelRect,
    /// The content area's left edge — the exterior plus the horizontal
    /// padding; column zero of every row starts here.
    pub content_x: f32,
    /// The content area's top edge — the exterior plus the vertical padding;
    /// row zero starts here.
    pub content_y: f32,
    /// The row pitch, the text renderer's line height.
    pub line_height: f32,
    /// One character cell's width.
    pub char_width: f32,
    /// The window scale factor the logical chrome values were multiplied by.
    pub scale: f32,
}

impl PanelGeometry {
    /// The selected-row band behind interior row `index`: nearly the panel's
    /// full width — inset [`ROW_INSET`] logical pixels each side so its own
    /// arcs never overhang the panel's — with the chip radius, so no sharp
    /// corner exists anywhere in the chrome.
    #[must_use]
    pub fn selected_row_rect(&self, index: usize) -> PanelRect {
        let inset = ROW_INSET * self.scale;
        let width = 2.0_f32.mul_add(-inset, self.exterior.width);
        let radius = (ROW_RADIUS * self.scale)
            .min(width / 2.0)
            .min(self.line_height / 2.0);
        PanelRect {
            x: self.exterior.x + inset,
            y: index_to_f32(index).mul_add(self.line_height, self.content_y),
            width,
            height: self.line_height,
            radius,
        }
    }

    /// The hairline rule drawn for a separator row at interior `index`:
    /// centred in the row's height, inset like the selected-row band so it
    /// never crosses the panel's own arcs.
    #[must_use]
    pub fn separator_rect(&self, index: usize) -> PanelRect {
        let inset = ROW_INSET * self.scale;
        let row = self.selected_row_rect(index);
        PanelRect {
            x: self.exterior.x + inset,
            y: ((self.line_height - HAIRLINE) / 2.0) + row.y,
            width: 2.0_f32.mul_add(-inset, self.exterior.width),
            height: HAIRLINE,
            radius: 0.0,
        }
    }

    /// Whether a physical pixel lies on the panel.
    ///
    /// The exterior rectangle's corners are treated as square: half an arc of
    /// slack at four corners cannot change which side of a menu edge a click
    /// is on, and a click that lands in a rounded corner belongs to the panel
    /// it is visually part of.
    #[must_use]
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.exterior.x
            && x < self.exterior.x + self.exterior.width
            && y >= self.exterior.y
            && y < self.exterior.y + self.exterior.height
    }

    /// The interior row a physical pixel lands on, or `None` for a pixel off
    /// the panel or inside its padding.
    ///
    /// `rows` is how many interior rows the panel was composed with; a pixel
    /// past the last of them is padding, not the last row.
    #[must_use]
    pub fn row_at(&self, x: f32, y: f32, rows: usize) -> Option<usize> {
        if !self.contains(x, y) || self.line_height <= 0.0 || y < self.content_y {
            return None;
        }
        let row = pixel_to_index((y - self.content_y) / self.line_height);
        (row < rows).then_some(row)
    }

    /// The caret bar for a field caret at interior `(row, column)`.
    #[must_use]
    pub fn caret_rect(&self, caret: PanelCaret) -> PanelRect {
        PanelRect {
            x: index_to_f32(caret.column).mul_add(self.char_width, self.content_x),
            y: index_to_f32(caret.row).mul_add(self.line_height, self.content_y),
            width: CARET_WIDTH,
            height: self.line_height,
            radius: 0.0,
        }
    }
}

/// Places a panel of `rows × content_columns` on a `window_width ×
/// window_height` window, or `None` when the window cannot honestly hold it.
///
/// All arguments and results are physical pixels; the metrics' scale
/// converts the chrome's logical measurements. `reserve_bottom` is the
/// strip's height while one is up, so a bottom-anchored panel stacks above
/// it. The corner radius is clamped to half the panel's short side, so even
/// a two-row search panel keeps honest arcs.
#[must_use]
pub fn panel_geometry(
    window_width: f32,
    window_height: f32,
    metrics: GridMetrics,
    anchor: PanelAnchor,
    rows: usize,
    content_columns: usize,
    reserve_bottom: f32,
) -> Option<PanelGeometry> {
    if rows == 0 || metrics.is_degenerate() {
        return None;
    }
    let pad_x = PAD_X * metrics.scale;
    let pad_y = PAD_Y * metrics.scale;
    let width = index_to_f32(content_columns).mul_add(metrics.char_width, 2.0 * pad_x);
    // A sidebar is as tall as its band, not as its rows — and never shorter
    // than them, so a band mis-measured smaller than the content still draws
    // every row it was handed rather than clipping the last few.
    let interior = match anchor {
        PanelAnchor::Left { interior_rows, .. } => rows.max(interior_rows),
        PanelAnchor::Top | PanelAnchor::Bottom | PanelAnchor::Point { .. } => rows,
    };
    let height = index_to_f32(interior).mul_add(metrics.line_height, 2.0 * pad_y);
    if width > window_width || height > window_height {
        return None;
    }

    let centred = ((window_width - width) / 2.0).max(0.0);
    let (x, y) = match anchor {
        PanelAnchor::Left { top, .. } => (
            0.0,
            finite_or_zero(top).clamp(0.0, (window_height - height).max(0.0)),
        ),
        PanelAnchor::Top => (
            centred,
            (TOP_ANCHOR_FRACTION * window_height)
                .min(window_height - height)
                .max(0.0),
        ),
        PanelAnchor::Bottom => (
            centred,
            (window_height - reserve_bottom.max(0.0) - height).max(0.0),
        ),
        PanelAnchor::Point { x, y } => {
            anchored_origin((x, y), (width, height), (window_width, window_height))
        },
    };
    let radius = (PANEL_RADIUS * metrics.scale)
        .min(width / 2.0)
        .min(height / 2.0);

    Some(PanelGeometry {
        exterior: PanelRect {
            x,
            y,
            width,
            height,
            radius,
        },
        content_x: x + pad_x,
        content_y: y + pad_y,
        line_height: metrics.line_height,
        char_width: metrics.char_width,
        scale: metrics.scale,
    })
}

/// Where a point-anchored panel's top-left corner lands: at the point where
/// the window allows it, clamped inside the window where it does not.
///
/// All values are physical pixels, and the caller has already established
/// that `size` fits inside `window`. Horizontally the panel slides left until
/// its right edge is inside the window; vertically it **flips above** the
/// point when hanging below it would clip the bottom edge, which is what a
/// menu opened near the bottom of a screen does everywhere, and only slides
/// when there is no room either way. A non-finite anchor — which no pointer
/// can report, [`crate::units::pixel_from_f64`] having clamped it — lands at
/// the window's origin rather than off it.
fn anchored_origin(point: (f32, f32), size: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let (width, height) = size;
    let (window_width, window_height) = window;
    let anchor_x = finite_or_zero(point.0);
    let anchor_y = finite_or_zero(point.1);

    let x = anchor_x.min(window_width - width).max(0.0);
    let y = if anchor_y + height <= window_height {
        anchor_y
    } else if anchor_y >= height {
        // No room below: the panel's bottom edge takes the anchor instead.
        anchor_y - height
    } else {
        window_height - height
    };
    (x, y.max(0.0))
}

/// `value` when a display could report it, and zero when it could not.
const fn finite_or_zero(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// What a `window_width × window_height` window can honestly show of a
/// panel, or `None` when it cannot show one at all — the terminal face's
/// `FloatingBox` rule, restated on the pixel grid. The pure half of
/// [`OverlayPainter::panel_fit`], separated so the fit-to-geometry round
/// trip is testable without a GPU.
pub(super) fn fit_for(
    window_width: f32,
    window_height: f32,
    metrics: GridMetrics,
) -> Option<PanelFit> {
    if metrics.is_degenerate() {
        return None;
    }
    let pad_x = PAD_X * metrics.scale;
    let pad_y = PAD_Y * metrics.scale;
    let exterior = window_width.min(index_to_f32(PANEL_MAX_COLUMNS) * metrics.char_width);
    let content_columns = pixels_to_cells(2.0_f32.mul_add(-pad_x, exterior), metrics.char_width);
    if content_columns + 4 < PANEL_MIN_COLUMNS {
        return None;
    }
    let vertical = 2.0_f32.mul_add(
        -pad_y,
        TOP_ANCHOR_FRACTION.mul_add(-window_height, window_height),
    );
    let max_interior_rows = pixels_to_cells(vertical, metrics.line_height);
    if max_interior_rows == 0 {
        return None;
    }
    Some(PanelFit::popover(content_columns, max_interior_rows))
}

/// What a `window_width × window_height` window can show of a **sidebar** — a
/// fixed-width column down the left edge, starting `reserve_top` pixels below
/// the top so it clears the tab strip — or `None` when the window is too
/// narrow or too short to hold one honestly.
///
/// The counterpart of [`fit_for`], and deliberately a separate function rather
/// than a flag on it: every one of the three numbers is derived differently.
/// The width comes from [`SIDEBAR_COLUMNS`] under [`SIDEBAR_MAX_FRACTION`]
/// instead of [`PANEL_MAX_COLUMNS`] under the whole window; the height is the
/// band below the strip rather than `1 - TOP_ANCHOR_FRACTION` of the window;
/// and the browse ceiling is the band rather than a taste bound.
///
/// ⚠️ **A `None` here is a fallback, not a failure.** The caller draws the
/// explorer as a popover instead, which is why the popover fit is the one it
/// always computes and this one is the override.
pub(super) fn sidebar_fit_for(
    window_width: f32,
    window_height: f32,
    reserve_top: f32,
    metrics: GridMetrics,
) -> Option<PanelFit> {
    // ⚠️ A reserve that is not a number is refused rather than treated as
    // zero. Zero would draw the sidebar over the strip it was meant to clear,
    // and a column overlapping the tabs looks like a paint bug rather than
    // like a measurement that failed — which is what it would be.
    if metrics.is_degenerate() || !reserve_top.is_finite() {
        return None;
    }
    let pad_x = PAD_X * metrics.scale;
    let pad_y = PAD_Y * metrics.scale;
    let allowance = (SIDEBAR_MAX_FRACTION * window_width).max(0.0);
    let exterior = allowance.min(index_to_f32(SIDEBAR_COLUMNS) * metrics.char_width);
    let content_columns = pixels_to_cells(2.0_f32.mul_add(-pad_x, exterior), metrics.char_width);
    if content_columns + 4 < PANEL_MIN_COLUMNS {
        return None;
    }
    // A reserve taller than the window leaves nothing rather than wrapping
    // into a huge band — `pixels_to_cells` floors a negative width at zero,
    // and the row check below refuses it.
    let vertical = 2.0_f32.mul_add(-pad_y, window_height - reserve_top.max(0.0));
    let max_interior_rows = pixels_to_cells(vertical, metrics.line_height);
    if max_interior_rows == 0 {
        return None;
    }
    Some(PanelFit::sidebar(content_columns, max_interior_rows))
}
