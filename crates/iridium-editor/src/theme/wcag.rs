//! WCAG 2.x contrast arithmetic, for the presets' legibility assertions.
//!
//! Test-only, and deliberately so: nothing at runtime rejects a theme for
//! being illegible, because a user who loads a low-contrast theme has chosen
//! it. What the presets *this crate ships* must satisfy is a different
//! question, and these are the tools the tests answer it with.
//!
//! ⚠️ **One implementation, used from both [`colors`](super::colors) and
//! [`classic`](super::classic).** They check different claims — the public
//! `light()` pair versus the three transcribed variants — and a second copy of
//! the arithmetic would let one drift into agreeing with a palette the other
//! rejects, which is the failure mode where both files look green and the
//! screen is wrong.
//!
//! # What this deliberately does not model
//!
//! Alpha. Every ratio here treats its inputs as opaque, so a translucent
//! colour is measured at full strength rather than as composited. That is
//! correct for the things measured — text inks, the caret, body foreground —
//! which are all opaque; it would be wrong for `selection` or `search_match`,
//! and no caller passes those.

use super::Color;

/// One channel's contribution to relative luminance (WCAG 2.x).
fn linearize(channel: f32) -> f32 {
    if channel <= 0.03928 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG relative luminance of an opaque colour.
fn luminance(color: Color) -> f32 {
    let red = 0.2126 * linearize(color.r);
    let green = 0.7152 * linearize(color.g);
    let blue = 0.0722 * linearize(color.b);
    red + green + blue
}

/// WCAG contrast ratio between two opaque colours, in the range 1.0..=21.0.
///
/// Symmetric in its arguments — the brighter of the two is found rather than
/// assumed, so a caller never has to know whether it is measuring ink on a
/// page or a page behind ink.
pub(super) fn contrast(a: Color, b: Color) -> f32 {
    let first = luminance(a);
    let second = luminance(b);
    let high = first.max(second);
    let low = first.min(second);
    (high + 0.05) / (low + 0.05)
}

/// The largest per-channel distance between two colours, in 0-255 steps — the
/// measure the light-theme map states its surface-separation rule in.
///
/// Rounded back onto the 0-255 grid the tables are written on: the channels
/// are exact bytes divided by 255, so a distance of exactly one step can land
/// a hair either side of the integer in `f32`.
pub(super) fn channel_distance(a: Color, b: Color) -> f32 {
    let red = (a.r - b.r).abs();
    let green = (a.g - b.g).abs();
    let blue = (a.b - b.b).abs();
    (red.max(green).max(blue) * 255.0).round()
}

#[cfg(test)]
mod tests {
    use super::{channel_distance, contrast};
    use crate::theme::Color;

    /// The two anchors of the scale, which is the only way to know the
    /// arithmetic is the right way up: a 21:1 that came out 1:1 would make
    /// every legibility assertion below it pass on a blank page.
    #[test]
    fn the_scale_runs_from_one_to_twenty_one() {
        let black = Color::new(0.0, 0.0, 0.0, 1.0);
        let white = Color::new(1.0, 1.0, 1.0, 1.0);
        assert!((contrast(black, white) - 21.0).abs() < 0.01);
        assert!((contrast(white, black) - 21.0).abs() < 0.01);
        assert!((contrast(white, white) - 1.0).abs() < 0.01);
    }

    /// Alpha is not part of the ratio. Stated as a test rather than left to
    /// the module doc, because a caller who passed a translucent colour would
    /// otherwise get a plausible number that means nothing.
    #[test]
    fn alpha_does_not_change_the_ratio() {
        let page = Color::new(1.0, 1.0, 1.0, 1.0);
        let opaque = Color::new(0.0, 0.0, 0.0, 1.0);
        let ghost = Color::new(0.0, 0.0, 0.0, 0.05);
        assert!((contrast(opaque, page) - contrast(ghost, page)).abs() < f32::EPSILON);
    }

    #[test]
    fn channel_distance_is_the_widest_channel_in_steps() {
        let a = Color::new(0.0, 0.0, 0.0, 1.0);
        let b = Color::new(2.0 / 255.0, 17.0 / 255.0, 5.0 / 255.0, 1.0);
        assert!((channel_distance(a, b) - 17.0).abs() < f32::EPSILON);
        assert!((channel_distance(b, a) - 17.0).abs() < f32::EPSILON);
        assert!((channel_distance(a, a)).abs() < f32::EPSILON);
    }
}
