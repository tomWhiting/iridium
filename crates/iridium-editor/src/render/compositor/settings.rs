//! Host-facing state pushed in between frames: fonts, theme, toggles and the
//! presentation inputs a face layers over the document.
//!
//! Several of these bump a generation counter that feeds the retained-shaping
//! key. That is not bookkeeping — it is the whole correctness argument for the
//! cache: a setter that changes what a frame would draw and does not move a
//! generation serves a stale frame forever.

use std::collections::HashMap;

use wgpu::Queue;

use super::state::FrameCompositor;
use crate::theme::{Color, Theme};

impl FrameCompositor {
    /// Loads a font from raw TTF/OTF data and remeasures the character
    /// width, which every layout answer above depends on.
    pub fn load_font(&mut self, data: Vec<u8>) {
        self.text_renderer.load_font(data);
        // Measure actual character width from the loaded font
        self.cached_char_width = self.text_renderer.char_width();
        // New font data can change how Family::Monospace resolves — every
        // retained shape is stale.
        self.font_generation = self.font_generation.wrapping_add(1);
    }

    /// Sets the font size in pixels (already scaled for DPI by the face).
    ///
    /// Remeasures [`Self::char_width`], because it is a measurement *at a
    /// size* and this call changes the size. Everything positional reads that
    /// number — column to x, x back to column on the mouse path, the gutter's
    /// width — so a stale one misplaces the caret by an error that grows with
    /// the column rather than by a constant anybody would notice.
    ///
    /// ⚠️ An earlier revision deliberately did *not* remeasure, on the grounds
    /// that "before a font is loaded there is nothing honest to measure". The
    /// premise was right and the conclusion did not follow: the honest answer
    /// for an unloaded font is not the *previous size's* width, it is this
    /// size's fallback, and
    /// [`TextRenderer::char_width`](crate::render::TextRenderer::char_width)
    /// already produces exactly that — it falls back to `font_size * 0.6` when
    /// shaping yields no glyphs. The protection was already one layer down, so
    /// declining to remeasure bought nothing and cost correctness on the path
    /// that changes size with a font already loaded. That path is what a
    /// display change is: see `docs/IN-FLIGHT-35-hidpi.md`.
    ///
    /// Deliberately does **not** move `font_generation`. That counter exists
    /// for inputs the retained-shaping key cannot see for itself, and the key
    /// carries the font size in its own right (`ShapeKey::font_size`, in the
    /// sibling `shape` module), so a size change is already a miss. Bumping
    /// here would invalidate nothing extra and would make the counter mean two
    /// things.
    ///
    /// Returns whether the size was accepted; a rejected size leaves the
    /// previous one in place — and, with it, the previous width, since the
    /// remeasure happens only on the accepting branch. The bound is
    /// [`TextRenderer::set_font_size`](crate::render::TextRenderer::set_font_size)'s,
    /// and the reason it is forwarded rather than swallowed is that a face
    /// which scales the font by a display factor is exactly the caller that
    /// can supply a degenerate one and wants to know.
    pub fn set_font_size(&mut self, size: f32) -> bool {
        if !self.text_renderer.set_font_size(size) {
            return false;
        }
        self.cached_char_width = self.text_renderer.char_width();
        true
    }

    /// Updates the renderers' viewport uniforms for a resized surface.
    ///
    /// Also updates the dimension cache so [`Self::compose`] does not
    /// redundantly re-upload the uniforms on its next frame.
    pub fn resize(&mut self, queue: &Queue, width: u32, height: u32) {
        self.text_renderer.update_viewport(queue, width, height);
        self.background_quad_renderer
            .update_viewport(queue, width, height);
        self.cursor_quad_renderer
            .update_viewport(queue, width, height);
        // Update cache to prevent redundant updates in compose
        self.cached_viewport_width = width;
        self.cached_viewport_height = height;
    }

    /// Restarts the caret blink cycle so the caret is visible immediately.
    ///
    /// Every input path that moves a caret calls this: a caret that stays in
    /// its off-phase across a movement looks like a caret that vanished.
    pub fn reset_blink(&mut self) {
        self.cursor_renderer.reset_blink();
    }

