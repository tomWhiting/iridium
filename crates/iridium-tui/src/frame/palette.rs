//! The theme, resolved once per frame into the styles a cell can carry.
//!
//! The kernel's theme is expressed in `f32` RGBA, which a terminal cannot
//! reproduce directly: it has no alpha channel, and its default foreground and
//! background are the user's choice rather than ours. Two rules follow, and
//! they are applied here so that no other module has to know them.
//!
//! **A fully transparent colour is the terminal's own.** The dark preset's
//! editor background is `(0, 0, 0, 0)` — the GPU face composites it over the
//! window. There is no window here, so it becomes [`Color::Default`], which is
//! exactly "whatever the user configured", and which survives every colour
//! degradation tier untouched.
//!
//! **A translucent colour is composited over the editor background when there
//! is one, and used at full strength when there is not.** Selection and
//! current-line highlights are half-transparent in every preset; over an
//! unknown background the honest answer is the colour itself, because guessing
//! at the user's terminal background is how a selection ends up invisible.

use iridium_editor::syntax::{HighlightType, highlight_to_color};
use iridium_editor::theme::{Color as ThemeColor, SyntaxColors, Theme};

use super::units::channel;
use crate::cell::{Attributes, Color, Style};

/// Every style a frame paints with, resolved from one theme.
#[derive(Debug, Clone)]
pub struct Palette {
    /// Ordinary document text.
    text: Style,
    /// A line number that has no cursor on it.
    gutter: Style,
    /// A line number on a line that has a cursor on it.
    gutter_active: Style,
    /// The statusline.
    status: Style,
    /// The background of a line that has a cursor on it.
    current_line: Color,
    /// The background of selected text.
    selection: Color,
    /// The background of a caret cell.
    caret: Color,
    /// The foreground of a caret cell, so the glyph under it stays legible.
    caret_text: Color,
    /// The theme's syntax colours, mapped per highlight by the kernel.
    syntax: SyntaxColors,
}

impl Palette {
    /// Resolves a theme into cell styles.
    pub fn from_theme(theme: &Theme) -> Self {
        let background = solid(theme.editor.background);
        let foreground = solid(theme.editor.foreground);
        let base = Style::new(foreground, background, Attributes::NONE);
        let editor_background = theme.editor.background;

        Self {
            text: base,
            gutter: base.with_foreground(solid(theme.editor.line_number)),
            gutter_active: base.with_foreground(solid(theme.editor.line_number_active)),
            // The statusline must read as a band across the screen even when
            // the editor background is the terminal's own, so its background
            // is the gutter colour composited to something opaque, and it
            // falls back to reverse video when the theme offers nothing.
            status: status_style(theme),
            current_line: over(theme.editor.current_line, editor_background),
            selection: over(theme.editor.selection, editor_background),
            caret: over(theme.editor.cursor, editor_background),
            caret_text: background,
            syntax: theme.syntax.clone(),
        }
    }

    /// The style of ordinary document text.
    pub const fn text(&self) -> Style {
        self.text
    }

    /// The style of a line number.
    pub const fn gutter(&self, is_active: bool) -> Style {
        if is_active {
            self.gutter_active
        } else {
            self.gutter
        }
    }

    /// The style of the statusline.
    pub const fn status(&self) -> Style {
        self.status
    }

    /// The background of a line that has a cursor on it.
    pub const fn current_line(&self) -> Color {
        self.current_line
    }

    /// `style` with the selection background.
    pub const fn selected(&self, style: Style) -> Style {
        style.with_background(self.selection)
    }

    /// `style` as a caret cell: a block in the cursor colour.
    pub const fn caret(&self, style: Style) -> Style {
        style
            .with_background(self.caret)
            .with_foreground(self.caret_text)
    }

    /// The style of text carrying a syntax highlight.
    ///
    /// The highlight-to-colour mapping is the kernel's
    /// ([`highlight_to_color`]): a face with its own copy would drift from the
    /// GPU face's colours the first time a highlight type was added.
    pub fn highlighted(&self, highlight: HighlightType, background: Color) -> Style {
        Style::new(
            solid(highlight_to_color(highlight, &self.syntax)),
            background,
            Attributes::NONE,
        )
    }
}

/// A theme colour as a cell colour, with transparency meaning "the
/// terminal's own".
fn solid(color: ThemeColor) -> Color {
    if color.a <= 0.0 {
        return Color::Default;
    }
    Color::Rgb(channel(color.r), channel(color.g), channel(color.b))
}

