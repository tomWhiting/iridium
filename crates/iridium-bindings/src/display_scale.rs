//! Sanitising the display scale factor that JavaScript hands the web face.
//!
//! `createWebEditor` takes `window.devicePixelRatio` as an `f32` and
//! multiplies the base font size by it. Everything downstream of that
//! multiplication assumes a positive, finite line height: viewport
//! arithmetic divides by it, and `render::units::pixel_to_index` converts
//! the quotient to a row index. A ratio of `0` makes the line height `0`,
//! `scroll_y / 0.0` is `+∞`, and the first visible line becomes
//! `usize::MAX`.
//!
//! Zero is not hypothetical. `devicePixelRatio` is `0` in some headless and
//! synthetic environments and `undefined` before first layout, and
//! `undefined` arrives here as `NaN`. TypeScript guards it with
//! `window.devicePixelRatio || 1` at three of its four call sites, which is
//! both incomplete and fragile: `||` rejects `0` and `NaN` only by accident
//! of falsiness, so the routine `||`-to-`??` modernisation deletes the guard
//! silently.
//!
//! So the guard belongs here, at the boundary the values actually cross,
//! where it protects every caller rather than the callers one face
//! remembered.
//!
//! This module is deliberately free of `wasm-bindgen` so its tests run on
//! the host.

/// The scale factor used when the supplied one is unusable.
///
/// One, not the last good value: this runs during construction, where there
/// is no previous value, and an unscaled editor is legible on every display
/// while a zero-scaled one is not an editor at all.
pub const FALLBACK_PIXEL_RATIO: f32 = 1.0;

/// The smallest scale factor accepted as a genuine display measurement.
///
/// A quarter, and the reason is agreement with the consumer rather than
/// arithmetic. Scaling 14 px by a subnormal ratio does *not* underflow to
/// zero — multiplying by 14 raises it — so this bound is not, as a first
/// draft of it claimed, an underflow guard. What it is: the renderer refuses
/// a font size below one physical pixel, so every ratio under about `0.071`
/// would be waved through here and then silently refused there, leaving the
/// editor at a size the face believed it had changed. **A sanitiser that
/// accepts values its consumer rejects is not sanitising**, it is moving the
/// rejection somewhere quieter.
///
/// A quarter sits well above that arithmetic floor and below every real
/// configuration, so it also rejects the merely absurd. It keeps the scaled
/// base size at 3.5 px or more.
pub const MIN_PIXEL_RATIO: f32 = 0.25;

/// The largest scale factor accepted as a genuine display measurement.
///
/// Chosen from hardware rather than from arithmetic: shipping displays run
/// at 1, 1.25, 1.5, 2 and 3, and macOS reports at most 3. Sixteen is far
/// past every real device while still rejecting the values that indicate a
/// unit confusion — a caller passing a DPI (96, 144) or a percentage (150)
/// instead of a ratio. Those are not scale factors, and silently honouring
/// one produces an unreadable editor rather than an error anybody can act
/// on.
pub const MAX_PIXEL_RATIO: f32 = 16.0;

/// Why a supplied scale factor was replaced.
///
/// Carried separately from the corrected value so the caller can log the
/// substitution. Silently correcting the input is how a misconfigured host
/// stays misconfigured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelRatioFault {
    /// The value was `NaN` or an infinity — typically `undefined` or a
    /// division by zero reaching the boundary as a float.
    NotFinite,
    /// The value was zero or negative. A scale factor is a ratio of
    /// lengths and cannot be either.
    NotPositive,
    /// The value was positive but below [`MIN_PIXEL_RATIO`] — too small to
    /// be a display, and small enough that scaling by it underflows.
    TooSmall,
    /// The value was finite and positive but implausibly large, which
    /// indicates a unit confusion rather than a display.
    TooLarge,
}

impl PixelRatioFault {
    /// A short explanation suitable for a console line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NotFinite => "not finite",
            Self::NotPositive => "not positive",
            Self::TooSmall => "implausibly small",
            Self::TooLarge => "implausibly large",
        }
    }
}