    /// Switches between the built-in dark and light themes, keeping the
    /// fallback highlighter's palette in step.
    pub fn set_dark_theme(&mut self, dark: bool) {
        self.set_theme(if dark { Theme::dark() } else { Theme::light() });
    }

    /// Replaces the active theme wholesale, keeping the fallback
    /// highlighter's palette in step — the path for a face whose editor
    /// holds a theme that is not one of the built-in presets.
    pub fn set_theme(&mut self, theme: Theme) {
        // The keyword bridge's palette is its own type, keyed to darkness
        // rather than to the theme's syntax colors.
        if theme.is_dark {
            self.highlighter.set_dark_theme();
        } else {
            self.highlighter.set_light_theme();
        }
        self.theme = theme;
        // Text colors and the fallback highlighter's palette both feed the
        // shaped buffers; a set_theme that skipped this bump would serve
        // stale-colored retained frames.
        self.theme_generation = self.theme_generation.wrapping_add(1);
    }

    /// The active theme, for hosts that derive colors from it (e.g. gutter
    /// change-bar kinds).
    pub const fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Enables or disables syntax highlighting.
    pub const fn set_syntax_enabled(&mut self, enabled: bool) {
        self.syntax_enabled = enabled;
    }

    /// Whether syntax highlighting is enabled.
    pub const fn syntax_enabled(&self) -> bool {
        self.syntax_enabled
    }

    /// Enables or disables the gutter (line numbers).
    ///
    /// Bumps the gutter generation unconditionally: toggling changes both
    /// the gutter buffer's existence and the content column's width, and a
    /// redundant call merely costs one rebuilt frame.
    pub const fn set_gutter_enabled(&mut self, enabled: bool) {
        self.gutter_enabled = enabled;
        self.gutter_text_generation = self.gutter_text_generation.wrapping_add(1);
    }

    /// Whether the gutter is enabled.
    pub const fn gutter_enabled(&self) -> bool {
        self.gutter_enabled
    }

    /// The capture-name → color map the face's highlight resolver reads,
    /// mutable so the host can replace it in place.
    ///
    /// Exposed as the raw map — rather than a setter taking a finished map —
    /// so a host that fails partway through parsing its theme leaves exactly
    /// the partial state it always did.
    ///
    /// Every mutable borrow bumps the syntax-theme generation: the raw
    /// accessor defeats change tracking, so the borrow itself is the
    /// change signal. It over-invalidates only when the host actually
    /// calls this, which coincides with real theme changes.
    pub const fn syntax_theme_mut(&mut self) -> &mut HashMap<String, Color> {
        self.syntax_theme_generation = self.syntax_theme_generation.wrapping_add(1);
        &mut self.syntax_theme
    }

    /// Per-line background colors (`doc_line` → color) for diff
    /// highlighting, mutable for in-place host updates.
    pub const fn line_backgrounds_mut(&mut self) -> &mut HashMap<usize, Color> {
        &mut self.line_backgrounds
    }

    /// Per-line gutter change bar colors (`doc_line` → color), mutable for
    /// in-place host updates.
    pub const fn gutter_changes_mut(&mut self) -> &mut HashMap<usize, Color> {
        &mut self.gutter_changes
    }

    /// Replaces the custom gutter text: one string per document line, or
    /// `None` to restore automatic line numbers.
    ///
    /// Bumps the gutter generation unconditionally — custom gutter text
    /// changes both the gutter buffer's text and (through its measured
    /// width) the content column — which also makes this call the honest
    /// way for a host to force a full rebuild of the retained shapes.
    pub fn set_custom_gutter_lines(&mut self, lines: Option<Vec<String>>) {
        self.custom_gutter_lines = lines;
        self.gutter_text_generation = self.gutter_text_generation.wrapping_add(1);
    }

    /// Per-line blame text (`doc_line` → formatted string), mutable for
    /// in-place host updates. Only the cursor line's entry is rendered.
    pub const fn blame_data_mut(&mut self) -> &mut HashMap<usize, String> {
        &mut self.blame_data
    }
}
