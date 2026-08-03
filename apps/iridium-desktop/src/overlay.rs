//! The painted overlays: the prompt strip and the floating panels.
//!
//! The strip was the first painted overlay, and it deliberately established
//! the pattern the bigger overlays reuse: after
//! [`FrameCompositor::compose`](iridium_editor::render::FrameCompositor)
//! has drawn the document into the surface's texture view, a **second render
//! pass** runs on the same view with [`LoadOp::Load`] — the composed frame is
//! kept, not cleared — drawing background quads and text over it. The
//! primitives are the kernel's own render types,
//! [`QuadRenderer`] and [`TextRenderer`], publicly exported from
//! `iridium_editor::render` precisely so a face paints its overlays with the
//! renderers the kernel already trusts instead of forking text rendering.
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
//! 3. Draw quads first (backgrounds, selected rows, carets), then the text.
//! 4. Submit. The face's `render_frame` closure presents afterwards, so the
//!    document pass and the overlay pass reach the screen as one frame.
//!
//! # Panels are boxes with rounded corners, on a character grid
//!
//! [`QuadRenderer`] draws sharp rectangles, and this face's boxes are never
//! sharp — Tom's standing ruling. So a panel's border is *text*: the arc
//! corners and light lines `╭ ─ ╮ │ ╰ ╯`, the same visual language the
//! terminal face draws its palette and undo-tree boxes in, laid on the
//! `char × char_width` grid the compositor already uses. The background quad
//! is inset half a cell on every side so the quad's own sharp corners sit
//! behind the arc glyphs and the text never touches the quad edge. Selected
//! rows are a translucent selection-coloured quad behind the row's text —
//! the same statement the terminal face makes with its selected-row
//! background. A [`PanelContent`] is therefore rows of coloured character
//! runs ([`Span`]), which the builders in [`crate::search`],
//! [`crate::command_palette`] and [`crate::history_overlay`] compose and this
//! module paints; the builders stay windowless and testable.
//!
//! # Geometry
//!
//! The strip is one line high plus padding, pinned to the bottom edge, full
//! width. A panel is horizontally centred, at most [`PANEL_MAX_COLUMNS`]
//! characters wide, anchored either one line down from the top (palette,
//! undo tree) or to the bottom edge above the strip (search) — and it
//! declines to exist at all on a window too small for an honest panel,
//! exactly as the terminal face's `FloatingBox` declines.

use glyphon::{Buffer, TextBounds};
use iridium_editor::IridiumError;
use iridium_editor::render::{FrameTarget, Quad, QuadRenderer, TextRenderer};
use iridium_editor::theme::{Color, Theme};
use wgpu::{
    CommandEncoderDescriptor, Device, LoadOp, Operations, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureFormat,
};

use crate::units::{dimension_to_bound, index_to_f32, pixel_to_bound, pixels_to_cells, u32_to_f32};

/// Horizontal padding between the window edge and the strip's text.
const TEXT_PAD: f32 = 10.0;

/// Vertical padding above and below the strip's line of text.
const STRIP_PAD: f32 = 6.0;

/// The caret bar's width in physical pixels.
const CARET_WIDTH: f32 = 2.0;

/// The widest a floating panel gets, borders included — the terminal face's
/// own cap, so the two faces' furniture reads the same.
const PANEL_MAX_COLUMNS: usize = 64;

/// The narrowest exterior worth drawing. Below this a panel shows nothing
/// honestly — a list four characters wide answers no question.
const PANEL_MIN_COLUMNS: usize = 20;

/// The rows a top-anchored panel starts below the window's top edge, so it
/// reads as floating rather than as a title bar.
const PANEL_TOP: usize = 1;

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
/// The spans read left to right; the painter pads the row with spaces to the
/// panel's content width and truncates anything past it, so a builder that
/// measured correctly is drawn exactly and a builder mistake costs alignment,
/// never a panic or an overrun border.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelRow {
    /// The row's text runs, left to right.
    pub spans: Vec<Span>,
    /// Whether the row is the selection, which tints its background.
    pub selected: bool,
}

impl PanelRow {
    /// A row from its runs.
    #[must_use]
    pub const fn new(spans: Vec<Span>) -> Self {
        Self {
            spans,
            selected: false,
        }
    }

