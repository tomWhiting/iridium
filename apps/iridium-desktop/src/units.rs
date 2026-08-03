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
    use super::{scale_to_f32, u32_to_f32};

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
