//! The painted overlays: the prompt strip and the floating panels.
//!
//! The strip was the first painted overlay, and it deliberately established
//! the pattern the bigger overlays reuse: after
//! [`FrameCompositor::compose`](iridium_editor::render::FrameCompositor)
//! has drawn the document into the surface's texture view, a **second render
//! pass** runs on the same view with [`LoadOp::Load`] — the composed frame is
//! kept, not cleared — drawing chrome and text over it. The primitives are
//! the kernel's own render types, [`RoundedQuadRenderer`] and
//! [`TextRenderer`], publicly exported from `iridium_editor::render`
//! precisely so a face paints its overlays with the renderers the kernel
//! already trusts instead of forking text rendering.
//!
//! # The pass structure
//!
//! 1. Shape every overlay's text into its own glyphon buffer — this painter
//!    owns a [`TextRenderer`] (and therefore an atlas and font system)
//!    separate from the compositor's, because `prepare` uploads one frame's
//!    text areas per renderer and the compositor has already spent its upload
//!    on the document. The strip and every open panel are prepared together
//!    as one frame's areas.
//! 2. Begin a render pass on the frame's view with `LoadOp::Load`.
//! 3. Draw the chrome — one ordered list of rounded-quad instances:
//!    backdrops, shadows, borders, backgrounds, selected rows, carets —
//!    then the text.
//! 4. Submit. The face's `render_frame` closure presents afterwards, so the
//!    document pass and the overlay pass reach the screen as one frame.
//!
//! # Panels are real shapes, matched to the web demo's chrome
//!
//! A panel is drawn as the web demo draws its palette and undo-tree overlays
//! (`examples/web/src/CommandPalette.tsx`, `UndoTreePanel.tsx`): an opaque
//! rounded-rectangle card with a hairline border, a soft drop shadow, real
//! pixel padding, and — for the modal top-anchored panels — a dimmed
//! backdrop over the page. The corners are genuine antialiased arcs from the
//! kernel's signed-distance [`RoundedQuadRenderer`]; nothing in the chrome
//! is a box-drawing glyph. (The glyph borders this face first shipped inked
//! the font's 1.32 em design cell against the renderer's 1.4 em line height,
//! so every vertical join gapped onto the document — chrome drawn as
//! terminal furniture is structurally broken at code line-spacing. The
//! terminal face's `FloatingBox` keeps its glyph borders, correctly, at
//! terminal cell packing.)
//!
//! The measurements are the web demo's, extracted from its style objects and
//! recorded on the constants below; the *colours* are not imported — they
//! continue to come from the theme system, so both presets keep their own
//! face. Content stays on the character grid inside the padding: a
//! [`PanelContent`] is rows of coloured character runs ([`Span`]), which the
//! builders in [`crate::search`], [`crate::command_palette`] and
//! [`crate::history_overlay`] compose and this module paints; the builders
//! stay windowless and testable, and [`PanelGeometry`] — the pixel
//! placement of a composed panel — is a pure function tested without a GPU.
//!
//! Because the chrome instances all render before any overlay text, a
//! backdrop dims the strip's background but not its already-shaped text;
//! panels never overlap the strip's row in practice, so the seam stays
//! theoretical — and it is the same layering the glyph chrome had.
//!
//! # Geometry
//!
//! The strip is one line high plus padding, pinned to the bottom edge, full
//! width — a docked bar with no free corners to round, given an honest
//! opaque background (the dark preset's transparent gutter colour used to
//! make its backing quad a no-op) and a one-physical-pixel hairline along
//! its top edge. A panel is at most [`PANEL_MAX_COLUMNS`] characters wide and
//! declines to exist at all on a window too small for an honest one, exactly
//! as the terminal face's `FloatingBox` declines. Where it lands is its
//! [`PanelAnchor`]: horizontally centred 12% down from the window's top edge
//! (palette, undo tree — the web demo's `paddingTop: 12vh`), horizontally
//! centred above the bottom edge and above the strip (search), or hung from
//! an arbitrary point (the context menu, from the click that opened it),
//! which is the one anchor that clamps itself inside the window edges and
//! flips above its point rather than clip.

use glyphon::{Buffer, TextBounds};
use iridium_editor::IridiumError;
use iridium_editor::render::{FrameTarget, RoundedQuad, RoundedQuadRenderer, TextRenderer};
use iridium_editor::theme::{Color, Theme};
use wgpu::{
    CommandEncoderDescriptor, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureFormat,
};

use crate::tab_strip::{
    TabColors, TabStripContent, TabStripLayout, tab_strip_height, tab_strip_layout, tab_strip_spans,
};
use crate::units::{
    dimension_to_bound, index_to_f32, pixel_to_bound, pixel_to_index, pixels_to_cells, u32_to_f32,
};

// =============================================================================
// The design values, extracted from the web demo
// =============================================================================
//
// The web demo is the spec for the chrome's shape language; every value below
// cites the style it was read from. Logical values are multiplied by the
// window's scale factor before they reach the pixel grid.

/// Panel corner radius in logical pixels — the web panels' `borderRadius:
/// "8px"`.
const PANEL_RADIUS: f32 = 8.0;

/// Horizontal interior padding in logical pixels — the web demo pads every
/// panel row and field `1rem` (16 px) horizontally.
const PAD_X: f32 = 16.0;

/// Vertical interior padding in logical pixels — the web input's `0.75rem`
/// (12 px) vertical padding.
const PAD_Y: f32 = 12.0;

/// The shadow's vertical offset in logical pixels — the web panels'
/// `boxShadow: "0 16px 48px rgba(0, 0, 0, 0.55)"`, first two lengths.
const SHADOW_OFFSET_Y: f32 = 16.0;

/// The shadow's blur half-width in logical pixels — the same `boxShadow`'s
/// blur radius.
const SHADOW_BLUR: f32 = 48.0;

/// The shadow's opacity — the same `boxShadow`'s alpha.
const SHADOW_ALPHA: f32 = 0.55;

/// The backdrop dim behind the modal top-anchored panels — the web demo's
/// full-screen `backgroundColor: "rgba(0, 0, 0, 0.45)"` behind both the
/// palette and the undo tree. The web demo has no search overlay, and the
/// native search panel gets no backdrop: it must not dim the matches it just
/// highlighted.
const BACKDROP_ALPHA: f32 = 0.45;

/// How far down the window's top edge a top-anchored panel starts — the web
/// backdrop's `paddingTop: "12vh"`, as a fraction of the window height.
const TOP_ANCHOR_FRACTION: f32 = 0.12;

