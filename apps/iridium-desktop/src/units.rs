//! Exact numeric conversions for the values winit hands this face.
//!
//! Layout arithmetic ends in `f32` because that is what the compositor
//! consumes, but the quantities winit supplies — physical surface dimensions
//! as `u32`, the scale factor as `f64` — do not start there, and `value as
//! f32` is a silent lossy cast this workspace lints against. The kernel
//! solved this once in `iridium-editor/src/render/units.rs`, but that module
//! is private to the kernel, so the two conversions this face needs are
//! restated here with the same technique: split the value into halves each
//! half representable exactly, recombine with one `mul_add`, and let the
//! single final rounding be the correctly rounded answer.

/// Converts a `u32` to the nearest `f32`.
///
/// Values below 2^24 — every real surface dimension — convert exactly; above
/// that the result is the correctly rounded nearest `f32`, which is the best
/// any `f32` can do. The `u32` is split into two 16-bit halves, each exact in
/// `f32`, and recombined with a fused multiply-add that rounds once.
pub fn u32_to_f32(value: u32) -> f32 {
    let [b0, b1, b2, b3] = value.to_le_bytes();
    let low = u16::from_le_bytes([b0, b1]);
    let high = u16::from_le_bytes([b2, b3]);
    f32::from(high).mul_add(65_536.0, f32::from(low))
}

/// Converts a line or column index to `f32` for grid arithmetic.
///
/// Indices up to 2^24 — far past any line a caret can reach in a real session
/// — convert exactly; past that the value saturates through `u32` and rounds
/// once, which is the same precision envelope the pixel pipeline itself lives
/// in at those magnitudes.
pub fn index_to_f32(value: usize) -> f32 {
    u32_to_f32(u32::try_from(value).unwrap_or(u32::MAX))
}

/// Converts a non-negative pixel quantity to a whole count of rows or
/// columns: truncation toward zero, with `NaN` and negatives landing on
/// zero and values past the integer range saturating.
///
/// The inverse direction of [`index_to_f32`], for the same reason: viewport
/// math divides a scroll offset by a line height and needs the resulting row
/// as an integer, and `value as usize` would perform that narrowing silently.
/// The value is floored — already integral thereafter — and read exactly
/// through the module's bit-level rounding.
pub fn pixel_to_index(value: f32) -> usize {
    usize::try_from(nearest_u32(f64::from(value.floor()))).unwrap_or(usize::MAX)
}

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
        assert_eq!(
            pixel_to_index(f32::INFINITY),
            usize::try_from(u32::MAX).unwrap()
        );
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
