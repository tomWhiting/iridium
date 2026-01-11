//! Line number gutter rendering.
//!
//! Renders line numbers in a gutter area to the left of the editor content.

/// Width calculation for the line number gutter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GutterWidth {
    /// Total width of the gutter in pixels.
    pub total: f32,
    /// Width of the line numbers area in pixels.
    pub line_numbers: f32,
    /// Padding on the left side.
    pub padding_left: f32,
    /// Padding on the right side (between numbers and content).
    pub padding_right: f32,
}

impl GutterWidth {
    /// Creates a new gutter width with the given parameters.
    #[must_use]
    pub fn new(line_numbers: f32, padding_left: f32, padding_right: f32) -> Self {
        Self {
            total: line_numbers + padding_left + padding_right,
            line_numbers,
            padding_left,
            padding_right,
        }
    }

    /// Returns the x-coordinate where line numbers should be drawn (right-aligned).
    #[must_use]
    pub fn line_number_x(&self) -> f32 {
        self.padding_left
    }

    /// Returns the x-coordinate where the content area begins.
    #[must_use]
    pub fn content_start_x(&self) -> f32 {
        self.total
    }
}

/// Configuration for the line number gutter.
#[derive(Debug, Clone)]
pub struct GutterConfig {
    /// Whether to show line numbers.
    pub show_line_numbers: bool,
    /// Minimum number of digits to display (affects gutter width).
    pub min_digits: usize,
    /// Left padding in pixels.
    pub padding_left: f32,
    /// Right padding in pixels.
    pub padding_right: f32,
    /// Line number color (RGBA, 0.0-1.0).
    pub line_number_color: [f32; 4],
    /// Current line number color (RGBA, 0.0-1.0).
    pub current_line_number_color: [f32; 4],
    /// Gutter background color (RGBA, 0.0-1.0).
    pub background_color: [f32; 4],
    /// Gutter separator color (RGBA, 0.0-1.0).
    pub separator_color: [f32; 4],
    /// Whether to show a separator line between gutter and content.
    pub show_separator: bool,
}

impl Default for GutterConfig {
    fn default() -> Self {
        Self {
            show_line_numbers: true,
            min_digits: 3,
            padding_left: 8.0,
            padding_right: 12.0,
            line_number_color: [0.5, 0.5, 0.5, 1.0],        // Gray
            current_line_number_color: [0.9, 0.9, 0.9, 1.0], // Bright gray
            background_color: [0.10, 0.10, 0.10, 1.0],       // Darker than editor
            separator_color: [0.2, 0.2, 0.2, 1.0],           // Subtle separator
            show_separator: true,
        }
    }
}

impl GutterConfig {
    /// Creates a dark theme gutter configuration.
    #[must_use]
    pub fn dark_theme() -> Self {
        Self::default()
    }

    /// Creates a light theme gutter configuration.
    #[must_use]
    pub fn light_theme() -> Self {
        Self {
            line_number_color: [0.5, 0.5, 0.5, 1.0],
            current_line_number_color: [0.2, 0.2, 0.2, 1.0],
            background_color: [0.95, 0.95, 0.95, 1.0],
            separator_color: [0.85, 0.85, 0.85, 1.0],
            ..Default::default()
        }
    }

    /// Sets the line number color.
    #[must_use]
    pub fn with_line_number_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.line_number_color = [r, g, b, a];
        self
    }

    /// Sets the current line number color.
    #[must_use]
    pub fn with_current_line_number_color(mut self, r: f32, g: f32, b: f32, a: f32) -> Self {
        self.current_line_number_color = [r, g, b, a];
        self
    }
}

/// A line number to be rendered.
#[derive(Debug, Clone)]
pub struct LineNumberEntry {
    /// The line number (1-based for display).
    pub number: usize,
    /// The Y coordinate for this line number.
    pub y: f32,
    /// Whether this is the current line.
    pub is_current: bool,
    /// The color to use for this line number.
    pub color: [f32; 4],
}

/// Renders line numbers in the gutter area.
#[derive(Debug)]
pub struct GutterRenderer {
    /// Configuration.
    config: GutterConfig,
    /// Font metrics.
    char_width: f32,
    line_height: f32,
    /// Total number of lines (affects gutter width calculation).
    total_lines: usize,
    /// Cached gutter width.
    cached_width: GutterWidth,
}