/// The selected-row band's corner radius in logical pixels — the web demo's
/// key-hint chip radius (`borderRadius: "4px"`), reused so no sharp corner
/// exists anywhere in the chrome. (The web rows are square-edged but clipped
/// by their panel's `overflow: hidden`; a GPU face has no such clip, so the
/// band carries its own arcs.)
const ROW_RADIUS: f32 = 4.0;

/// How far the selected-row band is inset from each panel side, in logical
/// pixels, so its rounded corners never overhang the panel's.
const ROW_INSET: f32 = 4.0;

/// The hairline border's opacity: the theme's foreground at this alpha,
/// composited over the panel background — the derivation standing in for the
/// web demo's `border: "1px solid #444"` until a theme colour exists for it.
const HAIRLINE_ALPHA: f32 = 0.18;

/// The hairline border's width in **physical** pixels — one device pixel,
/// like the web demo's `1px` border on a `devicePixelRatio` display.
const HAIRLINE: f32 = 1.0;

/// Horizontal padding between the window edge and the strip's text, in
/// logical pixels — the shared `1rem` horizontal scale.
const STRIP_PAD_X: f32 = 16.0;

/// Vertical padding above and below the strip's line of text, in logical
/// pixels — the web footer's `0.4rem` vertical padding, the strip's closest
/// web relative.
const STRIP_PAD_Y: f32 = 6.0;

/// The caret bar's width in physical pixels.
const CARET_WIDTH: f32 = 2.0;

/// The opacity an inactive tab's label is drawn at, composited over the tab
/// strip's background: legible, and plainly behind the tab in front.
const TAB_INACTIVE_ALPHA: f32 = 0.55;

/// The opacity a tab's close control is drawn at — quieter still than an
/// inactive label, since it is a control rather than a name.
const TAB_CLOSE_ALPHA: f32 = 0.40;

/// The widest a floating panel gets in exterior character columns — the
/// terminal face's own cap, so the two faces' furniture reads the same
/// width class.
const PANEL_MAX_COLUMNS: usize = 64;

/// The narrowest exterior worth drawing, in the glyph chrome's
/// exterior-column vocabulary (content plus four border-and-pad columns).
/// Below this a panel shows nothing honestly — a list a dozen characters
/// wide answers no question.
const PANEL_MIN_COLUMNS: usize = 20;

/// The most content rows a panel's list shows at once, matching the terminal
/// face's `MAX_VISIBLE_ROWS`.
pub const PANEL_MAX_VISIBLE_ROWS: usize = 12;

/// One line of strip content, ready to paint.
#[derive(Debug, Clone)]
pub struct StripContent {
    /// The line's text, label and field together.
    pub text: String,
    /// The caret's column in `char`s, or `None` for a strip with nothing to
    /// type into.
    pub caret_column: Option<usize>,
    /// Whether the line reports a failure, which colors it accordingly.
    pub is_error: bool,
}

/// One run of characters in one colour.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    /// The characters.
    pub text: String,
    /// Their colour.
    pub color: Color,
}

impl Span {
    /// A run of characters in one colour.
    #[must_use]
    pub fn new(text: impl Into<String>, color: Color) -> Self {
        Self {
            text: text.into(),
            color,
        }
    }
}

/// One interior row of a panel.
///
/// The spans read left to right; the painter truncates anything past the
/// panel's content width and clips the shaped text at the content area, so a
/// builder that measured correctly is drawn exactly and a builder mistake
/// costs alignment, never a panic or an overrun panel edge.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRow {
    /// The row's text runs, left to right.
    pub spans: Vec<Span>,
    /// Whether the row is the selection, which tints its background.
    pub selected: bool,
    /// Whether the row is a group separator, drawn as a hairline rule across
    /// the panel instead of text.
    pub separator: bool,
}

impl PanelRow {
    /// A row from its runs.
    #[must_use]
    pub const fn new(spans: Vec<Span>) -> Self {
        Self {
            spans,
            selected: false,
            separator: false,
        }
    }

    /// A selection row from its runs.
    #[must_use]
    pub const fn selected(spans: Vec<Span>) -> Self {
        Self {
            spans,
            selected: true,
            separator: false,
        }
    }

    /// A separator row: a hairline rule between two groups of rows.
    #[must_use]
    pub const fn separator() -> Self {
        Self {
            spans: Vec::new(),
            selected: false,
            separator: true,
        }
    }

    /// The row's text with the colours dropped, for tests and diagnostics.
    #[must_use]
    pub fn text(&self) -> String {
        self.spans.iter().map(|span| span.text.as_str()).collect()
    }
}

/// Where a panel's caret sits, in interior coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelCaret {
    /// The interior row, counted from the panel's first content row.
    pub row: usize,
    /// The column in `char`s, counted from the content area's left edge.
    pub column: usize,
}

/// Which window edge a panel hangs from, or which point it hangs at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PanelAnchor {
    /// Twelve percent down from the top, horizontally centred — the palette
    /// and the undo tree, at the web demo's anchor.
    Top,
    /// Above the bottom edge (and above the strip when one is up),
    /// horizontally centred — the search panel, which belongs near the
    /// statusline row it occupies in the terminal face.
    Bottom,
    /// At an arbitrary point in the window, in physical pixels — the context
    /// menu, which hangs from the click that opened it. The point is the
    /// panel's top-left corner where the window allows it; see
    /// [`anchored_origin`] for the clamping and the flip.
    Point {
        /// The anchor's distance from the window's left edge.
        x: f32,
        /// The anchor's distance from the window's top edge.
        y: f32,
    },
}

/// A panel composed and ready to paint: rows of coloured runs on the
/// character grid, plus where its caret goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelContent {
    /// Which edge the panel hangs from.
    pub anchor: PanelAnchor,
    /// The characters available to each row inside the padding.
    pub content_columns: usize,
    /// The interior rows, top to bottom.
    pub rows: Vec<PanelRow>,
    /// The caret, if a field in the panel has focus.
    pub caret: Option<PanelCaret>,
}

