//! Exact numeric conversions for the values winit hands this face.
//!
//! Layout arithmetic ends in `f32` because that is what the compositor
//! consumes, but the quantities winit supplies — physical surface dimensions
//! as `u32`, the scale factor as `f64` — do not start there, and `value as
//! f32` is a silent lossy cast this workspace lints against.
//!
//! **The index↔pixel conversions are the kernel's and are re-exported below,
//! not restated here.** They were restated, from 2026-06 until 2026-08-06,
//! for one reason: `iridium_editor::render::units` was `pub(crate)` and this
//! face could not reach it. That copy had already drifted — its
//! `pixel_to_index` saturated at `u32::MAX` where the kernel's saturates at
//! `usize::MAX`, so the two disagreed on every input at or above 2^32. No
//! real line count reaches that, which is exactly why it went unnoticed;
//! a duplicate is discovered when its drift stops being benign, not when it
//! starts.
//!
//! What remains local is the family that is genuinely this face's:
//! `pixel_from_f64` (winit hands cursor positions as `f64`, which the kernel
//! never sees), `scale_to_f32`, `pixels_to_cells`, and the two glyphon clip
//! bounds. The bounds stay despite the kernel having same-named functions,
//! because `pixel_to_bound` genuinely differs — the kernel's returns
//! negative bounds and this one clamps to zero at the window edge — and
//! splitting one two-function family across two crates to share the half
//! that happens to match would cost more clarity than the duplication does.

pub use iridium_editor::render::units::{index_to_f32, pixel_to_index, u32_to_f32};

/// Converts a physical cursor coordinate from winit's `f64` to `f32`.
///
/// The conversion goes through 1/256-pixel fixed point: exact for every
/// coordinate below 2^24 device pixels, which is every window a display can
/// show. Negative coordinates — a drag that left the window — clamp to zero,
/// the window edge, which is where a selection dragged off-screen should
/// anchor; `NaN` lands there too.
pub fn pixel_from_f64(value: f64) -> f32 {
    u32_to_f32(nearest_u32(value * 256.0)) / 256.0
}

/// Converts a pixel measure to a glyphon text bound.
///
/// Bounds are `i32` clip edges. Negative input clamps to zero and anything
/// past `i32::MAX` saturates, neither of which a real strip geometry can
/// produce.
pub fn pixel_to_bound(value: f32) -> i32 {
    i32::try_from(nearest_u32(f64::from(value))).unwrap_or(i32::MAX)
}