/// `color` composited over `background`, or at full strength when the
/// background is itself transparent and so unknown to us.
fn over(color: ThemeColor, background: ThemeColor) -> Color {
    if color.a <= 0.0 {
        return Color::Default;
    }
    if color.a >= 1.0 || background.a <= 0.0 {
        return solid(color);
    }
    let blend = |source: f32, target: f32| color.a.mul_add(source, (1.0 - color.a) * target);
    Color::Rgb(
        channel(blend(color.r, background.r)),
        channel(blend(color.g, background.g)),
        channel(blend(color.b, background.b)),
    )
}

/// The statusline's style.
///
/// A theme whose gutter is transparent gives the statusline nothing to stand
/// out against, so it falls back to reverse video — the one way to be visible
/// on a terminal whose colours we do not know.
fn status_style(theme: &Theme) -> Style {
    let background = solid(theme.editor.gutter);
    if background == Color::Default {
        return Style::new(Color::Default, Color::Default, Attributes::REVERSE);
    }
    Style::new(solid(theme.editor.foreground), background, Attributes::NONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transparent_colour_is_the_terminals_own() {
        assert_eq!(solid(ThemeColor::new(0.0, 0.0, 0.0, 0.0)), Color::Default);
        assert_eq!(solid(ThemeColor::new(1.0, 0.5, 0.0, 0.0)), Color::Default);
    }

    #[test]
    fn an_opaque_colour_becomes_its_own_rgb() {
        assert_eq!(
            solid(ThemeColor::new(1.0, 0.5, 0.0, 1.0)),
            Color::Rgb(255, 128, 0)
        );
    }

    #[test]
    fn a_translucent_colour_composites_over_a_known_background() {
        let background = ThemeColor::new(0.0, 0.0, 0.0, 1.0);
        let half_white = ThemeColor::new(1.0, 1.0, 1.0, 0.5);
        assert_eq!(over(half_white, background), Color::Rgb(128, 128, 128));
    }

    #[test]
    fn a_translucent_colour_over_an_unknown_background_keeps_its_own_strength() {
        let unknown = ThemeColor::new(0.0, 0.0, 0.0, 0.0);
        let half_white = ThemeColor::new(1.0, 1.0, 1.0, 0.5);
        assert_eq!(over(half_white, unknown), Color::Rgb(255, 255, 255));
    }

    #[test]
    fn the_dark_preset_leaves_the_background_to_the_terminal() {
        let palette = Palette::from_theme(&Theme::dark());
        assert_eq!(palette.text().background, Color::Default);
        assert_ne!(palette.text().foreground, Color::Default);
    }

    #[test]
    fn the_dark_presets_selection_is_visible_against_an_unknown_background() {
        let palette = Palette::from_theme(&Theme::dark());
        let selected = palette.selected(palette.text());
        assert_ne!(selected.background, Color::Default);
        assert_ne!(selected.background, palette.text().background);
    }

    #[test]
    fn a_theme_with_no_gutter_colour_gets_a_reverse_video_statusline() {
        let palette = Palette::from_theme(&Theme::dark());
        assert!(palette.status().attributes.contains(Attributes::REVERSE));
    }

    #[test]
    fn a_theme_with_a_gutter_colour_gets_a_coloured_statusline() {
        let palette = Palette::from_theme(&Theme::light());
        assert!(!palette.status().attributes.contains(Attributes::REVERSE));
        assert_ne!(palette.status().background, Color::Default);
    }

    #[test]
    fn a_highlight_takes_its_colour_from_the_theme() {
        let theme = Theme::dark();
        let palette = Palette::from_theme(&theme);
        let style = palette.highlighted(HighlightType::Keyword, Color::Default);
        assert_eq!(style.foreground, solid(theme.syntax.keyword));
        assert_ne!(style.foreground, palette.text().foreground);
    }

    #[test]
    fn a_caret_inverts_the_cell_it_sits_on() {
        let palette = Palette::from_theme(&Theme::light());
        let caret = palette.caret(palette.text());
        assert_eq!(caret.background, solid(Theme::light().editor.cursor));
        assert_ne!(caret.foreground, palette.text().foreground);
    }

    #[test]
    fn the_active_line_number_differs_from_the_rest() {
        let palette = Palette::from_theme(&Theme::dark());
        assert_ne!(palette.gutter(true), palette.gutter(false));
    }
}
