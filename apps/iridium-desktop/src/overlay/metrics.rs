//! The design values the chrome is drawn with, and the two theme-dependent
//! choices derived from them.
//!
//! Split out of the painter for the reason the compositor's `metrics` module
//! was: a constant read by the geometry and painted by the painter belongs to
//! neither of them, and a value transcribed from the web demo is worth being
//! able to find in one place.

use iridium_editor::theme::Theme;

// =============================================================================
// The design values, extracted from the web demo
// =============================================================================
//
// The web demo is the spec for the chrome's shape language; every value below
// cites the style it was read from. Logical values are multiplied by the
// window's scale factor before they reach the pixel grid.

/// Panel corner radius in logical pixels — the web panels' `borderRadius:
/// "8px"`.
pub(super) const PANEL_RADIUS: f32 = 8.0;

/// Horizontal interior padding in logical pixels — the web demo pads every
/// panel row and field `1rem` (16 px) horizontally.
pub(super) const PAD_X: f32 = 16.0;

/// Vertical interior padding in logical pixels — the web input's `0.75rem`
/// (12 px) vertical padding.
pub(super) const PAD_Y: f32 = 12.0;

/// A panel's drop shadow: offset and blur in logical pixels, plus opacity.
///
/// ⭐ **Kept a face constant rather than a theme field, deliberately** (D-5).
/// The shadow and the backdrop dim are *black ink over the page*, not chrome
/// colour: they say "there is a panel in front of this", which is a fact
/// about the face's compositing, not about the palette a theme states. A
/// theme should never have to own them, and one that tried would be stating
/// something it cannot see — how dark the thing underneath happens to be.
#[derive(Debug, Clone, Copy)]
pub(super) struct Shadow {
    /// Vertical offset in logical pixels.
    pub(super) offset_y: f32,
    /// Blur half-width in logical pixels. Near zero gives a hard edge.
    pub(super) blur: f32,
    /// Opacity of the black it is drawn in.
    pub(super) alpha: f32,
}

/// The dark chrome's shadow — the web panels' `boxShadow: "0 16px 48px
/// rgba(0, 0, 0, 0.55)"`, transcribed.
pub(super) const DARK_SHADOW: Shadow = Shadow {
    offset_y: 16.0,
    blur: 48.0,
    alpha: 0.55,
};

/// The light chrome's shadow (D-4): **hard, close and pale**.
///
/// ⚠️ A 48-logical-pixel black smear at 0.55 is a bruise on a grey page. The
/// era's shadow is not a blur at all — a Platinum window casts a solid
/// offset rectangle a few pixels down and right — so the blur drops to 2 and
/// the offset to 3. `RoundedQuad::shadow` already takes blur as a parameter,
/// so a near-hard edge costs nothing and needs no new capability.
pub(super) const LIGHT_SHADOW: Shadow = Shadow {
    offset_y: 3.0,
    blur: 2.0,
    alpha: 0.34,
};

/// The shadow the face draws under a panel in `theme`.
pub(super) const fn shadow_for(theme: &Theme) -> Shadow {
    if theme.is_dark {
        DARK_SHADOW
    } else {
        LIGHT_SHADOW
    }
}

/// The backdrop dim behind the modal top-anchored panels — the web demo's
/// full-screen `backgroundColor: "rgba(0, 0, 0, 0.45)"` behind both the
/// palette and the undo tree. The web demo has no search overlay, and the
/// native search panel gets no backdrop: it must not dim the matches it just
/// highlighted.
pub(super) const DARK_BACKDROP_ALPHA: f32 = 0.45;

/// The light chrome's backdrop dim (D-4).
///
/// ⚠️ 0.45 of black over a `#EFEFEF` page composites to roughly `#838383` —
/// darker than the dark theme's own document background. The modal would be
/// the only thing on screen that inverts when you switch to the light theme.
/// 0.16 dims enough to say "this is behind something" and leaves the page
/// recognisably the page.
pub(super) const LIGHT_BACKDROP_ALPHA: f32 = 0.16;

/// The dim drawn behind a modal panel in `theme`.
pub(super) const fn backdrop_alpha(theme: &Theme) -> f32 {
    if theme.is_dark {
        DARK_BACKDROP_ALPHA
    } else {
        LIGHT_BACKDROP_ALPHA
    }
}

/// How much of the selection's strength the hover band is drawn at.
///
/// ⭐ **The hover is the selection colour, weakened — not a colour of its
/// own.** A separate hue would be a third thing on screen competing with the
/// selection and the caret, and it would have to be chosen again for every
/// theme anyone imports. Weakening the one colour the theme already nominates
/// for "this row" says *this row, provisionally* in whatever palette the theme
/// brought with it.
///
/// 0.4 rather than something fainter because the band is composited onto the
/// panel background rather than blended live: at 0.2 the two presets' selection
/// colours land within a few percent of the background they sit on, which is a
/// highlight nobody can see. At 0.4 it is unmistakably present and still
/// unmistakably not the selection sitting a row away from it.
pub(super) const HOVER_STRENGTH: f32 = 0.4;

