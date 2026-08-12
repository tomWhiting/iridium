//! The theme, resolved once per frame into the styles a cell can carry.
//!
//! The kernel's theme is expressed in `f32` RGBA, which a terminal cannot
//! reproduce directly: it has no alpha channel, and its default foreground and
//! background are the user's choice rather than ours. Two rules follow, and
//! they are applied here so that no other module has to know them.
//!
//! **A fully transparent colour is the terminal's own.** A theme may leave a
//! surface fully transparent for a GPU face to composite over whatever sits
//! behind it. There is no window here, so it becomes [`Color::Default`], which
//! is exactly "whatever the user configured", and which survives every colour
//! degradation tier untouched.
//!
//! **A translucent colour is composited over the editor background when there
//! is one, and used at full strength when there is not.** Selection and
//! current-line highlights are half-transparent in every preset; over an
//! unknown background the honest answer is the colour itself, because guessing
//! at the user's terminal background is how a selection ends up invisible.

use iridium_editor::render::{RunSlant, RunWeight};
use iridium_editor::syntax::{HighlightType, highlight_to_style};
use iridium_editor::theme::{Color as ThemeColor, SyntaxColors, SyntaxEmphasis, Theme};

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
    /// The background of a search match that is not the current one.
    search_match: Color,
    /// The background of the search match the user is standing on.
    search_match_current: Color,
    /// The base style of an overlay panel: the search UI's own two rows.
    overlay: Style,
    /// The style of an inactive toggle, and of a field that is not focused.
    overlay_quiet: Style,
    /// The style of a toggle that is switched on.
    overlay_toggle_active: Style,
    /// The style feedback about a failure is shown in.
    overlay_error: Style,
    /// The theme's syntax colours, mapped per highlight by the kernel.
    syntax: SyntaxColors,
    /// The theme's emphasis table, applied per highlight by the same kernel
    /// mapping — so a heading this face draws bold is bold in the GPU face too.
    emphasis: SyntaxEmphasis,
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
            search_match: over(theme.editor.search_match, editor_background),
            search_match_current: over(theme.editor.search_match_current, editor_background),
            overlay: overlay_style(theme, foreground, editor_background),
            overlay_quiet: overlay_style(theme, foreground, editor_background)
                .with_attributes(Attributes::DIM)
                .with_foreground(solid(theme.editor.line_number)),
            overlay_toggle_active: overlay_style(theme, foreground, editor_background)
                .with_attributes(Attributes::BOLD)
                .with_foreground(solid(theme.editor.line_number_active)),
            overlay_error: overlay_style(theme, foreground, editor_background)
                .with_foreground(solid(theme.editor.diagnostic_error)),
            syntax: theme.syntax.clone(),
            emphasis: theme.emphasis.clone(),
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

    /// `style` with a search match's background, the current match apart.
    ///
    /// Only the background moves. A match keeps the syntax colour of whatever
    /// it landed on, because a search that repainted the code it found in one
    /// flat colour would hide the very thing being read.
    pub const fn search_match(&self, style: Style, is_current: bool) -> Style {
        if is_current {
            style.with_background(self.search_match_current)
        } else {
            style.with_background(self.search_match)
        }
    }

    /// The base style of an overlay panel.
    pub const fn overlay(&self) -> Style {
        self.overlay
    }

    /// The style of an overlay's label text, and of a field that is not
    /// focused: quieter than the panel, so the focused field is obvious.
    pub const fn overlay_quiet(&self) -> Style {
        self.overlay_quiet
    }

    /// The style of an overlay field, focused or not.
    pub const fn overlay_field(&self, is_focused: bool) -> Style {
        if is_focused {
            self.overlay
        } else {
            self.overlay_quiet
        }
    }

    /// The style of an overlay toggle, switched on or off.
    pub const fn overlay_toggle(&self, is_active: bool) -> Style {
        if is_active {
            self.overlay_toggle_active
        } else {
            self.overlay_quiet
        }
    }

    /// The style feedback about a failure is shown in.
    pub const fn overlay_error(&self) -> Style {
        self.overlay_error
    }

    /// An overlay row's style for a run coloured by a shared panel builder.
    ///
    /// ⭐ **The one place a [`Span`](iridium_panel::Span)'s colour becomes a
    /// terminal colour.** The builders in [`iridium_panel`] compose against the
    /// theme and hand back runs carrying theme colours; this face decides what
    /// a terminal can do with one. Routed through [`solid`] like everything
    /// else here, so a fully transparent colour becomes the terminal's own
    /// foreground rather than a black that fights whatever the user's palette
    /// actually is.
    ///
    /// The background is the panel's, never the span's: a shared builder
    /// colours text, and a row's backdrop is the face's furniture.
    pub fn overlay_span(&self, color: ThemeColor) -> Style {
        self.overlay.with_foreground(solid(color))
    }

    /// The style of text carrying a syntax highlight.
    ///
    /// The highlight-to-appearance mapping is the kernel's
    /// ([`highlight_to_style`]): a face with its own copy would drift from the
    /// GPU face the first time a highlight category was added.
    ///
    /// ⚠️ **A terminal's bold and italic are not the GPU face's.** A weight of
    /// 700 becomes the `BOLD` attribute and any lean becomes `ITALIC`, because
    /// those are the only two the cell model has and the only two a terminal
    /// can be asked for. A theme asking for semibold therefore reads as bold
    /// here and as semibold there — a deliberate degradation in the same
    /// direction as [`solid`]'s alpha handling above, not a drift: both faces
    /// still answer from one theme, and the difference is what the surface can
    /// physically draw rather than what it believes.
    pub fn highlighted(&self, highlight: HighlightType, background: Color) -> Style {
        let style = highlight_to_style(highlight, &self.syntax, &self.emphasis);
        let mut attributes = Attributes::NONE;
        if style.weight >= RunWeight::BOLD {
            attributes |= Attributes::BOLD;
        }
        if style.slant == RunSlant::Italic {
            attributes |= Attributes::ITALIC;
        }
        Style::new(solid(style.color), background, attributes)
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
/// A theme whose gutter is transparent, or indistinct from the editor
/// background, gives the statusline nothing to stand out against, so it
/// falls back to reverse video — the one way to be visibly a band whether
/// the surface colour is the terminal's own or a gutter that collapsed into
/// the background.
fn status_style(theme: &Theme) -> Style {
    let background = solid(theme.editor.gutter);
    if background == Color::Default || theme.editor.gutter == theme.editor.background {
        return Style::new(Color::Default, Color::Default, Attributes::REVERSE);
    }
    Style::new(solid(theme.editor.foreground), background, Attributes::NONE)
}

/// The base style of an overlay panel.
///
/// The background is the current-line colour rather than the gutter's. Both
/// presets give it a value that reads as a band a step above the editor
/// surface, and it is the one theme colour
/// whose whole purpose is "a stripe across the text that is still text". A
/// theme that leaves it transparent gets a panel in the terminal's own
/// background, which is legible but no longer visibly a panel; the labels,
/// toggles and the panel's position above the statusline still separate it.
///
/// Reverse video is deliberately not the fallback here, as it is for the
/// statusline: one inverted row is a band, and two inverted rows carrying an
/// editable field is a wall the caret disappears into.
fn overlay_style(theme: &Theme, foreground: Color, editor_background: ThemeColor) -> Style {
    Style::new(
        foreground,
        over(theme.editor.current_line, editor_background),
        Attributes::NONE,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use iridium_editor::theme::Emphasis;

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
    fn the_dark_preset_paints_its_own_background() {
        // The dark preset states an opaque #1a1a1a surface — the pixels the
        // web demo shows — so the terminal paints it rather than deferring
        // to its own background.
        let palette = Palette::from_theme(&Theme::dark());
        assert_eq!(palette.text().background, Color::Rgb(26, 26, 26));
        assert_ne!(palette.text().foreground, Color::Default);
    }

    #[test]
    fn the_dark_presets_selection_is_visible() {
        let palette = Palette::from_theme(&Theme::dark());
        let selected = palette.selected(palette.text());
        assert_ne!(selected.background, Color::Default);
        assert_ne!(selected.background, palette.text().background);
    }

    #[test]
    fn a_theme_with_no_gutter_colour_gets_a_reverse_video_statusline() {
        let mut theme = Theme::dark();
        theme.editor.gutter = ThemeColor::new(0.0, 0.0, 0.0, 0.0);
        let palette = Palette::from_theme(&theme);
        assert!(palette.status().attributes.contains(Attributes::REVERSE));
    }

    #[test]
    fn a_gutter_indistinct_from_the_background_gets_a_reverse_video_statusline() {
        // The dark preset's gutter equals its editor background: a statusline
        // painted gutter-on-gutter is no band at all, so it must fall back to
        // reverse video exactly as a transparent gutter does.
        let theme = Theme::dark();
        assert_eq!(theme.editor.gutter, theme.editor.background);
        let palette = Palette::from_theme(&theme);
        assert!(palette.status().attributes.contains(Attributes::REVERSE));
    }

    #[test]
    fn a_gutter_distinct_from_the_background_gets_a_coloured_statusline() {
        let mut distinct = Theme::dark();
        distinct.editor.gutter = ThemeColor::new(0.15, 0.15, 0.18, 1.0);
        for theme in [distinct, Theme::light()] {
            assert_ne!(theme.editor.gutter, theme.editor.background);
            let palette = Palette::from_theme(&theme);
            assert!(!palette.status().attributes.contains(Attributes::REVERSE));
            assert_ne!(palette.status().background, Color::Default);
        }
    }

    #[test]
    fn a_highlight_takes_its_colour_from_the_theme() {
        let theme = Theme::dark();
        let palette = Palette::from_theme(&theme);
        let style = palette.highlighted(HighlightType::Keyword, Color::Default);
        assert_eq!(style.foreground, solid(theme.syntax.keyword));
        assert_ne!(style.foreground, palette.text().foreground);
        assert_eq!(
            style.attributes,
            Attributes::NONE,
            "a theme that asks for no emphasis must paint no attributes — the \
             terminal's bold is a thing the user sees"
        );
    }

    /// ⭐ The terminal half of the rich-text lane: a theme that bolds headings
    /// bolds them here too, and bolds nothing else.
    ///
    /// The mapping is the kernel's, so this is not asserting that bold works —
    /// it is asserting that this face *asks* the kernel rather than deciding
    /// for itself, which is the only reason the two native faces cannot
    /// disagree about what a heading looks like.
    #[test]
    fn a_theme_that_emphasises_a_category_reaches_the_cells() {
        let mut theme = Theme::dark();
        theme.emphasis = SyntaxEmphasis::none()
            .with(HighlightType::MarkupHeading, Emphasis::bold())
            .with(HighlightType::MarkupEmphasis, Emphasis::italic());
        let palette = Palette::from_theme(&theme);

        let heading = palette.highlighted(HighlightType::MarkupHeading, Color::Default);
        assert!(heading.attributes.contains(Attributes::BOLD));
        assert!(!heading.attributes.contains(Attributes::ITALIC));

        let emphasis = palette.highlighted(HighlightType::MarkupEmphasis, Color::Default);
        assert!(emphasis.attributes.contains(Attributes::ITALIC));
        assert!(!emphasis.attributes.contains(Attributes::BOLD));

        let keyword = palette.highlighted(HighlightType::Keyword, Color::Default);
        assert_eq!(
            keyword.attributes,
            Attributes::NONE,
            "a keyword shares the heading's colour and must not have gained \
             its weight"
        );
        assert_eq!(
            keyword.foreground, heading.foreground,
            "they still share a colour — that sharing was never the defect"
        );
    }

    /// The degradation, asserted rather than left to the doc comment.
    ///
    /// A terminal has one bold. Every weight at or above 700 reaches it and
    /// every weight below stays regular, which is a real loss of information
    /// against the GPU face and must be a stated, tested loss rather than a
    /// surprise — someone will eventually author a semibold heading and need
    /// to know what a terminal does with it.
    #[test]
    fn a_terminal_has_one_bold_and_the_threshold_is_seven_hundred() {
        for (weight, bold) in [
            (RunWeight(300), false),
            (RunWeight::NORMAL, false),
            (RunWeight(600), false),
            (RunWeight::BOLD, true),
            (RunWeight(900), true),
        ] {
            let mut theme = Theme::dark();
            theme.emphasis = SyntaxEmphasis::none().with(
                HighlightType::MarkupHeading,
                Emphasis {
                    weight: Some(weight),
                    slant: None,
                },
            );
            let style = Palette::from_theme(&theme)
                .highlighted(HighlightType::MarkupHeading, Color::Default);
            assert_eq!(
                style.attributes.contains(Attributes::BOLD),
                bold,
                "weight {weight:?} in a terminal"
            );
        }
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

    #[test]
    fn the_current_search_match_is_distinguishable_from_the_rest() {
        // Painting every match the same colour is a silent regression: the
        // matches are all still marked and navigation still works, and the
        // editor simply stops saying which one `Enter` will move on from.
        for theme in [Theme::dark(), Theme::light()] {
            let palette = Palette::from_theme(&theme);
            let plain = palette.text();
            let other = palette.search_match(plain, false);
            let current = palette.search_match(plain, true);
            assert_ne!(other.background, plain.background);
            assert_ne!(current.background, plain.background);
            assert_ne!(
                current.background, other.background,
                "the current match must not share the other matches' background"
            );
        }
    }

    #[test]
    fn a_search_match_keeps_the_foreground_it_landed_on() {
        let palette = Palette::from_theme(&Theme::dark());
        let keyword = palette.highlighted(HighlightType::Keyword, Color::Default);
        for is_current in [false, true] {
            assert_eq!(
                palette.search_match(keyword, is_current).foreground,
                keyword.foreground,
                "a match must not repaint the syntax colour underneath it"
            );
        }
    }

    #[test]
    fn an_overlay_toggle_looks_different_switched_on() {
        let palette = Palette::from_theme(&Theme::dark());
        assert_ne!(palette.overlay_toggle(true), palette.overlay_toggle(false));
        assert!(
            palette
                .overlay_toggle(true)
                .attributes
                .contains(Attributes::BOLD)
        );
    }

    #[test]
    fn an_unfocused_overlay_field_is_quieter_than_a_focused_one() {
        let palette = Palette::from_theme(&Theme::light());
        assert_eq!(palette.overlay_field(true), palette.overlay());
        assert_ne!(palette.overlay_field(false), palette.overlay_field(true));
    }

    #[test]
    fn overlay_error_feedback_takes_the_themes_error_colour() {
        let theme = Theme::light();
        let palette = Palette::from_theme(&theme);
        assert_eq!(
            palette.overlay_error().foreground,
            solid(theme.editor.diagnostic_error)
        );
        assert_ne!(
            palette.overlay_error().foreground,
            palette.overlay().foreground
        );
    }

    #[test]
    fn the_overlay_panel_stands_apart_from_the_text_in_both_presets() {
        for theme in [Theme::dark(), Theme::light()] {
            let palette = Palette::from_theme(&theme);
            assert_ne!(
                palette.overlay().background,
                palette.text().background,
                "the panel must not be indistinguishable from the document"
            );
        }
    }
}
