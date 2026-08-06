//! Scroll, the clamp on it, and the window geometry both depend on.
//!
//! `scroll_y` is this face's, in physical pixels, and the compositor answers
//! layout questions about it. Every write to it — the wheel, scroll-to-caret,
//! a resize, a kernel-reported scroll — goes through
//! [`clamp_scroll`](DesktopApp::clamp_scroll), because a clamp applied on
//! three paths out of four is a document that scrolls past its end on the
//! fourth.

use iridium_editor::render::FrameCompositor;

use super::startup::{BASE_FONT_SIZE, FONT};
use super::state::DesktopApp;
use crate::units::{pixel_to_index, scale_to_f32, u32_to_f32};

/// The vertical padding scroll-to-caret keeps between the caret and the
/// viewport edge, matching the compositor's own content padding.
const SCROLL_PADDING: f32 = 10.0;

impl DesktopApp {
    /// Moves the scroll offset under the usual clamp, repainting only when it
    /// actually moved.
    pub(super) fn scroll_by(&mut self, delta_y: f32) {
        let Some(document) = self.workspace.active_payload_mut() else {
            return;
        };
        let before = document.scroll_y;
        document.scroll_y += delta_y;
        // Re-read rather than predict: the clamp is what decides where the
        // scroll actually landed, and a repaint keyed off the unclamped value
        // would fire on every wheel tick at the end of a document.
        self.clamp_scroll();
        let after = self
            .workspace
            .active_payload()
            .map_or(before, |document| document.scroll_y);
        if (after - before).abs() > f32::EPSILON
            && let Some(shell) = &self.shell
        {
            shell.window.request_redraw();
        }
    }

    /// Scrolls so the primary caret is on screen, mirroring the logic the web
    /// face kept when the compositor was extracted: the caret's document-space
    /// Y comes from the compositor, wrap-aware when the caret's line has not
    /// changed; the scroll decision stays here, on the face that owns
    /// `scroll_y`.
    ///
    /// While the search panel is up, the rows it covers at the bottom do not
    /// count as visible — the terminal face shrinks its viewport for the same
    /// reason, so a match is never scrolled to a row the panel that found it
    /// is covering.
    pub(super) fn ensure_caret_visible(&mut self) {
        let search_open = self.search_open;
        let Some(shell) = &self.shell else {
            return;
        };
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };
        let line_height = shell.compositor.line_height();
        let bottom_inset = if search_open {
            shell.overlay.panel_height(2)
        } else {
            0.0
        };
        let viewport_height = (u32_to_f32(shell.surface.height()) - bottom_inset).max(line_height);
        let caret_y = shell
            .compositor
            .cursor_anchor_y(editor, editor.fold_state());