impl GutterRenderer {
    /// Creates a new gutter renderer.
    #[must_use]
    pub fn new(config: GutterConfig) -> Self {
        let mut renderer = Self {
            config,
            char_width: 8.0,
            line_height: 20.0,
            total_lines: 1,
            cached_width: GutterWidth::new(24.0, 8.0, 12.0),
        };
        renderer.update_cached_width();
        renderer
    }

    /// Returns the configuration.
    #[must_use]
    pub fn config(&self) -> &GutterConfig {
        &self.config
    }

    /// Returns a mutable reference to the configuration.
    pub fn config_mut(&mut self) -> &mut GutterConfig {
        &mut self.config
    }

    /// Sets font metrics.
    pub fn set_font_metrics(&mut self, char_width: f32, line_height: f32) {
        self.char_width = char_width;
        self.line_height = line_height;
        self.update_cached_width();
    }

    /// Sets the total number of lines.
    pub fn set_total_lines(&mut self, lines: usize) {
        let old_digits = self.digit_count(self.total_lines);
        self.total_lines = lines.max(1);
        let new_digits = self.digit_count(self.total_lines);

        // Only recalculate width if digit count changed
        if old_digits != new_digits {
            self.update_cached_width();
        }
    }

    /// Returns the current gutter width.
    #[must_use]
    pub fn width(&self) -> &GutterWidth {
        &self.cached_width
    }

    /// Returns the total gutter width in pixels.
    #[must_use]
    pub fn total_width(&self) -> f32 {
        if self.config.show_line_numbers {
            self.cached_width.total
        } else {
            0.0
        }
    }

    /// Updates the cached gutter width based on total lines.
    fn update_cached_width(&mut self) {
        let digits = self.digit_count(self.total_lines).max(self.config.min_digits);
        let line_number_width = digits as f32 * self.char_width;
        self.cached_width = GutterWidth::new(
            line_number_width,
            self.config.padding_left,
            self.config.padding_right,
        );
    }

    /// Returns the number of digits needed to display a number.
    fn digit_count(&self, n: usize) -> usize {
        if n == 0 {
            1
        } else {
            (n as f64).log10().floor() as usize + 1
        }
    }

    /// Generates line number entries for the visible range.
    ///
    /// `first_line` and `last_line` are 0-indexed line numbers.
    /// `current_line` is the line containing the cursor (0-indexed).
    /// `scroll_y` is the current vertical scroll offset.
    #[must_use]
    pub fn generate_line_numbers(
        &self,
        first_line: usize,
        last_line: usize,
        current_line: usize,
        scroll_y: f32,
    ) -> Vec<LineNumberEntry> {
        if !self.config.show_line_numbers {
            return Vec::new();
        }

        let mut entries = Vec::with_capacity(last_line - first_line + 1);

        for line in first_line..=last_line.min(self.total_lines.saturating_sub(1)) {
            let is_current = line == current_line;
            let y = (line as f32 * self.line_height) - scroll_y;

            entries.push(LineNumberEntry {
                number: line + 1, // Convert to 1-based for display
                y,
                is_current,
                color: if is_current {
                    self.config.current_line_number_color
                } else {
                    self.config.line_number_color
                },
            });
        }

        entries
    }

    /// Formats a line number for display.
    ///
    /// Right-aligns the number to match the gutter width.
    #[must_use]
    pub fn format_line_number(&self, number: usize) -> String {
        let digits = self.digit_count(self.total_lines).max(self.config.min_digits);
        format!("{:>width$}", number, width = digits)
    }

    /// Returns the gutter background color.
    #[must_use]
    pub fn background_color(&self) -> [f32; 4] {
        self.config.background_color
    }

    /// Returns the gutter background as a wgpu::Color.
    #[must_use]
    pub fn background_wgpu_color(&self) -> wgpu::Color {
        wgpu::Color {
            r: self.config.background_color[0] as f64,
            g: self.config.background_color[1] as f64,
            b: self.config.background_color[2] as f64,
            a: self.config.background_color[3] as f64,
        }
    }

    /// Returns line number color as a glyphon Color.
    #[must_use]
    pub fn line_number_glyphon_color(&self, is_current: bool) -> glyphon::Color {
        let color = if is_current {
            self.config.current_line_number_color
        } else {
            self.config.line_number_color
        };
        glyphon::Color::rgba(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
            (color[3] * 255.0) as u8,
        )
    }

