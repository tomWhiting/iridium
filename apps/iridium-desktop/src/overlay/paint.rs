//! The two render passes that put the chrome on the screen.
//!
//! Everything that touches the GPU lives here: the owned [`TextRenderer`] and
//! its atlas, the rounded-quad instances, and the pass that draws them over
//! the composed document frame. The placement it draws to is
//! [`super::geometry`]'s, and the content it draws is [`super::content`]'s —
//! so both of those stay checkable without a device.

use glyphon::{Buffer, TextBounds};
use iridium_editor::IridiumError;
use iridium_editor::render::{
    FrameTarget, RoundedQuad, RoundedQuadRenderer, RunStyle, TextRenderer,
};
use iridium_editor::theme::{Color, Theme};
use wgpu::{
    CommandEncoderDescriptor, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureFormat,
};

use iridium_panel::line::scroll_for;
use iridium_panel::{PanelFit, Span};

use super::content::{PanelAnchor, PanelContent, PanelKind, StripContent};
use super::frame::{PaintedFrame, PaintedPanel};
use super::geometry::{GridMetrics, PanelGeometry, fit_for, panel_geometry, sidebar_fit_for};
use super::metrics::{
    CARET_WIDTH, HOVER_STRENGTH, PAD_X, PAD_Y, STRIP_PAD_X, STRIP_PAD_Y, TAB_CLOSE_ALPHA,
    TAB_INACTIVE_ALPHA, backdrop_alpha, frame_width, shadow_for,
};
use crate::tab_strip::{
    TabColors, TabStripContent, TabStripLayout, tab_strip_height, tab_strip_layout, tab_strip_spans,
};
use crate::units::{dimension_to_bound, index_to_f32, pixel_to_bound, pixels_to_cells, u32_to_f32};

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

    /// What a window of this size can show of a full-height sidebar down its
    /// left edge, or `None` when it cannot hold one.
    ///
    /// The band starts below the tab strip, measured here rather than passed
    /// in — the same call [`crate::app`] makes to reserve the strip's height
    /// above the document, so the sidebar's top edge and the document's agree
    /// by construction instead of by two callers remembering the same number.
    pub fn sidebar_fit(&mut self, width: u32, height: u32) -> Option<PanelFit> {
        let height = u32_to_f32(height);
        let reserve_top = self.tab_strip_height(height);
        let metrics = self.metrics();
        sidebar_fit_for(u32_to_f32(width), height, reserve_top, metrics)
    }

    /// The width in pixels a panel with `content_columns` characters to a row
    /// takes, padding included — the horizontal twin of
    /// [`Self::panel_height`], and what a face reserves with
    /// [`iridium_editor::render::FrameCompositor::set_left_inset`].
    pub fn panel_width(&mut self, content_columns: usize) -> f32 {
        let metrics = self.metrics();
        index_to_f32(content_columns).mul_add(metrics.char_width, 2.0 * PAD_X * metrics.scale)
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
    /// # `painted`
    ///
    /// Filled with where everything it drew landed — the face's record of the
    /// frame the user is about to click on. ⭐ **Handed back rather than
    /// kept**: the painter cannot exist without a GPU device, and a hit test
    /// that can only be checked against a live device is a hit test nobody
    /// checks. Giving the record to the face leaves exactly one copy of it and
    /// makes the pointer's whole ladder testable without a display.
    ///
    /// An out-parameter rather than a return value because it must survive the
    /// error path: a panel that shaped and placed before a later failure is
    /// still on the record, and the caller decides what a frame that never
    /// presented means.
    ///
    /// # Errors
    ///
    /// Returns an error when glyph preparation or text rendering fails.
    pub fn paint(
        &mut self,
        target: FrameTarget<'_>,
        tabs: Option<&TabStripContent>,
        strip: Option<&StripContent>,
        panels: &[(PanelKind, &PanelContent)],
        theme: &Theme,
        painted: &mut PaintedFrame,
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
        painted.tabs = tab_buffer.as_ref().map(|(_, layout)| layout.clone());

        let strip_buffer = strip
            .and_then(|content| self.shape_strip(content, theme, width_f, height_f, &mut chrome));

        let reserve_bottom = if strip_buffer.is_some() {
            self.strip_height()
        } else {
            0.0
        };
        // Placement is recorded per input panel — `None` for one the window
        // could not hold — so a panel that declined to draw stays
        // distinguishable from one that was never composed, and a face can
        // hit-test exactly what it is looking at.
        let mut placed: Vec<PaintedPanel> = Vec::with_capacity(panels.len());
        let shaped: Vec<ShapedPanel> = panels
            .iter()
            .filter_map(|&(kind, content)| {
                let shaped = self.shape_panel(
                    content,
                    theme,
                    width_f,
                    height_f,
                    reserve_bottom,
                    &mut chrome,
                );
                placed.push(PaintedPanel {
                    kind,
                    geometry: shaped.as_ref().map(|panel| panel.geometry),
                    // The rows this frame *drew*, not the rows the panel has
                    // now: the record answers for the screen the user is
                    // clicking on.
                    rows: content.rows.len(),
                });
                shaped
            })
            .collect();
        painted.panels = placed;

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
        let frame = frame_width(theme, self.scale);
        chrome.push(RoundedQuad::new(
            band.x,
            band.y + band.height - frame,
            band.width,
            frame,
            hairline_color(theme),
            0.0,
        ));

        let spans = tab_strip_spans(content, tab_colors(theme));
        let mut buffer = self.text.create_buffer(None);
        self.text.set_rich_text(
            &mut buffer,
            spans
                .iter()
                .map(|span| (span.text.as_str(), RunStyle::plain(span.color))),
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
            frame_width(theme, self.scale),
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
                Color::new(0.0, 0.0, 0.0, backdrop_alpha(theme)),
                0.0,
            ));
        }
        // The drop shadow: the same rounded shape, offset and blurred.
        let shadow = shadow_for(theme);
        chrome.push(RoundedQuad::shadow(
            exterior.x,
            shadow.offset_y.mul_add(self.scale, exterior.y),
            exterior.width,
            exterior.height,
            Color::new(0.0, 0.0, 0.0, shadow.alpha),
            exterior.radius,
            shadow.blur * self.scale,
        ));
        // The frame: the exterior in the border colour, with the opaque
        // background inset over it by the frame's own width.
        let frame = frame_width(theme, self.scale);
        chrome.push(RoundedQuad::new(
            exterior.x,
            exterior.y,
            exterior.width,
            exterior.height,
            hairline_color(theme),
            exterior.radius,
        ));
        chrome.push(RoundedQuad::new(
            exterior.x + frame,
            exterior.y + frame,
            2.0_f32.mul_add(-frame, exterior.width),
            2.0_f32.mul_add(-frame, exterior.height),
            panel_background(theme),
            (exterior.radius - frame).max(0.0),
        ));
        // Before the selection band and after the background, so a pointer
        // resting on the selected row shows the selection rather than a
        // fainter wash over it — the row is still the selection, and the hover
        // has nothing to add once the two coincide.
        if let Some(index) = panel.hovered
            && index < panel.rows.len()
            && !panel.rows[index].separator
        {
            let band = geometry.selected_row_rect(index);
            chrome.push(RoundedQuad::new(
                band.x,
                band.y,
                band.width,
                band.height,
                hover_color(theme),
                band.radius,
            ));
        }
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
            spans
                .iter()
                .map(|span| (span.text.as_str(), RunStyle::plain(span.color))),
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
pub(super) fn panel_spans(panel: &PanelContent, base: Color) -> Vec<Span> {
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
pub(super) fn panel_background(theme: &Theme) -> Color {
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
pub(super) fn strip_background(theme: &Theme) -> Color {
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
pub(super) fn tab_card_color(theme: &Theme) -> Color {
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
pub(super) fn tab_colors(theme: &Theme) -> TabColors {
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

/// The hairline border colour: the theme's own `panel_border`, composited
/// opaque over the panel background.
///
/// ⭐ **The alpha is the theme's, not the face's.** This used to derive the
/// border as `foreground` at a face constant of 0.18, which quietly asserted
/// that every theme wants the same strength of frame. The classic-Mac light
/// presets disagree — 0.55 for Platinum and Paper, 0.85 for Monochrome, whose
/// page has no tone left to hold a panel off it — so D-5 made the border a
/// stated `EditorColors` field. The compositing stays here, because whether
/// a panel floats over the document or over another panel is the face's fact,
/// not the theme's.
pub(super) fn hairline_color(theme: &Theme) -> Color {
    composite(theme.editor.panel_border, panel_background(theme))
}

/// The band drawn behind the row the pointer is resting on: the theme's own
/// selection colour at [`HOVER_STRENGTH`], composited onto the panel's
/// background so it is opaque like every other quad in the chrome.
///
/// A theme whose selection colour is fully transparent gets no hover band
/// rather than an invisible one — the composite would return the background
/// exactly, and pushing a quad that draws nothing is a quad that costs the
/// same as one that does.
pub(super) fn hover_color(theme: &Theme) -> Color {
    let selection = theme.editor.selection;
    let over = Color::new(
        selection.r,
        selection.g,
        selection.b,
        selection.a * HOVER_STRENGTH,
    );
    composite(over, panel_background(theme))
}

/// `over` alpha-composited onto an opaque `under`.
pub(super) fn composite(over: Color, under: Color) -> Color {
    let blend = |source: f32, target: f32| over.a.mul_add(source, (1.0 - over.a) * target);
    Color::new(
        blend(over.r, under.r),
        blend(over.g, under.g),
        blend(over.b, under.b),
        1.0,
    )
}