/// Returns a usable scale factor, plus the fault if `ratio` was replaced.
///
/// `Ok(ratio)` means the value was taken as given. `Err((fallback, fault))`
/// means it was replaced by [`FALLBACK_PIXEL_RATIO`]; the caller should use
/// the value and report the fault.
///
/// The result is always finite and within
/// `[MIN_PIXEL_RATIO, MAX_PIXEL_RATIO]` — which is the whole contract, since
/// it is what makes `base_font_size * ratio` finite and *non-zero* for any
/// sane base. Both ends matter: the upper one against overflow, the lower
/// one against underflow.
///
/// # Errors
///
/// Returns the fallback and a [`PixelRatioFault`] when `ratio` is not
/// finite, not strictly positive, or outside
/// `[MIN_PIXEL_RATIO, MAX_PIXEL_RATIO]`.
pub fn sanitize_pixel_ratio(ratio: f32) -> Result<f32, (f32, PixelRatioFault)> {
    if !ratio.is_finite() {
        return Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotFinite));
    }
    if ratio <= 0.0 {
        return Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotPositive));
    }
    if ratio < MIN_PIXEL_RATIO {
        return Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::TooSmall));
    }
    if ratio > MAX_PIXEL_RATIO {
        return Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::TooLarge));
    }
    Ok(ratio)
}

#[cfg(test)]
mod tests {
    use super::{
        FALLBACK_PIXEL_RATIO, MAX_PIXEL_RATIO, MIN_PIXEL_RATIO, PixelRatioFault,
        sanitize_pixel_ratio,
    };

    /// The base font size `createWebEditor` scales by the ratio. Duplicated
    /// here on purpose: if that constant moves, this test should be the
    /// thing that notices.
    const BASE_FONT_SIZE: f32 = 14.0;

    /// The renderer's own lower bound on a font size, in physical pixels
    /// (`render::text::MIN_FONT_SIZE`, which is private to that module).
    /// Restated rather than imported, and the restatement is the point: the
    /// assertions below are what would fail if the two ever drifted apart.
    const MIN_RENDERABLE_FONT_SIZE: f32 = 1.0;

    #[test]
    fn every_shipping_display_ratio_is_taken_as_given() {
        for ratio in [1.0_f32, 1.25, 1.5, 2.0, 3.0] {
            assert_eq!(
                sanitize_pixel_ratio(ratio),
                Ok(ratio),
                "a real display's ratio must survive the guard unchanged"
            );
        }
    }