/// What a window can honestly show of a panel, in grid units.
///
/// Builders lay their rows out against this; the painter places the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanelFit {
    /// The characters available to each interior row.
    pub content_columns: usize,
    /// The most interior rows a panel may hold on this window.
    pub max_interior_rows: usize,
}

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
    let height = index_to_f32(rows).mul_add(metrics.line_height, 2.0 * pad_y);
    if width > window_width || height > window_height {
        return None;
    }

    let centred = ((window_width - width) / 2.0).max(0.0);
    let (x, y) = match anchor {
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
fn fit_for(window_width: f32, window_height: f32, metrics: GridMetrics) -> Option<PanelFit> {
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
    Some(PanelFit {
        content_columns,
        max_interior_rows,
    })
}

/// Paints the overlays. One instance belongs to one surface, exactly as the
/// compositor does; its renderers' viewport uniforms track that surface.
pub struct OverlayPainter {
    /// The kernel's text renderer, owned by this overlay: its own font
    /// system, atlas and prepared-area slot.
    text: TextRenderer,
    /// The kernel's rounded-quad renderer for all the chrome: backdrops,
    /// shadows, borders, backgrounds, selected rows and carets, drawn as one
    /// ordered instance list.
    chrome: RoundedQuadRenderer,
    /// The window scale factor, converting the chrome's logical measurements
    /// to physical pixels. Only the face knows it; see [`Self::set_scale`].
    scale: f32,
    /// Where each panel of the last painted frame landed, in the order the
    /// caller handed them in. See [`Self::painted_panels`].
    painted: Vec<Option<PanelGeometry>>,
    /// Where the tab strip of the last painted frame landed. See
    /// [`Self::painted_tab_strip`].
    painted_tabs: Option<TabStripLayout>,
}

impl std::fmt::Debug for OverlayPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayPainter").finish_non_exhaustive()
    }
}

/// One panel shaped into a buffer, waiting for the shared prepare.
struct ShapedPanel {
    /// The shaped content rows.
    buffer: Buffer,
    /// Where the panel landed.
    geometry: PanelGeometry,
    /// The characters each row was budgeted, giving the horizontal clip bound.
    columns: usize,
    /// The rows shaped, giving the vertical clip bound.
    rows: usize,
    /// The panel's base text colour, used as the area default.
    color: Color,
}

