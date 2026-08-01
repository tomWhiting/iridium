//! Exact integer-to-pixel conversions.
//!
//! Layout arithmetic is done in `f32` because that is what the GPU consumes, but
//! the quantities that feed it — line indices, column indices, glyph counts,
//! surface dimensions — are integers. Writing `value as f32` there is a silent
//! lossy conversion: `f32` has a 24-bit significand, so every integer above
//! 2^24 that is not a multiple of the local exponent step is rounded, and the
//! cast gives no indication that it happened.
//!
//! The two functions here perform the same conversion *explicitly* and, over the
//! whole of `u32`, with the same result. `u32` is split into two 16-bit halves,
//! each of which is representable in `f32` without loss, and recombined with a
//! fused multiply-add. `mul_add` rounds once, at the end, so the result is the
//! `f32` nearest to the original integer — what `value as f32` produces, but
//! stated as arithmetic rather than as a cast.

/// Converts a `u32` to the nearest `f32`.
///
/// Values below 2^24 are represented exactly; above that the result is the
/// correctly rounded nearest `f32`, which is the best any `f32` can do.
pub fn u32_to_f32(value: u32) -> f32 {
    let [b0, b1, b2, b3] = value.to_le_bytes();
    let low = u16::from_le_bytes([b0, b1]);
    let high = u16::from_le_bytes([b2, b3]);

    // `high` and `low` are both below 2^16, so `f32::from` is exact for each.
    // `mul_add` computes `high * 65536 + low` with a single rounding step, which
    // makes the result the correctly rounded image of the original `u32`.
    f32::from(high).mul_add(65_536.0, f32::from(low))
}

/// Converts an index or count to the nearest `f32` for pixel arithmetic.
///
/// Values up to [`u32::MAX`] convert as described on [`u32_to_f32`]. Larger
/// values saturate at [`u32::MAX`]; reaching that requires a document of more
/// than four billion lines or columns, at which point the `f32` pixel
/// coordinate it would feed is meaningless in any case.
pub fn index_to_f32(value: usize) -> f32 {
    u32_to_f32(u32::try_from(value).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use super::{index_to_f32, u32_to_f32};

    /// The first `u32` whose `f32` image needs rounding.
    const SIGNIFICAND_LIMIT: u32 = 1 << 24;

    /// Asserts that no `f32` is closer to `value` than `u32_to_f32(value)` is.
    ///
    /// Both operands are widened to `f64`, which is lossless for `u32` and for
    /// `f32` alike, so the comparison is exact.
    fn assert_correctly_rounded(value: u32) {
        let result = u32_to_f32(value);
        assert!(result.is_finite(), "non-finite result for {value}");
        assert!(result >= 0.0, "negative result for {value}");

        let exact = f64::from(value);
        let error = (f64::from(result) - exact).abs();

        // For a positive finite `f32`, incrementing or decrementing the bit
        // pattern steps to the adjacent representable value.
        let bits = result.to_bits();
        let neighbours = [
            f32::from_bits(bits.saturating_add(1)),
            f32::from_bits(bits.saturating_sub(1)),
        ];
        for neighbour in neighbours {
            if !neighbour.is_finite() {
                continue;
            }
            assert!(
                (f64::from(neighbour) - exact).abs() >= error,
                "{value} rounds to a value that is not the nearest f32"
            );
        }

        // Below the significand limit the conversion must be exact, not merely
        // nearest. Comparing bit patterns of the widened values avoids any
        // reliance on float equality semantics.
        if value < SIGNIFICAND_LIMIT {
            assert_eq!(
                f64::from(result).to_bits(),
                exact.to_bits(),
                "{value} should convert exactly"
            );
        }
    }

    /// Every value in a dense low range converts correctly.
    #[test]
    fn converts_dense_low_range() {
        for value in 0_u32..=200_000 {
            assert_correctly_rounded(value);
        }
    }

    /// Rounding boundaries live next to the powers of two; check both sides.
    #[test]
    fn converts_around_powers_of_two() {
        for exponent in 0..32_u32 {
            let base = 1_u32 << exponent;
            for delta in 0..1_024_u32 {
                assert_correctly_rounded(base.wrapping_sub(delta));
                assert_correctly_rounded(base.wrapping_add(delta));
            }
        }
    }

    /// A deterministic spread across the full 32-bit range.
    #[test]
    fn converts_across_full_range() {
        let mut state: u64 = 0x243F_6A88_85A3_08D3;
        for _ in 0..200_000_u32 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let value = u32::try_from(state & u64::from(u32::MAX)).unwrap_or(u32::MAX);
            assert_correctly_rounded(value);
        }
    }

    /// The extremes of the domain are handled without special-casing.
    #[test]
    fn converts_domain_extremes() {
        assert_correctly_rounded(0);
        assert_correctly_rounded(1);
        assert_correctly_rounded(SIGNIFICAND_LIMIT - 1);
        assert_correctly_rounded(SIGNIFICAND_LIMIT);
        assert_correctly_rounded(SIGNIFICAND_LIMIT + 1);
        assert_correctly_rounded(u32::MAX);
    }

    /// `usize` inputs beyond `u32::MAX` saturate rather than wrapping.
    #[test]
    fn index_saturates_beyond_u32_range() {
        let max_u32 = usize::try_from(u32::MAX).unwrap_or(usize::MAX);

        assert_eq!(index_to_f32(0).to_bits(), 0.0_f32.to_bits());
        assert_eq!(index_to_f32(1).to_bits(), u32_to_f32(1).to_bits());
        assert_eq!(
            index_to_f32(max_u32).to_bits(),
            u32_to_f32(u32::MAX).to_bits()
        );

        if let Some(beyond) = max_u32.checked_add(1) {
            assert_eq!(
                index_to_f32(beyond).to_bits(),
                u32_to_f32(u32::MAX).to_bits()
            );
        }
    }
}