/// How far down the window's top edge a top-anchored panel starts — the web
/// backdrop's `paddingTop: "12vh"`, as a fraction of the window height.
pub(super) const TOP_ANCHOR_FRACTION: f32 = 0.12;

/// The selected-row band's corner radius in logical pixels — the web demo's
/// key-hint chip radius (`borderRadius: "4px"`), reused so no sharp corner
/// exists anywhere in the chrome. (The web rows are square-edged but clipped
/// by their panel's `overflow: hidden`; a GPU face has no such clip, so the
/// band carries its own arcs.)
pub(super) const ROW_RADIUS: f32 = 4.0;

/// How far the selected-row band is inset from each panel side, in logical
/// pixels, so its rounded corners never overhang the panel's.
pub(super) const ROW_INSET: f32 = 4.0;

/// The hairline border's width in **physical** pixels — one device pixel,
/// like the web demo's `1px` border on a `devicePixelRatio` display.
///
/// This is the dark chrome's width, and it is also the width of every
/// *interior* rule regardless of theme — see [`frame_width`] for why the two
/// part company.
pub(super) const HAIRLINE: f32 = 1.0;

/// The width, in physical pixels, of the rule that **frames** a panel or
/// closes a strip, for `theme` at `scale`.
///
/// ⚠️ **One physical pixel is a modern idiom, not an era one.** On a Retina
/// display it is half a logical pixel: a rule so fine it reads as a shading
/// rather than an edge. That is right for the dark chrome, which the web demo
/// is the spec for; it is wrong for Platinum, whose frames were 1 px at 1×
/// and therefore *two* physical pixels here (D-4).
///
/// Interior separators keep the physical pixel in both themes. A rule between
/// two rows and a frame around a panel are different objects — the frame says
/// where the panel ends, and the separator only groups what is inside it, so
/// the frame is the one that has to be seen.
pub(super) fn frame_width(theme: &Theme, scale: f32) -> f32 {
    if theme.is_dark {
        HAIRLINE
    } else {
        // `scale` is validated positive and finite before a frame is painted;
        // `max` is belt-and-braces so a frame can never vanish entirely.
        (HAIRLINE * scale).max(HAIRLINE)
    }
}

/// Horizontal padding between the window edge and the strip's text, in
/// logical pixels — the shared `1rem` horizontal scale.
pub(super) const STRIP_PAD_X: f32 = 16.0;

/// Vertical padding above and below the strip's line of text, in logical
/// pixels — the web footer's `0.4rem` vertical padding, the strip's closest
/// web relative.
pub(super) const STRIP_PAD_Y: f32 = 6.0;

/// The caret bar's width in physical pixels.
pub(super) const CARET_WIDTH: f32 = 2.0;

/// The opacity an inactive tab's label is drawn at, composited over the tab
/// strip's background: legible, and plainly behind the tab in front.
pub(super) const TAB_INACTIVE_ALPHA: f32 = 0.55;

/// The opacity a tab's close control is drawn at — quieter still than an
/// inactive label, since it is a control rather than a name.
pub(super) const TAB_CLOSE_ALPHA: f32 = 0.40;

/// The widest a floating panel gets in exterior character columns — the
/// terminal face's own cap, so the two faces' furniture reads the same
/// width class.
pub(super) const PANEL_MAX_COLUMNS: usize = 64;

/// The widest a sidebar gets in exterior character columns.
///
/// Narrower than [`PANEL_MAX_COLUMNS`] on purpose, and for a different reason:
/// a popover is read and dismissed, so it may take the middle of the window,
/// while a sidebar is *kept open beside the code* and every column it takes is
/// a column the document does not get. Thirty-four exterior columns leaves
/// around thirty for names, which fits all but the longest filename at the
/// depths a tree is usually browsed at, and the row truncates with an ellipsis
/// when it does not.
pub(super) const SIDEBAR_COLUMNS: usize = 34;

/// The most of a window's width a sidebar may take.
///
/// ⚠️ **The guard is against a *narrow* window, not a wide one.** On any
/// ordinary display [`SIDEBAR_COLUMNS`] is the binding constraint and this
/// fraction never comes near it; on a half-width window it is what stops the
/// sidebar from taking the document's half of the screen. Below the fraction
/// the fit refuses outright rather than drawing a column too thin to read,
/// and the caller falls back to the popover.
pub(super) const SIDEBAR_MAX_FRACTION: f32 = 0.5;

/// The narrowest exterior worth drawing, in the glyph chrome's
/// exterior-column vocabulary (content plus four border-and-pad columns).
/// Below this a panel shows nothing honestly — a list a dozen characters
/// wide answers no question.
pub(super) const PANEL_MIN_COLUMNS: usize = 20;