    /// A selection row from its runs.
    #[must_use]
    pub const fn selected(spans: Vec<Span>) -> Self {
        Self {
            spans,
            selected: true,
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

/// Which window edge a panel hangs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelAnchor {
    /// One line down from the top, horizontally centred — the palette and the
    /// undo tree.
    Top,
    /// Above the bottom edge (and above the strip when one is up),
    /// horizontally centred — the search panel, which belongs near the
    /// statusline row it occupies in the terminal face.
    Bottom,
}

/// A panel composed and ready to paint: rows of coloured runs on the
/// character grid, plus where its caret goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelContent {
    /// Which edge the panel hangs from.
    pub anchor: PanelAnchor,
    /// The characters available to each row between the borders and pads.
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

/// Paints the overlays. One instance belongs to one surface, exactly as the
/// compositor does; its renderers' viewport uniforms track that surface.
pub struct OverlayPainter {
    /// The kernel's text renderer, owned by this overlay: its own font
    /// system, atlas and prepared-area slot.
    text: TextRenderer,
    /// The kernel's quad renderer for backgrounds, selections and carets.
    quads: QuadRenderer,
}

impl std::fmt::Debug for OverlayPainter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OverlayPainter").finish_non_exhaustive()
    }
}

/// One panel shaped into a buffer, waiting for the shared prepare.
struct ShapedPanel {
    /// The shaped text, borders included.
    buffer: Buffer,
    /// The panel's left edge in physical pixels.
    x: f32,
    /// The panel's top edge in physical pixels.
    y: f32,
    /// The panel's exterior width in physical pixels.
    width: f32,
    /// The panel's exterior height in physical pixels.
    height: f32,
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
        let quads = QuadRenderer::new(device, format);
        let mut painter = Self { text, quads };
        painter.resize(queue, width, height);
        Ok(painter)
    }

    /// Sets the overlay font, size first so the load remeasures at it —
    /// the same order the compositor requires, for the same reason:
    /// `load_font` is the only remeasuring path.
    pub fn set_font(&mut self, size: f32, data: Vec<u8>) {
        self.text.set_font_size(size);
        self.text.load_font(data);
    }

    /// Tracks a surface resize, in physical pixels.
    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32) {
        self.text.update_viewport(queue, width, height);
        self.quads.update_viewport(queue, width, height);
    }

    /// The height in pixels the strip occupies when it is up.
    pub fn strip_height(&self) -> f32 {
        2.0_f32.mul_add(STRIP_PAD, self.text.line_height())
    }

    /// The height in pixels a panel with `interior_rows` content rows takes,
    /// borders included.
    pub fn panel_height(&self, interior_rows: usize) -> f32 {
        index_to_f32(interior_rows + 2) * self.text.line_height()
    }

    /// What a window of this size can honestly show of a panel, or `None`
    /// when it cannot show one at all — the terminal face's `FloatingBox`
    /// rule, restated on the pixel grid.
    ///
    /// The keys of an undrawable panel keep working regardless; declining to
    /// draw must never trap `Escape`.
    pub fn panel_fit(&mut self, width: u32, height: u32) -> Option<PanelFit> {
        let char_width = self.text.char_width();
        let line_height = self.text.line_height();
        let columns = pixels_to_cells(u32_to_f32(width), char_width);
        let rows = pixels_to_cells(u32_to_f32(height), line_height);
        let exterior = columns.min(PANEL_MAX_COLUMNS);
        if exterior < PANEL_MIN_COLUMNS || rows < PANEL_TOP + 3 {
            return None;
        }
        Some(PanelFit {
            content_columns: exterior - 4,
            max_interior_rows: rows - PANEL_TOP - 2,
        })
    }

    /// Paints the overlays over an already-composed frame: the strip, then
    /// every open panel in the order given, later panels on top.
    ///
    /// Runs the second render pass described in the module documentation.
    /// A window shorter than the strip paints no strip; a panel that no
    /// longer fits paints nothing rather than a sliver.
    ///
    /// # Errors
    ///
    /// Returns an error when glyph preparation or text rendering fails.
    pub fn paint(
        &mut self,
        target: FrameTarget<'_>,
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
        let mut quads: Vec<Quad> = Vec::new();

        let strip_buffer = strip
            .and_then(|content| self.shape_strip(content, theme, width_f, height_f, &mut quads));

        let reserve_bottom = if strip_buffer.is_some() {
            self.strip_height()
        } else {
            0.0
        };
        let shaped: Vec<ShapedPanel> = panels
            .iter()
            .filter_map(|panel| {
                self.shape_panel(panel, theme, width_f, height_f, reserve_bottom, &mut quads)
            })
            .collect();

        if quads.is_empty() && strip_buffer.is_none() && shaped.is_empty() {
            return Ok(());
        }

        let mut areas = Vec::new();
        if let Some((buffer, scroll_x, top, color)) = &strip_buffer {
            areas.push(TextRenderer::create_text_area(
                buffer,
                TEXT_PAD - scroll_x,
                *top,
                1.0,
                TextBounds {
                    // Clipped at the padding so a scrolled field's cut glyphs
                    // vanish instead of bleeding under the window edge.
                    left: pixel_to_bound(TEXT_PAD),
                    top: pixel_to_bound(*top - STRIP_PAD),
                    right: dimension_to_bound(width),
                    bottom: dimension_to_bound(height),
                },
                *color,
            ));
        }
        for panel in &shaped {
            areas.push(TextRenderer::create_text_area(
                &panel.buffer,
                panel.x,
                panel.y,
                1.0,
                TextBounds {
                    left: pixel_to_bound(panel.x),
                    top: pixel_to_bound(panel.y),
                    right: pixel_to_bound(panel.x + panel.width),
                    bottom: pixel_to_bound(panel.y + panel.height),
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
            self.quads.render(&mut pass, queue, &quads);
            self.text.render(&mut pass)?;
        }
        queue.submit(std::iter::once(encoder.finish()));

        self.text.trim_cache();
        Ok(())
    }

    /// Shapes the strip and pushes its quads, or `None` on a window too small
    /// for it. Returns the buffer with its scroll, top edge and colour.
    fn shape_strip(
        &mut self,
        content: &StripContent,
        theme: &Theme,
        width_f: f32,
        height_f: f32,
        quads: &mut Vec<Quad>,
    ) -> Option<(Buffer, f32, f32, Color)> {
        let line_height = self.text.line_height();
        let strip_height = 2.0_f32.mul_add(STRIP_PAD, line_height);
        if strip_height > height_f || width_f <= 2.0 * TEXT_PAD {
            return None;
        }
        let strip_top = height_f - strip_height;
        let char_width = self.text.char_width();

        // Scroll the field just enough to keep the caret on screen.
        let cells = pixels_to_cells(2.0_f32.mul_add(-TEXT_PAD, width_f), char_width);
        let scroll_cells = content
            .caret_column
            .map_or(0, |caret| scroll_for(caret, cells));
        let scroll_x = index_to_f32(scroll_cells) * char_width;

        // Background first, caret over it, text on top of both.
        quads.push(Quad::new(
            0.0,
            strip_top,
            width_f,
            strip_height,
            theme.editor.gutter,
        ));
        if let Some(caret) = content.caret_column {
            if caret >= scroll_cells {
                let caret_x = index_to_f32(caret - scroll_cells).mul_add(char_width, TEXT_PAD);
                quads.push(Quad::new(
                    caret_x,
                    strip_top + STRIP_PAD,
                    CARET_WIDTH,
                    line_height,
                    theme.editor.cursor,
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
        Some((buffer, scroll_x, strip_top + STRIP_PAD, color))
    }

    /// Shapes one panel and pushes its quads, or `None` when the window can
    /// no longer show it honestly.
    fn shape_panel(
        &mut self,
        panel: &PanelContent,
        theme: &Theme,
        width_f: f32,
        height_f: f32,
        reserve_bottom: f32,
        quads: &mut Vec<Quad>,
    ) -> Option<ShapedPanel> {
        let char_width = self.text.char_width();
        let line_height = self.text.line_height();
        if char_width <= 0.0 || line_height <= 0.0 || panel.rows.is_empty() {
            return None;
        }

        let exterior_columns = panel.content_columns + 4;
        let rows_total = panel.rows.len() + 2;
        let panel_width = index_to_f32(exterior_columns) * char_width;
        let panel_height = index_to_f32(rows_total) * line_height;
        if panel_width > width_f || panel_height > height_f {
            return None;
        }

        let x = ((width_f - panel_width) / 2.0).max(0.0);
        let y = match panel.anchor {
            PanelAnchor::Top => index_to_f32(PANEL_TOP) * line_height,
            PanelAnchor::Bottom => (height_f - reserve_bottom - panel_height).max(0.0),
        };

        // The background quad is inset half a cell so its sharp corners stay
        // behind the arc glyphs and text never touches its edge.
        quads.push(Quad::new(
            char_width.mul_add(0.5, x),
            line_height.mul_add(0.5, y),
            panel_width - char_width,
            panel_height - line_height,
            panel_background(theme),
        ));
        for (index, row) in panel.rows.iter().enumerate() {
            if row.selected {
                quads.push(Quad::new(
                    x + char_width,
                    index_to_f32(index + 1).mul_add(line_height, y),
                    2.0_f32.mul_add(-char_width, panel_width),
                    line_height,
                    theme.editor.selection,
                ));
            }
        }
        if let Some(caret) = panel.caret {
            if caret.row < panel.rows.len() && caret.column <= panel.content_columns {
                quads.push(Quad::new(
                    index_to_f32(caret.column + 2).mul_add(char_width, x),
                    index_to_f32(caret.row + 1).mul_add(line_height, y),
                    CARET_WIDTH,
                    line_height,
                    theme.editor.cursor,
                ));
            }
        }

        let border = theme.editor.foreground;
        let spans = panel_text(panel, border, exterior_columns);
        let mut buffer = self.text.create_buffer(None);
        self.text.set_rich_text(
            &mut buffer,
            spans.iter().map(|span| (span.text.as_str(), span.color)),
        );
        self.text.shape_buffer(&mut buffer);
        Some(ShapedPanel {
            buffer,
            x,
            y,
            width: panel_width,
            height: panel_height,
            color: border,
        })
    }
}

/// The panel's text, borders included, as one run list with newlines.
///
/// Rows are padded with spaces to the content width and truncated past it, so
/// the right border always lands on its column.
fn panel_text(panel: &PanelContent, border: Color, exterior_columns: usize) -> Vec<Span> {
    let horizontal = "─".repeat(exterior_columns.saturating_sub(2));
    let mut spans = Vec::with_capacity(panel.rows.len() * 4 + 2);
    spans.push(Span::new(format!("╭{horizontal}╮\n"), border));
    for row in &panel.rows {
        spans.push(Span::new("│ ", border));
        let mut used = 0;
        for span in &row.spans {
            if used >= panel.content_columns {
                break;
            }
            let budget = panel.content_columns - used;
            let text = truncate_chars(&span.text, budget);
            used += text.chars().count();
            if !text.is_empty() {
                spans.push(Span::new(text, span.color));
            }
        }
        if used < panel.content_columns {
            spans.push(Span::new(" ".repeat(panel.content_columns - used), border));
        }
        spans.push(Span::new(" │\n", border));
    }
    spans.push(Span::new(format!("╰{horizontal}╯"), border));
    spans
}

/// The first `budget` characters of `text`, on a character boundary.
fn truncate_chars(text: &str, budget: usize) -> String {
    text.chars().take(budget).collect()
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
    use iridium_editor::theme::Theme;

    use super::{
        Color, PanelAnchor, PanelContent, PanelRow, Span, panel_background, panel_text, scroll_for,
        truncate_chars,
    };

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
    fn panel_text_draws_a_rounded_box_around_padded_rows() {
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        let panel = PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: 6,
            rows: vec![PanelRow::new(vec![Span::new("ab", white)])],
            caret: None,
        };
        let text: String = panel_text(&panel, white, 10)
            .iter()
            .map(|span| span.text.as_str())
            .collect();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "╭────────╮");
        assert_eq!(lines[1], "│ ab     │");
        assert_eq!(lines[2], "╰────────╯");
        assert!(
            !text.contains(['┌', '┐', '└', '┘']),
            "no square corner may appear anywhere"
        );
    }

    #[test]
    fn a_row_wider_than_the_panel_is_truncated_at_the_border() {
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        let panel = PanelContent {
            anchor: PanelAnchor::Top,
            content_columns: 4,
            rows: vec![PanelRow::new(vec![Span::new("abcdefgh", white)])],
            caret: None,
        };
        let text: String = panel_text(&panel, white, 8)
            .iter()
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(text.lines().nth(1), Some("│ abcd │"));
    }

    #[test]
    fn truncation_counts_characters_not_bytes() {
        assert_eq!(truncate_chars("héllo", 2), "hé");
        assert_eq!(truncate_chars("ab", 5), "ab");
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
}