    /// Returns the separator line coordinates if enabled.
    ///
    /// Returns (x, y1, y2) where x is the x-coordinate and y1/y2 are the top/bottom.
    #[must_use]
    pub fn separator_line(&self, viewport_height: f32) -> Option<(f32, f32, f32)> {
        if self.config.show_separator && self.config.show_line_numbers {
            Some((self.cached_width.total - 1.0, 0.0, viewport_height))
        } else {
            None
        }
    }
}

impl Default for GutterRenderer {
    fn default() -> Self {
        Self::new(GutterConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gutter_config_default() {
        let config = GutterConfig::default();
        assert!(config.show_line_numbers);
        assert_eq!(config.min_digits, 3);
    }

    #[test]
    fn test_gutter_width() {
        let width = GutterWidth::new(30.0, 8.0, 12.0);
        assert_eq!(width.total, 50.0);
        assert_eq!(width.line_number_x(), 8.0);
        assert_eq!(width.content_start_x(), 50.0);
    }

    #[test]
    fn test_gutter_renderer_new() {
        let renderer = GutterRenderer::new(GutterConfig::default());
        assert!(renderer.total_width() > 0.0);
    }

    #[test]
    fn test_gutter_renderer_digit_count() {
        let renderer = GutterRenderer::default();
        assert_eq!(renderer.digit_count(1), 1);
        assert_eq!(renderer.digit_count(9), 1);
        assert_eq!(renderer.digit_count(10), 2);
        assert_eq!(renderer.digit_count(99), 2);
        assert_eq!(renderer.digit_count(100), 3);
        assert_eq!(renderer.digit_count(1000), 4);
    }

    #[test]
    fn test_gutter_renderer_width_scales_with_lines() {
        let mut renderer = GutterRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);

        // With min_digits=3, default width should be for 3 digits
        let width_3 = renderer.total_width();

        // 10000 lines needs 5 digits
        renderer.set_total_lines(10000);
        let width_5 = renderer.total_width();

        assert!(width_5 > width_3);
    }

    #[test]
    fn test_gutter_renderer_generate_line_numbers() {
        let mut renderer = GutterRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);
        renderer.set_total_lines(100);

        let entries = renderer.generate_line_numbers(0, 10, 5, 0.0);

        assert_eq!(entries.len(), 11); // lines 0-10 inclusive

        // Line numbers are 1-based for display
        assert_eq!(entries[0].number, 1);
        assert_eq!(entries[5].number, 6);

        // Current line should be highlighted
        assert!(entries[5].is_current);
        assert!(!entries[0].is_current);

        // Check Y coordinates
        assert_eq!(entries[0].y, 0.0);
        assert_eq!(entries[5].y, 100.0); // 5 * 20
    }

    #[test]
    fn test_gutter_renderer_generate_line_numbers_with_scroll() {
        let mut renderer = GutterRenderer::default();
        renderer.set_font_metrics(10.0, 20.0);
        renderer.set_total_lines(100);

        let entries = renderer.generate_line_numbers(10, 20, 15, 200.0);

        // First entry should be line 11 (1-based)
        assert_eq!(entries[0].number, 11);

        // Y coordinate should account for scroll
        assert_eq!(entries[0].y, 0.0); // (10 * 20) - 200 = 0
        assert_eq!(entries[5].y, 100.0); // (15 * 20) - 200 = 100
    }

    #[test]
    fn test_gutter_renderer_format_line_number() {
        let mut renderer = GutterRenderer::default();
        renderer.set_total_lines(100);

        assert_eq!(renderer.format_line_number(1), "  1");
        assert_eq!(renderer.format_line_number(10), " 10");
        assert_eq!(renderer.format_line_number(100), "100");
    }

    #[test]
    fn test_gutter_renderer_disabled() {
        let config = GutterConfig {
            show_line_numbers: false,
            ..Default::default()
        };
        let renderer = GutterRenderer::new(config);

        assert_eq!(renderer.total_width(), 0.0);

        let entries = renderer.generate_line_numbers(0, 10, 5, 0.0);
        assert!(entries.is_empty());
    }

    #[test]
    fn test_gutter_separator_line() {
        let renderer = GutterRenderer::default();

        let sep = renderer.separator_line(600.0);
        assert!(sep.is_some());

        let (x, y1, y2) = sep.unwrap();
        assert!(x > 0.0);
        assert_eq!(y1, 0.0);
        assert_eq!(y2, 600.0);
    }
}
