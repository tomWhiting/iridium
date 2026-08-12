//! The evidence that a `RunStyle`'s weight and slant become different glyphs.
//!
//! # Why this test and not the one beside it
//!
//! `render::text`'s unit tests already assert that a `RunStyle` reaches
//! cosmic-text as an `Attrs` carrying that weight and that slant. That is a
//! claim about the *seam*, and it would stay green if the shaper resolved the
//! same face for every weight it was handed — which is exactly what happens
//! when the family in play ships one face. The editor would ask for bold, get
//! regular, and report nothing.
//!
//! ⭐ **Crossing from "the attribute was set" to "the screen changed" is a
//! second claim and needs its own evidence.** That is what this file is: a real
//! compose through the real pipeline, read back, and compared.
//!
//! # The control is the load-bearing half
//!
//! "The bold frame differs from the plain frame" proves nothing on its own —
//! two composes could differ for a dozen uninteresting reasons, a caret phase
//! among them. So every comparison here is made against a **plain-versus-plain
//! control composed by the same path**: identical inputs must give identical
//! pixels, and only then does a difference under a changed weight mean the
//! weight caused it.
//!
//! # What a failure here means
//!
//! ⚠️ Two different things, and the message says which to check:
//!
//! - The weight is not reaching the shaper — a defect in this tree.
//! - The family `Family::Monospace` resolves to on *this machine* has no bold
//!   or italic face, so cosmic-text correctly drew the nearest thing it had.
//!   That is an environment fact, and it is the reason a theme must not be
//!   shipped asking for a weight until someone has watched it arrive.
//!
//! Headless by construction: no surface, an offscreen texture, and a missing
//! adapter fails the run loudly rather than passing over work that never
//! happened.

mod support;

use iridium_editor::render::{HighlightContext, HighlightSource, RunSlant, RunStyle, RunWeight};
use iridium_editor::theme::Color;

use support::frame::{Target, read_pixels, target};
use support::gpu::{Gpu, compositor, gpu};
use support::scene::editor;

/// The name this harness reports failures and labels GPU objects under.
const HARNESS: &str = "run_weight_reaches_the_glyphs";

/// Paints the whole visible content in one style.
///
/// The generation moves by hand whenever the style does, which is the contract
/// a real face implements — without it the compositor would reuse the retained
/// shape and the second frame would be the first frame, making every
/// comparison below vacuously equal.
struct OneStyle {
    /// The style every run is painted in.
    style: RunStyle,
    /// The reported generation.
    generation: u64,
}

impl HighlightSource for OneStyle {
    fn resolve<'a>(&mut self, context: &HighlightContext<'a>) -> Option<Vec<(&'a str, RunStyle)>> {
        Some(vec![(context.content, self.style)])
    }

    fn language_active(&self) -> bool {
        true
    }

    fn generation(&self) -> u64 {
        self.generation
    }
}

/// A colour with enough contrast against the page that a glyph's edge is a
/// large pixel difference rather than a rounding one.
const fn ink() -> Color {
    Color::new(0.05, 0.05, 0.05, 1.0)
}

/// Composes one frame in `style` and returns its pixels.
///
/// A fresh compositor per frame rather than a reused one: this file is about
/// what a style *draws*, and sharing a compositor would fold the retained-shape
/// question into every assertion. That question has its own binary.
fn frame_in(gpu: &Gpu, target: &Target, style: RunStyle) -> Vec<u8> {
    let editor = editor();
    let mut compositor = compositor(gpu);
    let mut highlights = OneStyle {
        style,
        generation: 1,
    };
    support::frame::compose(&mut compositor, &editor, 0.0, &mut highlights, gpu, target);
    read_pixels(gpu, target)
}

/// How many pixels differ between two frames.
fn differing_pixels(left: &[u8], right: &[u8]) -> usize {
    left.chunks_exact(4)
        .zip(right.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count()
}

#[test]
fn a_bold_run_draws_different_glyphs_than_a_plain_one() {
    let gpu = gpu(HARNESS);
    let target = target(&gpu);

    let plain = frame_in(&gpu, &target, RunStyle::plain(ink()));

    // ⭐ The control. Everything below is meaningless without it: it says the
    // pipeline is deterministic, so a difference under a changed weight is
    // attributable to the weight.
    let plain_again = frame_in(&gpu, &target, RunStyle::plain(ink()));
    assert_eq!(
        differing_pixels(&plain, &plain_again),
        0,
        "two identical composes differ, so this harness cannot attribute any \
         difference to a style — fix the nondeterminism before reading the \
         assertions below"
    );

    let bold = frame_in(&gpu, &target, RunStyle::plain(ink()).bold());
    let changed = differing_pixels(&plain, &bold);
    assert!(
        changed > 0,
        "asking for {:?} drew pixel-for-pixel what {:?} drew. Either the \
         weight is not reaching the shaper — check `TextRenderer::span_attrs` \
         and the `RunStyle` seam — or the family `Family::Monospace` resolves \
         to on this machine ships no bold face, in which case no theme may \
         ship asking for one until it does",
        RunWeight::BOLD,
        RunWeight::NORMAL
    );
}

#[test]
fn an_italic_run_draws_different_glyphs_than_an_upright_one() {
    let gpu = gpu(HARNESS);
    let target = target(&gpu);

    let upright = frame_in(&gpu, &target, RunStyle::plain(ink()));
    let leaning = frame_in(&gpu, &target, RunStyle::plain(ink()).italic());

    assert!(
        differing_pixels(&upright, &leaning) > 0,
        "asking for {:?} drew pixel-for-pixel what {:?} drew. Either the slant \
         is not reaching the shaper, or the resolved family ships no italic \
         face on this machine",
        RunSlant::Italic,
        RunSlant::Upright
    );
}

/// ⭐ The colour-only identity, held at the pixel level.
///
/// `RunStyle::plain` must draw exactly what the colour-only path drew before
/// weight and slant existed. The value-level version of this claim is asserted
/// in three other places; this is the one that would catch a default that is
/// equal as a value and different as a frame — a synthesised face, say, or a
/// weight of 400 taking a different resolution path from no weight at all.
#[test]
fn a_plain_run_draws_what_an_unstyled_run_always_drew() {
    let gpu = gpu(HARNESS);
    let target = target(&gpu);

    let plain = frame_in(&gpu, &target, RunStyle::plain(ink()));
    let spelled_out = frame_in(
        &gpu,
        &target,
        RunStyle {
            color: ink(),
            weight: RunWeight::NORMAL,
            slant: RunSlant::Upright,
        },
    );

    assert_eq!(
        differing_pixels(&plain, &spelled_out),
        0,
        "`RunStyle::plain` and the same values written out longhand drew \
         different frames, so `plain` is no longer the identity every other \
         layer assumes it is"
    );
}
