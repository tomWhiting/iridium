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

//!
//! # Where the pieces live
//!
//! Split along the axis a defect appears on, not by size: a placement bug is
//! arithmetic, a chrome bug is a pass, and a composition bug is neither.
//!
//! - [`metrics`] — the design values, and the two theme-dependent choices.
//! - [`content`] — the vocabulary a panel is composed in. No pixels.
//! - [`geometry`] — where a composed panel lands. A pure function, no device.
//! - [`paint`] — the render passes. Everything that touches the GPU.

mod content;
mod geometry;
mod metrics;
mod paint;
#[cfg(test)]
mod tests;

pub use content::{PanelAnchor, PanelCaret, PanelContent, PanelFit, PanelRow, Span, StripContent};
pub use geometry::{GridMetrics, PanelGeometry, PanelRect, panel_geometry};
pub use metrics::{EXPLORER_MAX_VISIBLE_ROWS, PANEL_MAX_VISIBLE_ROWS};
pub use paint::OverlayPainter;
pub(crate) use paint::scroll_for;