        let Some(document) = self.workspace.active_payload_mut() else {
            return;
        };
        if caret_y < document.scroll_y + SCROLL_PADDING {
            // Caret above the viewport: bring its line to the top.
            document.scroll_y = (caret_y - SCROLL_PADDING).max(0.0);
        } else if caret_y + line_height > document.scroll_y + viewport_height - SCROLL_PADDING {
            // Caret below the viewport: bring its line to the bottom.
            document.scroll_y = caret_y + line_height - viewport_height + SCROLL_PADDING;
        }
        self.clamp_scroll();
    }

    /// Clamps the scroll offset to what the document can honestly show —
    /// the same clamp the web face applies on every scroll write.
    pub(super) fn clamp_scroll(&mut self) {
        let Some(shell) = &self.shell else {
            return;
        };
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };
        let max = shell.compositor.max_scroll_y(
            editor,
            editor.fold_state(),
            u32_to_f32(shell.surface.height()),
        );
        if let Some(document) = self.workspace.active_payload_mut() {
            document.scroll_y = document.scroll_y.clamp(0.0, max);
        }
    }

    /// The document lines (half-open) the next frame will show — the window
    /// the highlight cache scopes its span derive to.
    ///
    /// Fold-aware through the kernel's visual-line mapping, and derived from
    /// the same inputs that drive the compose: the face-owned `scroll_y`, the
    /// surface height and the compositor's line height. Wrap-unaware by
    /// construction — the wrap map belongs to the frame being composed — so
    /// with wrapped lines above the viewport the estimate names later lines
    /// than are truly first on screen; the cache's overscan margin
    /// (`highlight::OVERSCAN_LINES` each way) is what absorbs that slack,
    /// alongside the row of partial-line slack added here. `None` without a
    /// shell: a headless session has no viewport to scope to.
    pub(super) fn viewport_window(&self) -> Option<std::ops::Range<usize>> {
        let shell = self.shell.as_ref()?;
        let line_height = shell.compositor.line_height();
        if line_height <= 0.0 {
            return None;
        }
        let scroll_y = self.workspace.active_payload()?.scroll_y;
        let editor = self.workspace.active_editor()?;
        let first_visual = pixel_to_index(scroll_y / line_height);
        let rows =
            pixel_to_index(u32_to_f32(shell.surface.height()) / line_height).saturating_add(2);
        let first_line = editor.visual_to_document_line(first_visual);
        let last_line = editor.visual_to_document_line(first_visual.saturating_add(rows));
        Some(first_line..last_line.saturating_add(1))
    }

    /// Pushes the surface's geometry into the kernel's viewport.
    ///
    /// Painting never reads that viewport — the compositor is driven by
    /// `scroll_y` — but `cursor.pageUp`/`pageDown` hop by its
    /// `visible_lines`, so it must describe the real window. The `state_mut`
    /// this costs discards sticky columns and any pending chord; it is paid
    /// only on resize and scale change, exactly where the terminal face pays
    /// it.
    pub(super) fn sync_kernel_viewport(&mut self) {
        let Some(shell) = &self.shell else {
            return;
        };
        let line_height = shell.compositor.line_height();
        let width = u32_to_f32(shell.surface.width());
        let height = u32_to_f32(shell.surface.height());
        self.workspace.set_viewport(line_height, width, height);
    }

    /// Reserves the tab strip's height above the document.
    ///
    /// Called wherever the strip's height can have changed — when the window
    /// opens, when it resizes, and when it moves to a display of a different
    /// scale — because the height is the overlay's measured line height plus
    /// scaled padding, and both of those move with the scale factor.
    ///
    /// The reserve is the strip *plus* the document's own breathing room, and
    /// it is what the painter, the hit test, the scroll-to-caret anchor and
    /// the scroll clamp all measure from. See
    /// [`FrameCompositor::set_top_inset`].
    pub(super) fn sync_top_inset(&mut self) {
        let Some(shell) = &mut self.shell else {
            return;
        };
        let height = u32_to_f32(shell.surface.height());
        let strip = shell.overlay.tab_strip_height(height);
        shell
            .compositor
            .set_top_inset(FrameCompositor::DOCUMENT_TOP_PADDING + strip);
    }

    /// Handles a new surface size, in physical pixels.
    pub(super) fn resized(&mut self, width: u32, height: u32) {
        // Zero means minimized, not small; keep the last honest size.
        if width == 0 || height == 0 {
            return;
        }
        if let Some(shell) = &mut self.shell {
            shell.surface.resize(width, height);
            shell
                .compositor
                .resize(shell.surface.queue(), width, height);
            shell.overlay.resize(shell.surface.queue(), width, height);
        }
        self.sync_top_inset();
        self.sync_kernel_viewport();
        self.clamp_scroll();
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }

    /// Handles a scale factor change: refits the font, physical pixels
    /// throughout.
    ///
    /// The font is *reloaded*, not just resized: `set_font_size` deliberately
    /// does not remeasure character width (before any font is loaded there is
    /// nothing honest to measure), and `load_font` is the only remeasuring
    /// path — for the overlay's painter as for the compositor. The matching
    /// `Resized` event carries the new physical dimensions and follows
    /// separately.
    pub(super) fn rescaled(&mut self, scale_factor: f64) {
        if let Some(shell) = &mut self.shell {
            let font_size = BASE_FONT_SIZE * scale_to_f32(scale_factor);
            // Ignored for the reason given where the shell is built: a
            // refusal means the previous, legible size survives, and this
            // face has no console to report it on.
            let _ = shell.compositor.set_font_size(font_size);
            shell.compositor.load_font(FONT.to_vec());
            shell.overlay.set_font(font_size, FONT.to_vec());
            shell.overlay.set_scale(scale_to_f32(scale_factor));
        }
        // After the reload, not before: the strip's height is the overlay's
        // *measured* line height, and `load_font` is the only path that
        // remeasures it.
        self.sync_top_inset();
        self.sync_kernel_viewport();
        if let Some(shell) = &self.shell {
            shell.window.request_redraw();
        }
    }
}