/// Converts a surface dimension to a glyphon text bound.
pub fn dimension_to_bound(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// The number of whole character cells that fit in `width` pixels.
///
/// Zero when the character width is not positive — a font that has not
/// measured yet must not turn a division into an absurd cell count.
pub fn pixels_to_cells(width: f32, char_width: f32) -> usize {
    if char_width <= 0.0 {
        return 0;
    }
    let cells = (width / char_width).floor();
    usize::try_from(nearest_u32(f64::from(cells))).unwrap_or(usize::MAX)
}

/// Converts a window scale factor to `f32` for font-size arithmetic.
///
/// Scale factors are small positive rationals — 1.0 and 2.0 in practice, with
/// fractional steps like 1.25 or 1.5 on some platforms — and every one of
/// those is exact in `f32`. The conversion goes through 1/65536 fixed point:
/// the factor is scaled up, rounded to the nearest integer read from the
/// `f64`'s own bits, converted exactly by [`u32_to_f32`], and divided back
/// down by a power of two, which is again exact. Values a display could never
/// report are still defined: `NaN` and negatives land on `0.0`, and anything
/// at or beyond 65536 saturates.
pub fn scale_to_f32(scale: f64) -> f32 {
    u32_to_f32(nearest_u32(scale * 65_536.0)) / 65_536.0
}

/// The `u32` nearest to a non-negative `f64`, read from its bits.
///
/// Adds one half and truncates, so ties round up; `NaN`, negatives and
/// everything below one half land on zero, and values at or beyond 2^32
/// saturate at [`u32::MAX`]. The truncation reads the exponent and mantissa
/// directly — the same statement-as-arithmetic the kernel's private `units`
/// module makes — rather than hiding the narrowing in a cast.
fn nearest_u32(value: f64) -> u32 {
    let rounded = value + 0.5;
    if rounded.is_nan() || rounded < 1.0 {
        return 0;
    }
    let bits = rounded.to_bits();
    // The biased exponent occupies 11 bits, so the conversion cannot fail;
    // saturating high would only trip the >= 32 check below in any case.
    let exponent = i64::try_from((bits >> 52) & 0x7FF).unwrap_or(i64::MAX) - 1023;
    let Ok(shift) = u32::try_from(exponent) else {
        // Unreachable for inputs >= 1.0; zero is the honest floor regardless.
        return 0;
    };
    if shift >= 32 {
        return u32::MAX;
    }
    let mantissa = (bits & 0x000F_FFFF_FFFF_FFFF) | (1 << 52);
    // `shift < 32 <= 52`, so the integer part is `mantissa >> (52 - shift)`,
    // which fits in 32 bits by the saturation check above.
    u32::try_from(mantissa >> (52 - shift)).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::{
        dimension_to_bound, index_to_f32, pixel_from_f64, pixel_to_bound, pixels_to_cells,
        scale_to_f32, u32_to_f32,
    };

    /// Grid indices convert exactly across the range a session can reach.
    #[test]
    fn indices_convert_exactly() {
        for value in [0_usize, 1, 79, 1_000, 65_536, (1 << 24) - 1] {
            let expected = u32::try_from(value).unwrap();
            assert_eq!(
                index_to_f32(value).to_bits(),
                u32_to_f32(expected).to_bits()
            );
        }
    }

    /// Cursor coordinates survive the trip from winit's `f64`.
    #[test]
    fn cursor_coordinates_convert_exactly() {
        for (input, expected) in [
            (0.0_f64, 0.0_f32),
            (1.0, 1.0),
            (799.5, 799.5),
            (2_559.25, 2_559.25),
            (16_383.0, 16_383.0),
        ] {
            assert_eq!(pixel_from_f64(input).to_bits(), expected.to_bits());
        }
    }

    /// Coordinates no pointer can report still land somewhere defined.
    #[test]
    fn degenerate_cursor_coordinates_clamp() {
        assert_eq!(pixel_from_f64(-5.0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(pixel_from_f64(f64::NAN).to_bits(), 0.0_f32.to_bits());
        assert!(pixel_from_f64(f64::INFINITY).is_finite());
    }

    /// Bounds clamp instead of wrapping.
    #[test]
    fn bounds_are_clamped() {
        assert_eq!(pixel_to_bound(0.0), 0);
        assert_eq!(pixel_to_bound(10.4), 10);
        assert_eq!(pixel_to_bound(-3.0), 0);
        assert_eq!(dimension_to_bound(1_080), 1_080);
        assert_eq!(dimension_to_bound(u32::MAX), i32::MAX);
    }

    /// Row conversions truncate toward zero and stay defined off the rails.
    ///
    /// Kept after this face stopped defining `pixel_to_index` itself: these
    /// are the values this face's geometry depends on, and they should fail
    /// here — not only in the kernel's own suite — if the shared conversion
    /// ever moves under it.
    ///
    /// The infinity case is the one that changed on 2026-08-06, and it is
    /// the record of why the duplicate had to go. This face's own copy
    /// saturated at `u32::MAX`, so that is what this line asserted. The
    /// kernel's saturates at `usize::MAX`, which is the specified behaviour:
    /// `pixel_to_index` is documented as bit-for-bit `value as usize`, and
    /// `f32::INFINITY as usize` is `usize::MAX`. Every input at or above
    /// 2^32 disagreed between the two, and nothing caught it because no
    /// caller here has ever produced one.
    #[test]
    fn pixel_indices_truncate_and_clamp() {
        use super::pixel_to_index;
        assert_eq!(pixel_to_index(0.0), 0);
        assert_eq!(pixel_to_index(0.9), 0);
        assert_eq!(pixel_to_index(1.0), 1);
        assert_eq!(pixel_to_index(41.99), 41);
        assert_eq!(pixel_to_index(16_383.5), 16_383);
        assert_eq!(pixel_to_index(-3.0), 0);
        assert_eq!(pixel_to_index(f32::NAN), 0);
        assert_eq!(pixel_to_index(f32::INFINITY), usize::MAX);
        // The boundary the old copy sat on, pinned so a future re-divergence
        // lands on an assertion rather than on a caret.
        assert_eq!(pixel_to_index(4_294_967_296.0), 4_294_967_296);
    }

    /// Cell counts floor, and a degenerate character width yields none.
    #[test]
    fn cell_counts_floor() {
        assert_eq!(pixels_to_cells(100.0, 10.0), 10);
        assert_eq!(pixels_to_cells(99.9, 10.0), 9);
        assert_eq!(pixels_to_cells(5.0, 10.0), 0);
        assert_eq!(pixels_to_cells(100.0, 0.0), 0);
        assert_eq!(pixels_to_cells(100.0, -1.0), 0);
    }

    /// Every dimension a surface can realistically take converts exactly.
    #[test]
    fn u32_conversion_is_exact_below_the_significand_limit() {
        for value in [0_u32, 1, 2, 799, 800, 1_080, 3_840, 16_384, (1 << 24) - 1] {
            let converted = u32_to_f32(value);
            assert_eq!(
                f64::from(converted).to_bits(),
                f64::from(value).to_bits(),
                "{value} should convert exactly"
            );
        }
    }

    /// Beyond the significand limit the result is still the nearest `f32`.
    #[test]
    fn u32_conversion_rounds_correctly_at_the_extremes() {
        for value in [1_u32 << 24, (1 << 24) + 1, u32::MAX - 1, u32::MAX] {
            let converted = u32_to_f32(value);
            let exact = f64::from(value);
            let error = (f64::from(converted) - exact).abs();
            let bits = converted.to_bits();
            for neighbour in [f32::from_bits(bits + 1), f32::from_bits(bits - 1)] {
                assert!(
                    (f64::from(neighbour) - exact).abs() >= error,
                    "{value} rounds to a value that is not the nearest f32"
                );
            }
        }
    }

    /// The scale factors real displays report convert exactly.
    #[test]
    fn real_scale_factors_convert_exactly() {
        for (scale, expected) in [
            (1.0_f64, 1.0_f32),
            (1.25, 1.25),
            (1.5, 1.5),
            (1.75, 1.75),
            (2.0, 2.0),
            (3.0, 3.0),
            (0.5, 0.5),
        ] {
            assert_eq!(
                scale_to_f32(scale).to_bits(),
                expected.to_bits(),
                "{scale} should convert exactly"
            );
        }
    }

    /// Values no display can report still land somewhere defined.
    #[test]
    fn degenerate_scale_factors_are_defined() {
        assert_eq!(scale_to_f32(f64::NAN).to_bits(), 0.0_f32.to_bits());
        assert_eq!(scale_to_f32(-2.0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(scale_to_f32(0.0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(
            scale_to_f32(f64::INFINITY).to_bits(),
            65_536.0_f32.to_bits()
        );
        assert_eq!(scale_to_f32(1e300).to_bits(), 65_536.0_f32.to_bits());
    }
}