    #[test]
    fn zero_is_replaced_because_it_is_the_case_that_reaches_infinity() {
        assert_eq!(
            sanitize_pixel_ratio(0.0),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotPositive))
        );
    }

    #[test]
    fn negative_zero_is_caught_as_well_as_positive_zero() {
        // `-0.0 <= 0.0` is true and `-0.0 == 0.0` is true, so this is the
        // same branch — asserted because a future rewrite to `ratio == 0.0`
        // would still pass and a rewrite to `ratio.is_sign_negative()`
        // would not.
        assert_eq!(
            sanitize_pixel_ratio(-0.0),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotPositive))
        );
    }

    #[test]
    fn nan_is_replaced_because_undefined_arrives_as_nan() {
        assert_eq!(
            sanitize_pixel_ratio(f32::NAN),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotFinite))
        );
    }

    #[test]
    fn both_infinities_are_replaced() {
        assert_eq!(
            sanitize_pixel_ratio(f32::INFINITY),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotFinite))
        );
        assert_eq!(
            sanitize_pixel_ratio(f32::NEG_INFINITY),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotFinite))
        );
    }

    #[test]
    fn a_negative_ratio_is_replaced() {
        assert_eq!(
            sanitize_pixel_ratio(-2.0),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::NotPositive))
        );
    }

    #[test]
    fn a_dpi_passed_where_a_ratio_was_expected_is_refused() {
        // The unit-confusion case the upper bound exists for.
        for confused in [96.0_f32, 144.0, 150.0] {
            assert_eq!(
                sanitize_pixel_ratio(confused),
                Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::TooLarge))
            );
        }
    }

    #[test]
    fn both_bounds_are_inclusive_and_just_past_each_is_not() {
        assert_eq!(sanitize_pixel_ratio(MIN_PIXEL_RATIO), Ok(MIN_PIXEL_RATIO));
        assert_eq!(
            sanitize_pixel_ratio(f32::from_bits(MIN_PIXEL_RATIO.to_bits() - 1)),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::TooSmall))
        );
        assert_eq!(sanitize_pixel_ratio(MAX_PIXEL_RATIO), Ok(MAX_PIXEL_RATIO));
        assert_eq!(
            sanitize_pixel_ratio(f32::from_bits(MAX_PIXEL_RATIO.to_bits() + 1)),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::TooLarge))
        );
    }

    /// The reason the lower bound is a positive number rather than zero.
    ///
    /// Not underflow — that was the first draft's claim and it is false, as
    /// this test asserts outright so nobody re-derives it. The reason is
    /// that the renderer refuses a font size below one physical pixel, so a
    /// ratio this small produces a size that is perfectly finite, perfectly
    /// positive, and refused downstream. Catching it here is what keeps the
    /// two guards from disagreeing.
    #[test]
    fn a_subnormal_ratio_is_refused_although_scaling_by_it_does_not_underflow() {
        let tiny = f32::from_bits(1);
        assert!(tiny > 0.0 && tiny.is_finite(), "the premise: positive");
        let scaled = BASE_FONT_SIZE * tiny;
        assert!(
            scaled > 0.0,
            "multiplying a subnormal by a base above one raises it; it does not underflow"
        );
        assert!(
            scaled < MIN_RENDERABLE_FONT_SIZE,
            "but it lands below the size the renderer will accept, which is the real reason"
        );

        assert_eq!(
            sanitize_pixel_ratio(tiny),
            Err((FALLBACK_PIXEL_RATIO, PixelRatioFault::TooSmall))
        );
    }

    /// Every accepted ratio produces a size the renderer will accept.
    ///
    /// This is the agreement the lower bound exists to maintain, checked at
    /// the boundary where it is tightest.
    #[test]
    fn the_smallest_accepted_ratio_still_clears_the_renderers_own_floor() {
        // A const block, at clippy's suggestion and to its benefit: the two
        // bounds are constants, so their agreement is a compile-time fact
        // and drift should break the build rather than a test run.
        const {
            assert!(
                BASE_FONT_SIZE * MIN_PIXEL_RATIO >= MIN_RENDERABLE_FONT_SIZE,
                "a ratio this module accepts must not be refused by the renderer"
            );
        }
    }

    /// The property the whole module exists to guarantee.
    #[test]
    fn the_scaled_font_size_is_always_finite_and_non_zero_whatever_goes_in() {
        let inputs = [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::from_bits(1),
            0.0,
            -0.0,
            -1.0,
            0.1,
            1.0,
            1.5,
            2.0,
            3.0,
            1e30,
            -1e30,
        ];
        for input in inputs {
            let ratio = sanitize_pixel_ratio(input).unwrap_or_else(|(fallback, _)| fallback);
            assert!(
                ratio.is_finite() && (MIN_PIXEL_RATIO..=MAX_PIXEL_RATIO).contains(&ratio),
                "sanitising {input} yielded {ratio}, which breaks the contract"
            );
            let scaled = BASE_FONT_SIZE * ratio;
            assert!(
                scaled.is_finite() && scaled > 0.0,
                "a base font size scaled by {ratio} became {scaled}"
            );
        }
    }

    #[test]
    fn each_fault_reports_a_distinct_reason() {
        let reasons = [
            PixelRatioFault::NotFinite.reason(),
            PixelRatioFault::NotPositive.reason(),
            PixelRatioFault::TooSmall.reason(),
            PixelRatioFault::TooLarge.reason(),
        ];
        assert_eq!(
            reasons.len(),
            reasons
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            "a log line must say which fault occurred"
        );
    }
}
