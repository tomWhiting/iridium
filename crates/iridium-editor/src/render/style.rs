//! How a run of text is drawn — colour, and the face it is drawn in.
//!
//! # Why this exists
//!
//! Until 12 Aug 2026 a highlight run was a `(&str, Color)` pair, and colour
//! was the *only* thing any face could say about how text should look. That is
//! enough for source code, where every token is the same face in a different
//! shade, and it is not enough for anything else: **a bold heading is not a
//! colour**, and neither is an emphasised word. The file explorer's own row
//! builder says so where it explains why a row marked for deletion wears a
//! `✗` instead of being struck through — it had a colour and nothing else to
//! work with.
//!
//! # What is deliberately not here
//!
//! ⛔ **No underline and no strikethrough.** Not an oversight and not a
//! placeholder: the text stack cannot express them. `cosmic-text`'s `Attrs`
//! carries colour, family, stretch, slant, weight, metrics, letter spacing and
//! font features, and nothing else — the only mention of either decoration in
//! the whole crate is a `//TODO: underline` in its own syntect example. They
//! are *geometry*, drawn as quads from the shaped run's position, and they
//! belong to the pass that draws quads rather than to the value that describes
//! a face. Carrying fields here that no renderer honoured would be a
//! decorative API, which is a defect this codebase has already had once and
//! fixed.
//!
//! # The one rule that keeps retained shaping honest
//!
//! ⚠️ A run whose style is [`RunStyle::plain`] must produce **byte-identical**
//! attributes to what the colour-only path produced before this module
//! existed. `TextRenderer::set_rich_text_diffed` reshapes a line only when its
//! attributes differ from the last frame's, so a default that merely *looked*
//! equivalent would reshape every line of the first frame after any full
//! rebuild — a performance cliff with no visible symptom to trace it by.

use crate::theme::Color;

/// The weight of the face a run is drawn in, on the usual 100–900 scale.
///
/// A number rather than a `Normal`/`Bold` pair because the text stack takes a
/// number and every intermediate value is real: a heading set in semibold is
/// an ordinary thing to want, and an enum would have to be widened into this
/// the first time somebody did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunWeight(pub u16);

impl RunWeight {
    /// Regular body text.
    pub const NORMAL: Self = Self(400);
    /// The weight `**strong**` and a heading are drawn at.
    pub const BOLD: Self = Self(700);
}

impl Default for RunWeight {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Whether a run leans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum RunSlant {
    /// Upright, as body text and code are.
    #[default]
    Upright,
    /// Italic, as `*emphasis*` is.
    Italic,
}

/// How one run of text is drawn.
///
/// Colour is mandatory and the rest default to body text, so
/// [`RunStyle::plain`] is exactly what a colour-only face said before this
/// type existed — see the module docs for why that identity is load-bearing
/// rather than merely tidy.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunStyle {
    /// The colour the glyphs are drawn in.
    pub color: Color,
    /// The weight of the face.
    pub weight: RunWeight,
    /// Whether the face leans.
    pub slant: RunSlant,
}

impl RunStyle {
    /// Body text in `color`: the style every syntax highlight had before
    /// weight and slant existed.
    #[must_use]
    pub fn plain(color: Color) -> Self {
        Self {
            color,
            weight: RunWeight::default(),
            slant: RunSlant::default(),
        }
    }

    /// The same run at [`RunWeight::BOLD`].
    #[must_use]
    pub const fn bold(mut self) -> Self {
        self.weight = RunWeight::BOLD;
        self
    }

    /// The same run in italic.
    #[must_use]
    pub const fn italic(mut self) -> Self {
        self.slant = RunSlant::Italic;
        self
    }

    /// The same run at an arbitrary weight.
    #[must_use]
    pub const fn with_weight(mut self, weight: RunWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Whether this style asks for anything the colour-only path could not
    /// have expressed.
    ///
    /// The question a renderer asks to know whether it may take a cheaper
    /// path, and the question a test asks to know that a default really is
    /// the default.
    #[must_use]
    pub fn is_plain(self) -> bool {
        self.weight == RunWeight::default() && self.slant == RunSlant::default()
    }
}

impl From<Color> for RunStyle {
    fn from(color: Color) -> Self {
        Self::plain(color)
    }
}

#[cfg(test)]
mod tests {
    use super::{RunSlant, RunStyle, RunWeight};
    use crate::theme::Color;

    fn ink() -> Color {
        Color::new(0.1, 0.2, 0.3, 1.0)
    }

    /// ⭐ The identity retained shaping depends on. If a plain run ever stops
    /// being the default in every field, the diffing setter starts reshaping
    /// lines nothing changed on, and the only symptom is that the editor gets
    /// slower.
    #[test]
    fn a_plain_run_asks_for_nothing_beyond_its_colour() {
        let style = RunStyle::plain(ink());
        assert!(style.is_plain());
        assert_eq!(style.weight, RunWeight::NORMAL);
        assert_eq!(style.slant, RunSlant::Upright);
        assert_eq!(style, RunStyle::from(ink()));
    }

    /// The builders compose, and each moves only its own field — a `bold`
    /// that also straightened the slant would be the kind of bug that only
    /// shows up on the one run wanting both.
    #[test]
    fn the_builders_are_independent_of_each_other() {
        let both = RunStyle::plain(ink()).bold().italic();
        assert_eq!(both.weight, RunWeight::BOLD);
        assert_eq!(both.slant, RunSlant::Italic);
        assert_eq!(both.color, ink());
        assert!(!both.is_plain());

        let italic_only = RunStyle::plain(ink()).italic();
        assert_eq!(
            italic_only.weight,
            RunWeight::NORMAL,
            "leaning changed the weight"
        );
    }

    /// Weight is a number on the real scale, not two names. This is the
    /// assertion that would fail if it were quietly narrowed back to an enum.
    #[test]
    fn a_weight_between_the_two_named_ones_survives() {
        let semibold = RunStyle::plain(ink()).with_weight(RunWeight(600));
        assert_eq!(semibold.weight, RunWeight(600));
        assert!(semibold.weight > RunWeight::NORMAL);
        assert!(semibold.weight < RunWeight::BOLD);
        assert!(!semibold.is_plain());
    }
}