impl OverlayPainter {
    /// Creates the painter for a surface with the given format and initial
    /// pixel dimensions.
    ///
    /// # Errors
    ///
    /// Returns an error when the kernel's text renderer cannot be created.
    pub fn new(
        device: &Device,
        queue: &Queue,
        format: TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<Self, IridiumError> {
        let text = TextRenderer::new(device, queue, format)?;
        let chrome = RoundedQuadRenderer::new(device, format);
        let mut painter = Self {
            text,
            chrome,
            scale: 1.0,
            painted: Vec::new(),
            painted_tabs: None,
        };
        painter.resize(queue, width, height);
        Ok(painter)
    }

    /// Sets the overlay font, size first so the load remeasures at it —
    /// the same order the compositor requires, for the same reason:
    /// `load_font` is the only remeasuring path.
    ///
    /// A refused size leaves the painter at its previous one; see the call
    /// site in `app` for why that is handled by surviving rather than by
    /// reporting.
    ///
    /// # Returns
    ///
    /// `false` when the bytes held no readable face. The desktop face passes a
    /// font compiled into the binary, so a `false` here means the build itself
    /// is wrong rather than the input — which is worth surfacing to the caller
    /// instead of being swallowed, precisely because it should be impossible.
    #[must_use = "an overlay with no font measures an approximation and draws nothing"]
    pub fn set_font(&mut self, size: f32, data: Vec<u8>) -> bool {
        let _ = self.text.set_font_size(size);
        self.text.load_font(data)
    }

    /// Sets the window scale factor the chrome's logical measurements are
    /// multiplied by — paddings, radii, the shadow. Called from the face on
    /// open and on every scale change, mirroring [`Self::set_font`]'s call
    /// discipline; only the face knows the window's scale. A value no
    /// display can report falls back to `1.0` rather than collapsing the
    /// chrome.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = if scale.is_finite() && scale > 0.0 {
            scale
        } else {
            1.0
        };
    }

    /// Tracks a surface resize, in physical pixels.
    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32) {
        self.text.update_viewport(queue, width, height);
        self.chrome.update_viewport(queue, width, height);
    }

    /// The height in pixels the strip occupies when it is up.
    pub fn strip_height(&self) -> f32 {
        2.0_f32.mul_add(STRIP_PAD_Y * self.scale, self.text.line_height())
    }

    /// The height in pixels the tab strip occupies along the top edge of a
    /// window `height` pixels tall, and so the height the document must be
    /// pushed down by.
    ///
    /// Zero before a font has measured, and zero on a window too short to
    /// hold the band — the same condition [`Self::paint`] declines to draw
    /// on, so the reserve and the band appear and disappear together.
    pub fn tab_strip_height(&mut self, height: f32) -> f32 {
        tab_strip_height(height, self.metrics())
    }

    /// Where the tab strip of the last painted frame landed, or `None` when
    /// the frame drew none.
    ///
    /// This is what a press is hit-tested against, for the same reason
    /// [`Self::painted_panels`] is: the placement on screen, not one
    /// recomputed from state that may have moved since.
    #[must_use]
    pub const fn painted_tab_strip(&self) -> Option<&TabStripLayout> {
        self.painted_tabs.as_ref()
    }

    /// The height in pixels a panel with `interior_rows` content rows takes,
    /// padding included.
    pub fn panel_height(&self, interior_rows: usize) -> f32 {
        index_to_f32(interior_rows).mul_add(self.text.line_height(), 2.0 * PAD_Y * self.scale)
    }

    /// What a window of this size can honestly show of a panel, or `None`
    /// when it cannot show one at all.
    ///
    /// The keys of an undrawable panel keep working regardless; declining to
    /// draw must never trap `Escape`.
    pub fn panel_fit(&mut self, width: u32, height: u32) -> Option<PanelFit> {
        let metrics = self.metrics();
        fit_for(u32_to_f32(width), u32_to_f32(height), metrics)
    }

    /// Where each panel of the last painted frame landed, in the order it was
    /// handed to [`Self::paint`]; `None` for a panel the window could not
    /// hold, so the indices never shift under the caller.
    ///
    /// This is what a pointer is hit-tested against: the placement that is on
    /// screen, rather than a placement recomputed from state that may have
    /// moved since the frame the user is clicking on.
    #[must_use]
    pub fn painted_panels(&self) -> &[Option<PanelGeometry>] {
        &self.painted
    }

    /// The text grid the chrome is placed against, as currently measured.
    fn metrics(&mut self) -> GridMetrics {
        GridMetrics {
            char_width: self.text.char_width(),
            line_height: self.text.line_height(),
            scale: self.scale,
        }
    }

    /// Paints the overlays over an already-composed frame: the tab strip,
    /// then the prompt strip, then every open panel in the order given, later
    /// panels on top.
    ///
    /// Runs the second render pass described in the module documentation.
    /// A window shorter than a strip paints that strip not at all; a panel
    /// that no longer fits paints nothing rather than a sliver.
    ///
    /// # Errors
    ///
    /// Returns an error when glyph preparation or text rendering fails.
    pub fn paint(
        &mut self,
        target: FrameTarget<'_>,
        tabs: Option<&TabStripContent>,
        strip: Option<&StripContent>,
        panels: &[&PanelContent],
        theme: &Theme,
    ) -> Result<(), IridiumError> {
        let FrameTarget {
            view,
            device,
            queue,
            width,
            height,
        } = target;

        let width_f = u32_to_f32(width);
        let height_f = u32_to_f32(height);
        let mut chrome: Vec<RoundedQuad> = Vec::new();

        // First, so a modal panel's backdrop dims the tab strip's chrome the
        // same way it dims the prompt strip's.
        let tab_buffer = tabs
            .and_then(|content| self.shape_tabs(content, theme, width_f, height_f, &mut chrome));
        self.painted_tabs = tab_buffer.as_ref().map(|(_, layout)| layout.clone());

        let strip_buffer = strip
            .and_then(|content| self.shape_strip(content, theme, width_f, height_f, &mut chrome));

        let reserve_bottom = if strip_buffer.is_some() {
            self.strip_height()
        } else {
            0.0
        };
        // Placement is recorded per input panel — `None` for one the window
        // could not hold — so the indices stay aligned with what the caller
        // handed in and a face can hit-test exactly what it is looking at.
        let mut placed: Vec<Option<PanelGeometry>> = Vec::with_capacity(panels.len());
        let shaped: Vec<ShapedPanel> = panels
            .iter()
            .filter_map(|panel| {
                let shaped =
                    self.shape_panel(panel, theme, width_f, height_f, reserve_bottom, &mut chrome);
                placed.push(shaped.as_ref().map(|panel| panel.geometry));
                shaped
            })
            .collect();
        self.painted = placed;

        if chrome.is_empty() && strip_buffer.is_none() && shaped.is_empty() {
            return Ok(());
        }

        let strip_pad_x = STRIP_PAD_X * self.scale;
        let mut areas = Vec::new();
        if let Some((buffer, layout)) = &tab_buffer {
            areas.push(TextRenderer::create_text_area(
                buffer,
                layout.text_x,
                layout.text_y,
                1.0,
                TextBounds {
                    // Clipped to the band, so a scrolled strip's cut glyphs
                    // vanish at the window edge instead of bleeding past it.
                    left: pixel_to_bound(layout.band.x),
                    top: pixel_to_bound(layout.band.y),
                    right: dimension_to_bound(width),
                    bottom: pixel_to_bound(layout.band.y + layout.band.height),
                },
                tab_colors(theme).inactive,
            ));
        }
        if let Some((buffer, scroll_x, top, color)) = &strip_buffer {
            areas.push(TextRenderer::create_text_area(
                buffer,
                strip_pad_x - scroll_x,
                *top,
                1.0,
                TextBounds {
                    // Clipped at the padding so a scrolled field's cut glyphs
                    // vanish instead of bleeding under the window edge.
                    left: pixel_to_bound(strip_pad_x),
                    top: pixel_to_bound(STRIP_PAD_Y.mul_add(-self.scale, *top)),
                    right: dimension_to_bound(width),
                    bottom: dimension_to_bound(height),
                },
                *color,
            ));
        }
        for panel in &shaped {
            let x = panel.geometry.content_x;
            let y = panel.geometry.content_y;
            let width = index_to_f32(panel.columns) * panel.geometry.char_width;
            let height = index_to_f32(panel.rows) * panel.geometry.line_height;
            areas.push(TextRenderer::create_text_area(
                &panel.buffer,
                x,
                y,
                1.0,
                TextBounds {
                    // Clipped at the content area, so an overlong row ends at
                    // the padding rather than crossing the panel's edge.
                    left: pixel_to_bound(x),
                    top: pixel_to_bound(y),
                    right: pixel_to_bound(x + width),
                    bottom: pixel_to_bound(y + height),
                },
                panel.color,
            ));
        }
        self.text.prepare(device, queue, areas)?;

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("Iridium Overlay Encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: Some("Iridium Overlay Pass"),
                color_attachments: &[Some(RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: Operations {
                        // The composed document frame is underneath; keep it.
                        load: LoadOp::Load,
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            self.chrome.render(&mut pass, queue, &chrome);
            self.text.render(&mut pass)?;
        }
        queue.submit(std::iter::once(encoder.finish()));

        self.text.trim_cache();
        Ok(())
    }

    /// Shapes the tab strip and pushes its chrome, or `None` when there are
    /// no tabs or no honest room for the band. Returns the buffer with the
    /// placement the caller records for hit-testing.
    ///
    /// Instance order is paint order: the band, the active tab's card, then
    /// the hairline along the band's lower edge — the hairline last so a card
    /// that reaches the band's bottom edge cannot cover the seam between the
    /// strip and the document.
    fn shape_tabs(
        &mut self,
        content: &TabStripContent,
        theme: &Theme,
        width_f: f32,
        height_f: f32,
        chrome: &mut Vec<RoundedQuad>,
    ) -> Option<(Buffer, TabStripLayout)> {
        let metrics = self.metrics();
        let layout = tab_strip_layout(width_f, height_f, metrics, content)?;
        let band = layout.band;

        chrome.push(RoundedQuad::new(
            band.x,
            band.y,
            band.width,
            band.height,
            strip_background(theme),
            band.radius,
        ));
        for (placement, tab) in layout.tabs.iter().zip(&content.tabs) {
            if tab.is_active {
                let card = placement.card;
                chrome.push(RoundedQuad::new(
                    card.x,
                    card.y,
                    card.width,
                    card.height,
                    tab_card_color(theme),
                    card.radius,
                ));
            }
        }
        chrome.push(RoundedQuad::new(
            band.x,
            band.y + band.height - HAIRLINE,
            band.width,
            HAIRLINE,
            hairline_color(theme),
            0.0,
        ));

        let spans = tab_strip_spans(content, tab_colors(theme));
        let mut buffer = self.text.create_buffer(None);
        self.text.set_rich_text(
            &mut buffer,
            spans.iter().map(|span| (span.text.as_str(), span.color)),
        );
        self.text.shape_buffer(&mut buffer);
        Some((buffer, layout))
    }

    /// Shapes the strip and pushes its chrome, or `None` on a window too
    /// small for it. Returns the buffer with its scroll, top edge and colour.
    fn shape_strip(
        &mut self,
        content: &StripContent,
        theme: &Theme,
        width_f: f32,
        height_f: f32,
        chrome: &mut Vec<RoundedQuad>,
    ) -> Option<(Buffer, f32, f32, Color)> {
        let line_height = self.text.line_height();
        let pad_x = STRIP_PAD_X * self.scale;
        let pad_y = STRIP_PAD_Y * self.scale;
        let strip_height = 2.0_f32.mul_add(pad_y, line_height);
        if strip_height > height_f || width_f <= 2.0 * pad_x {
            return None;
        }
        let strip_top = height_f - strip_height;
        let char_width = self.text.char_width();

        // Scroll the field just enough to keep the caret on screen.
        let cells = pixels_to_cells(2.0_f32.mul_add(-pad_x, width_f), char_width);
        let scroll_cells = content
            .caret_column
            .map_or(0, |caret| scroll_for(caret, cells));
        let scroll_x = index_to_f32(scroll_cells) * char_width;

        // A docked, edge-to-edge bar: no free corners to round, an honest
        // opaque background — the dark preset's transparent gutter used to
        // make this quad a no-op — and a hairline along its top edge.
        chrome.push(RoundedQuad::new(
            0.0,
            strip_top,
            width_f,
            strip_height,
            strip_background(theme),
            0.0,
        ));
        chrome.push(RoundedQuad::new(
            0.0,
            strip_top,
            width_f,
            HAIRLINE,
            hairline_color(theme),
            0.0,
        ));
        if let Some(caret) = content.caret_column {
            if caret >= scroll_cells {
                let caret_x = index_to_f32(caret - scroll_cells).mul_add(char_width, pad_x);
                chrome.push(RoundedQuad::new(
                    caret_x,
                    strip_top + pad_y,
                    CARET_WIDTH,
                    line_height,
                    theme.editor.cursor,
                    0.0,
                ));
            }
        }

        let color = if content.is_error {
            theme.editor.diagnostic_error
        } else {
            theme.editor.foreground
        };
        let mut buffer = self.text.create_buffer(None);
        self.text.set_text(&mut buffer, &content.text, color);
        self.text.shape_buffer(&mut buffer);
        Some((buffer, scroll_x, strip_top + pad_y, color))
    }

    /// Shapes one panel and pushes its chrome, or `None` when the window can
    /// no longer show it honestly.
    ///
    /// Instance order is paint order: backdrop (modal panels only), shadow,
    /// hairline border, background, selected rows, caret — then the text
    /// draws over all of it in the shared text pass.
    fn shape_panel(
        &mut self,
        panel: &PanelContent,
        theme: &Theme,
        width_f: f32,
        height_f: f32,
        reserve_bottom: f32,
        chrome: &mut Vec<RoundedQuad>,
    ) -> Option<ShapedPanel> {
        if panel.rows.is_empty() {
            return None;
        }
        let metrics = self.metrics();
        let geometry = panel_geometry(
            width_f,
            height_f,
            metrics,
            panel.anchor,
            panel.rows.len(),
            panel.content_columns,
            reserve_bottom,
        )?;
        let exterior = geometry.exterior;

        // The modal top-anchored panels dim the page behind them, exactly as
        // the web demo's full-screen backdrop does; each panel carries its
        // own dim, so stacked modals compound the way stacked portals do.
        if matches!(panel.anchor, PanelAnchor::Top) {
            chrome.push(RoundedQuad::new(
                0.0,
                0.0,
                width_f,
                height_f,
                Color::new(0.0, 0.0, 0.0, BACKDROP_ALPHA),
                0.0,
            ));
        }
        // The drop shadow: the same rounded shape, offset and blurred.
        chrome.push(RoundedQuad::shadow(
            exterior.x,
            SHADOW_OFFSET_Y.mul_add(self.scale, exterior.y),
            exterior.width,
            exterior.height,
            Color::new(0.0, 0.0, 0.0, SHADOW_ALPHA),
            exterior.radius,
            SHADOW_BLUR * self.scale,
        ));
        // The hairline border: the exterior in the border colour, with the
        // opaque background inset one physical pixel over it.
        chrome.push(RoundedQuad::new(
            exterior.x,
            exterior.y,
            exterior.width,
            exterior.height,
            hairline_color(theme),
            exterior.radius,
        ));
        chrome.push(RoundedQuad::new(
            exterior.x + HAIRLINE,
            exterior.y + HAIRLINE,
            2.0_f32.mul_add(-HAIRLINE, exterior.width),
            2.0_f32.mul_add(-HAIRLINE, exterior.height),
            panel_background(theme),
            (exterior.radius - HAIRLINE).max(0.0),
        ));
        for (index, row) in panel.rows.iter().enumerate() {
            if row.selected {
                let band = geometry.selected_row_rect(index);
                chrome.push(RoundedQuad::new(
                    band.x,
                    band.y,
                    band.width,
                    band.height,
                    theme.editor.selection,
                    band.radius,
                ));
            }
            if row.separator {
                let rule = geometry.separator_rect(index);
                chrome.push(RoundedQuad::new(
                    rule.x,
                    rule.y,
                    rule.width,
                    rule.height,
                    hairline_color(theme),
                    rule.radius,
                ));
            }
        }
        if let Some(caret) = panel.caret {
            if caret.row < panel.rows.len() && caret.column <= panel.content_columns {
                let bar = geometry.caret_rect(caret);
                chrome.push(RoundedQuad::new(
                    bar.x,
                    bar.y,
                    bar.width,
                    bar.height,
                    theme.editor.cursor,
                    bar.radius,
                ));
            }
        }

        let base = theme.editor.foreground;
        let spans = panel_spans(panel, base);
        let mut buffer = self.text.create_buffer(None);
        self.text.set_rich_text(
            &mut buffer,
            spans.iter().map(|span| (span.text.as_str(), span.color)),
        );
        self.text.shape_buffer(&mut buffer);
        Some(ShapedPanel {
            buffer,
            geometry,
            columns: panel.content_columns,
            rows: panel.rows.len(),
            color: base,
        })
    }
}

/// The panel's content rows as one run list with newlines, truncated to the
/// content width.
///
/// Truncation counts characters, never bytes, so a row that measured wrong
/// is cut on a character boundary; the painter's clip bounds catch whatever
/// a proportional glyph still lets past.
fn panel_spans(panel: &PanelContent, base: Color) -> Vec<Span> {
    let mut spans = Vec::with_capacity(panel.rows.len() * 4);
    for (index, row) in panel.rows.iter().enumerate() {
        if index > 0 {
            spans.push(Span::new("\n", base));
        }
        let mut used = 0;
        for span in &row.spans {
            if used >= panel.content_columns {
                break;
            }
            let budget = panel.content_columns - used;
            let mut text = String::new();
            for character in span.text.chars().take(budget) {
                text.push(character);
                used += 1;
            }
            if !text.is_empty() {
                spans.push(Span::new(text, span.color));
            }
        }
    }
    spans
}

/// The colour behind a panel: the theme's current-line band composited over
/// the editor background, opaque.
///
/// The current-line colour is the one theme colour whose whole purpose is "a
/// stripe that is still text", which is exactly what a panel is — the same
/// reasoning the terminal face's overlay style records at length. A theme
/// that leaves it fully transparent falls back to the gutter and then to the
/// background, so the panel always stands on something.
fn panel_background(theme: &Theme) -> Color {
    let under = if theme.editor.background.a > 0.0 {
        theme.editor.background
    } else {
        Color::new(0.0, 0.0, 0.0, 1.0)
    };
    for layer in [theme.editor.current_line, theme.editor.gutter] {
        if layer.a > 0.0 {
            return composite(layer, under);
        }
    }
    Color::new(under.r, under.g, under.b, 1.0)
}

/// The colour behind the strip: the panels' background, so the docked bar
/// and the floating cards read as one chrome family.
///
/// This is deliberately *not* `theme.editor.gutter`: the dark preset leaves
/// the gutter fully transparent, which made the strip's old backing quad a
/// no-op and sat strip text directly on document text.
fn strip_background(theme: &Theme) -> Color {
    panel_background(theme)
}

/// The colour of the active tab's card: the document's own background, made
/// opaque.
///
/// The tab in front is a window onto the document, so it is drawn in the
/// document's colour, sitting on a band that is a shade off it — the same
/// relationship a browser tab has to its page. A theme that leaves the
/// background transparent falls back to black, as the panels do, so the card
/// never shows the text it is meant to be covering.
fn tab_card_color(theme: &Theme) -> Color {
    let background = theme.editor.background;
    if background.a > 0.0 {
        Color::new(background.r, background.g, background.b, 1.0)
    } else {
        Color::new(0.0, 0.0, 0.0, 1.0)
    }
}

/// The four colours the tab strip's text is drawn in.
///
/// Each is composited opaque over the band it sits on, so a theme whose
/// foreground carries alpha cannot leave a label showing document text
/// through itself. The dirty dot takes the theme's "modified" colour — the
/// one colour whose whole meaning is *this differs from what was saved* —
/// rather than a fifth derivation.
fn tab_colors(theme: &Theme) -> TabColors {
    let band = strip_background(theme);
    let foreground = theme.editor.foreground;
    let at = |alpha: f32| {
        composite(
            Color::new(foreground.r, foreground.g, foreground.b, alpha),
            band,
        )
    };
    TabColors {
        active: composite(foreground, band),
        inactive: at(TAB_INACTIVE_ALPHA),
        close: at(TAB_CLOSE_ALPHA),
        dirty: composite(theme.editor.change_modified, band),
    }
}

/// The hairline border colour: the theme's foreground at [`HAIRLINE_ALPHA`],
/// composited opaque over the panel background — a derivation rather than a
/// new theme field, so existing theme files keep deserializing untouched.
fn hairline_color(theme: &Theme) -> Color {
    let foreground = theme.editor.foreground;
    composite(
        Color::new(foreground.r, foreground.g, foreground.b, HAIRLINE_ALPHA),
        panel_background(theme),
    )
}

/// `over` alpha-composited onto an opaque `under`.
fn composite(over: Color, under: Color) -> Color {
    let blend = |source: f32, target: f32| over.a.mul_add(source, (1.0 - over.a) * target);
    Color::new(
        blend(over.r, under.r),
        blend(over.g, under.g),
        blend(over.b, under.b),
        1.0,
    )
}

/// The first cell of a line that is shown, so `caret` stays inside `cells`.
///
/// Zero until the caret would fall off the right edge, and then just enough
/// to keep it in the last cell. The caret's own cell counts: a caret past the
/// last glyph of a full line must still be visible, or typing into a field
/// that has filled the strip gives no feedback at all.
pub(crate) const fn scroll_for(caret: usize, cells: usize) -> usize {
    if cells == 0 || caret < cells {
        return 0;
    }
    caret + 1 - cells
}

#[cfg(test)]
mod tests {
    use iridium_editor::theme::{ClassicVariant, Theme};

    use super::{
        Color, GridMetrics, PAD_X, PAD_Y, PANEL_MAX_VISIBLE_ROWS, PanelAnchor, PanelCaret,
        PanelContent, PanelRow, Span, TOP_ANCHOR_FRACTION, fit_for, hairline_color,
        panel_background, panel_geometry, panel_spans, scroll_for, strip_background,
    };

    /// The working grid of a 14 px face on a 2× display.
    const CHAR_WIDTH: f32 = 16.8;
    const LINE_HEIGHT: f32 = 39.2;
    const SCALE: f32 = 2.0;
    const WINDOW: (f32, f32) = (3024.0, 1964.0);

    /// The standard test window's grid.
    const METRICS: GridMetrics = GridMetrics {
        char_width: CHAR_WIDTH,
        line_height: LINE_HEIGHT,
        scale: SCALE,
    };

    /// A grid whose cell tracks the given scale, as a real rescale would.
    fn metrics_at(scale: f32) -> GridMetrics {
        GridMetrics {
            char_width: CHAR_WIDTH / SCALE * scale,
            line_height: LINE_HEIGHT / SCALE * scale,
            scale,
        }
    }

    /// A mid-sized panel placed on the standard test window.
    fn placed(
        anchor: PanelAnchor,
        rows: usize,
        columns: usize,
        reserve: f32,
    ) -> super::PanelGeometry {
        panel_geometry(WINDOW.0, WINDOW.1, METRICS, anchor, rows, columns, reserve)
            .expect("the standard window holds a mid-sized panel")
    }

    #[test]
    fn a_short_line_does_not_scroll() {
        assert_eq!(scroll_for(0, 40), 0);
        assert_eq!(scroll_for(39, 40), 0);
    }

    #[test]
    fn the_caret_is_kept_in_the_last_cell() {
        assert_eq!(scroll_for(40, 40), 1);
        assert_eq!(scroll_for(55, 40), 16);
    }

    #[test]
    fn a_zero_width_strip_is_not_divided_by() {
        assert_eq!(scroll_for(10, 0), 0);
    }

    #[test]
    fn a_panel_is_horizontally_centred() {
        let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
        let left = geometry.exterior.x;
        let right = WINDOW.0 - (geometry.exterior.x + geometry.exterior.width);
        assert!((left - right).abs() < 0.001);
    }

    #[test]
    fn the_top_anchor_is_the_web_demos_twelve_vh() {
        let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
        assert!(
            TOP_ANCHOR_FRACTION
                .mul_add(-WINDOW.1, geometry.exterior.y)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn a_bottom_panel_stacks_above_the_reserve() {
        let reserve = 60.0;
        let geometry = placed(PanelAnchor::Bottom, 2, 40, reserve);
        let bottom = geometry.exterior.y + geometry.exterior.height;
        assert!((bottom - (WINDOW.1 - reserve)).abs() < 0.001);
    }

    #[test]
    fn the_padding_arithmetic_matches_the_web_scale() {
        let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
        let expected_width = 40.0_f32.mul_add(CHAR_WIDTH, 2.0 * PAD_X * SCALE);
        let expected_height = 8.0_f32.mul_add(LINE_HEIGHT, 2.0 * PAD_Y * SCALE);
        assert!((geometry.exterior.width - expected_width).abs() < 0.001);
        assert!((geometry.exterior.height - expected_height).abs() < 0.001);
        let expected_content_x = PAD_X.mul_add(SCALE, geometry.exterior.x);
        let expected_content_y = PAD_Y.mul_add(SCALE, geometry.exterior.y);
        assert!((geometry.content_x - expected_content_x).abs() < 0.001);
        assert!((geometry.content_y - expected_content_y).abs() < 0.001);
    }

    #[test]
    fn the_radius_never_exceeds_half_the_short_side() {
        for rows in [1_usize, 2, 8, 12] {
            for columns in [16_usize, 40, 60] {
                let geometry = placed(PanelAnchor::Top, rows, columns, 0.0);
                let short = geometry.exterior.width.min(geometry.exterior.height);
                assert!(geometry.exterior.radius <= short / 2.0 + 0.001);
            }
        }
    }

    /// The standing rule the old glyph tests asserted with arc characters,
    /// translated: every drawable panel keeps genuine arcs.
    #[test]
    fn every_drawable_panel_keeps_a_genuine_arc() {
        for (width, height, scale) in [
            (3024.0, 1964.0, 2.0),
            (1512.0, 982.0, 1.0),
            (800.0, 500.0, 1.25),
        ] {
            for anchor in [PanelAnchor::Top, PanelAnchor::Bottom] {
                if let Some(geometry) =
                    panel_geometry(width, height, metrics_at(scale), anchor, 2, 20, 0.0)
                {
                    assert!(geometry.exterior.radius > 0.0, "a panel corner went sharp");
                }
            }
        }
    }

    /// A menu-sized panel hung from a point on the standard test window.
    fn hung(x: f32, y: f32) -> super::PanelGeometry {
        panel_geometry(
            WINDOW.0,
            WINDOW.1,
            METRICS,
            PanelAnchor::Point { x, y },
            7,
            20,
            0.0,
        )
        .expect("the standard window holds a menu-sized panel")
    }

    #[test]
    fn a_point_anchored_panel_hangs_from_the_click() {
        let geometry = hung(400.0, 500.0);
        assert!((geometry.exterior.x - 400.0).abs() < 0.001);
        assert!((geometry.exterior.y - 500.0).abs() < 0.001);
    }

    #[test]
    fn a_point_anchored_panel_is_clamped_inside_every_edge() {
        for (x, y) in [
            (WINDOW.0 - 4.0, WINDOW.1 - 4.0),
            (WINDOW.0 + 500.0, 0.0),
            (0.0, WINDOW.1 - 1.0),
            (-40.0, -40.0),
        ] {
            let geometry = hung(x, y);
            assert!(geometry.exterior.x >= -0.001, "clipped the left edge");
            assert!(geometry.exterior.y >= -0.001, "clipped the top edge");
            assert!(
                geometry.exterior.x + geometry.exterior.width <= WINDOW.0 + 0.001,
                "clipped the right edge"
            );
            assert!(
                geometry.exterior.y + geometry.exterior.height <= WINDOW.1 + 0.001,
                "clipped the bottom edge"
            );
            assert!(geometry.exterior.radius > 0.0, "a menu corner went sharp");
        }
    }

    #[test]
    fn a_point_anchored_panel_flips_above_a_click_near_the_bottom() {
        // The macOS behaviour: a menu with no room below opens upward from the
        // click rather than sliding until it covers it.
        let click = WINDOW.1 - 20.0;
        let geometry = hung(400.0, click);
        assert!(
            (geometry.exterior.y + geometry.exterior.height - click).abs() < 0.001,
            "the panel's bottom edge sits on the click"
        );
    }

    #[test]
    fn a_point_anchor_with_room_neither_way_slides_instead_of_flipping() {
        let height = 400.0;
        let geometry = panel_geometry(
            WINDOW.0,
            height,
            METRICS,
            PanelAnchor::Point { x: 0.0, y: 300.0 },
            7,
            20,
            0.0,
        )
        .expect("a short window still holds a seven-row menu");
        assert!(geometry.exterior.y >= -0.001);
        assert!(geometry.exterior.y + geometry.exterior.height <= height + 0.001);
    }

    #[test]
    fn a_point_anchored_panel_declines_a_window_it_cannot_fit() {
        assert!(
            panel_geometry(
                200.0,
                200.0,
                METRICS,
                PanelAnchor::Point { x: 10.0, y: 10.0 },
                7,
                20,
                0.0,
            )
            .is_none()
        );
    }

    #[test]
    fn the_hit_test_names_the_row_under_a_pixel_and_nothing_off_the_panel() {
        let geometry = hung(400.0, 500.0);
        let x = geometry.content_x + 1.0;
        for row in 0..7_usize {
            let y = (super::index_to_f32(row) + 0.5).mul_add(LINE_HEIGHT, geometry.content_y);
            assert_eq!(geometry.row_at(x, y, 7), Some(row));
        }
        assert!(
            geometry.row_at(x, geometry.exterior.y + 1.0, 7).is_none(),
            "the top padding is not row zero"
        );
        assert!(
            geometry
                .row_at(x, geometry.exterior.y + geometry.exterior.height - 1.0, 7)
                .is_none(),
            "the bottom padding is not the last row"
        );
        assert!(
            geometry
                .row_at(geometry.exterior.x - 1.0, 500.0, 7)
                .is_none(),
            "a pixel left of the panel is on no row"
        );
        assert!(!geometry.contains(geometry.exterior.x - 1.0, 500.0));
        assert!(geometry.contains(geometry.exterior.x + 1.0, geometry.exterior.y + 1.0));
    }

    #[test]
    fn separator_rules_stay_inside_the_panel() {
        let geometry = hung(400.0, 500.0);
        for index in 0..7_usize {
            let rule = geometry.separator_rect(index);
            let band = geometry.selected_row_rect(index);
            assert!(rule.x >= geometry.exterior.x);
            assert!(rule.x + rule.width <= geometry.exterior.x + geometry.exterior.width + 0.001);
            assert!(rule.y >= band.y, "the rule sits inside its own row");
            assert!(rule.y + rule.height <= band.y + band.height + 0.001);
        }
    }

    #[test]
    fn the_caret_rect_stays_inside_the_content_area() {
        let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
        for (row, column) in [(0_usize, 0_usize), (7, 40)] {
            let bar = geometry.caret_rect(PanelCaret { row, column });
            assert!(bar.x >= geometry.content_x - 0.001);
            let content_right = 40.0_f32.mul_add(CHAR_WIDTH, geometry.content_x);
            assert!(bar.x <= content_right + 0.001);
            assert!(bar.y >= geometry.content_y - 0.001);
            let content_bottom = 8.0_f32.mul_add(LINE_HEIGHT, geometry.content_y);
            assert!(bar.y + bar.height <= content_bottom + 0.001);
        }
    }

    #[test]
    fn selected_row_bands_stay_inside_the_panel() {
        let geometry = placed(PanelAnchor::Top, 8, 40, 0.0);
        for index in 0..8_usize {
            let band = geometry.selected_row_rect(index);
            assert!(band.x >= geometry.exterior.x);
            assert!(band.x + band.width <= geometry.exterior.x + geometry.exterior.width + 0.001);
            assert!(band.y >= geometry.exterior.y);
            assert!(band.y + band.height <= geometry.exterior.y + geometry.exterior.height + 0.001);
            assert!(band.radius > 0.0, "a chrome corner went sharp");
        }
    }

    /// Content composed against a fit always produces geometry that fits the
    /// window that produced the fit, at both common scales.
    #[test]
    fn the_fit_round_trip_fits_the_window_at_both_scales() {
        for (width, height, scale) in [(3024.0, 1964.0, 2.0), (1512.0, 982.0, 1.0)] {
            let metrics = metrics_at(scale);
            let fit = fit_for(width, height, metrics).expect("a full window fits a panel");
            for anchor in [PanelAnchor::Top, PanelAnchor::Bottom] {
                for rows in [1, fit.max_interior_rows.min(PANEL_MAX_VISIBLE_ROWS)] {
                    let geometry = panel_geometry(
                        width,
                        height,
                        metrics,
                        anchor,
                        rows,
                        fit.content_columns,
                        0.0,
                    )
                    .expect("fit-sized content places");
                    assert!(geometry.exterior.x >= 0.0);
                    assert!(geometry.exterior.y >= 0.0);
                    assert!(geometry.exterior.x + geometry.exterior.width <= width + 0.001);
                    assert!(geometry.exterior.y + geometry.exterior.height <= height + 0.001);
                }
            }
        }
    }

    #[test]
    fn a_window_too_small_composes_no_panel() {
        assert!(fit_for(200.0, 300.0, METRICS).is_none());
        assert!(fit_for(3024.0, 80.0, METRICS).is_none());
        let unmeasured = GridMetrics {
            char_width: 0.0,
            ..METRICS
        };
        assert!(fit_for(3024.0, 1964.0, unmeasured).is_none());
    }

    #[test]
    fn a_row_wider_than_the_panel_is_truncated_on_a_character_boundary() {
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        let panel = PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: 4,
            rows: vec![PanelRow::new(vec![Span::new("héllo!", white)])],
            caret: None,
        };
        let text: String = panel_spans(&panel, white)
            .iter()
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(text, "héll");
    }

    #[test]
    fn the_panel_background_is_opaque_in_both_presets() {
        for theme in [Theme::dark(), Theme::light()] {
            let background = panel_background(&theme);
            assert!(
                (background.a - 1.0).abs() < f32::EPSILON,
                "a translucent panel would show the document through itself"
            );
        }
    }

    /// The addendum finding, pinned: the strip stands on an opaque
    /// background in both presets, transparent gutter or not.
    #[test]
    fn the_strip_background_is_opaque_in_both_presets() {
        for theme in [Theme::dark(), Theme::light()] {
            let background = strip_background(&theme);
            assert!(
                (background.a - 1.0).abs() < f32::EPSILON,
                "a translucent strip would sit its text on document text"
            );
        }
    }

    /// The active tab has to be findable, and it is findable only because its
    /// card is a different colour from the band it sits on.
    ///
    /// Both are derived — the band from the theme's current-line band over the
    /// background, the card from the background itself — so a theme whose
    /// current-line colour is transparent, or a future edit that pointed both
    /// derivations at the same field, would leave a strip on which the tab in
    /// front is indistinguishable from every other. Nothing else in the face
    /// would fail; the strip would simply stop saying which file you are in.
    #[test]
    fn the_active_tab_is_distinguishable_from_the_band_in_every_preset() {
        let themes = [Theme::dark(), Theme::light()]
            .into_iter()
            .chain(ClassicVariant::ALL.into_iter().map(ClassicVariant::theme));
        for theme in themes {
            let band = strip_background(&theme);
            let card = super::tab_card_color(&theme);
            let distance =
                (band.r - card.r).abs() + (band.g - card.g).abs() + (band.b - card.b).abs();
            assert!(
                distance > 0.01,
                "{}: the active tab's card is the same colour as the band under it",
                theme.name
            );
            assert!(
                (card.a - 1.0).abs() < f32::EPSILON,
                "{}: a translucent card would show document text through the tab",
                theme.name
            );
        }
    }

    /// Every colour the strip's text is drawn in stands on the band opaquely,
    /// so no label can be washed out by a theme carrying alpha on its
    /// foreground.
    #[test]
    fn every_tab_text_colour_is_opaque_and_distinct_from_the_band() {
        for theme in [Theme::dark(), Theme::light()] {
            let band = strip_background(&theme);
            let colors = super::tab_colors(&theme);
            for (name, color) in [
                ("active", colors.active),
                ("inactive", colors.inactive),
                ("close", colors.close),
                ("dirty", colors.dirty),
            ] {
                assert!(
                    (color.a - 1.0).abs() < f32::EPSILON,
                    "{}: the {name} colour is translucent",
                    theme.name
                );
                let distance =
                    (band.r - color.r).abs() + (band.g - color.g).abs() + (band.b - color.b).abs();
                assert!(
                    distance > 0.01,
                    "{}: the {name} colour is invisible against the band",
                    theme.name
                );
            }
        }
    }

    #[test]
    fn the_hairline_is_opaque_in_both_presets() {
        for theme in [Theme::dark(), Theme::light()] {
            let hairline = hairline_color(&theme);
            assert!((hairline.a - 1.0).abs() < f32::EPSILON);
        }
    }
}
