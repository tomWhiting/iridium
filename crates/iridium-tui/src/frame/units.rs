//! Total conversions between cell counts, the kernel's `f32` geometry, and
//! colour channels.
//!
//! The kernel expresses layout in `f32` pixels and colour in `f32` components,
//! and the terminal counts whole cells and 8-bit channels. Every conversion
//! between the two is here, written so that it cannot truncate silently,
//! cannot lose a sign, and has a defined answer for a NaN or an infinity —
//! none of which a bare `as` cast gives, which is why there is not one in this
//! crate.

use crate::cell::MAX_DIMENSION;

/// A cell count as the kernel's pixel geometry, with one cell to one unit.
///
/// Exact for every value a buffer can hold: [`MAX_DIMENSION`] is `u16::MAX`,
/// and every `u16` converts to `f32` without loss.
pub fn cell_units(cells: usize) -> f32 {
    f32::from(u16::try_from(cells.min(MAX_DIMENSION)).unwrap_or(u16::MAX))
}

/// The largest whole number of cells that does not exceed `units`.
///
/// A negative value, a NaN and a value below one all give zero; a value above
/// [`MAX_DIMENSION`] gives `MAX_DIMENSION`. The search is over `u16` because
/// that is the whole range a buffer dimension can take, and every step of it
/// converts to `f32` exactly, so the comparison is not itself lossy.
pub fn whole_cells(units: f32) -> usize {
    if units.is_nan() || units < 1.0 {
        return 0;
    }
    let mut low: u16 = 1;
    let mut high: u16 = u16::MAX;
    if units >= f32::from(high) {
        return usize::from(high);
    }
    while low < high {
        let midpoint = low + (high - low).div_ceil(2);
        if f32::from(midpoint) <= units {
            low = midpoint;
        } else {
            high = midpoint - 1;
        }
    }
    usize::from(low)
}

/// A colour component in `0.0..=1.0` as an 8-bit channel.
///
/// Values outside the range are clamped rather than wrapped: a theme file is
/// user input, and a component of `-1.0` must darken the colour, not brighten
/// it by wrapping to 255.
pub fn channel(component: f32) -> u8 {
    let scaled = component.clamp(0.0, 1.0) * 255.0;
    u8::try_from(whole_cells(scaled.round())).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_round_trip_through_pixel_units() {
        for cells in [0_usize, 1, 2, 79, 80, 1000, MAX_DIMENSION] {
            assert_eq!(whole_cells(cell_units(cells)), cells, "{cells}");
        }
    }

    #[test]
    fn a_cell_count_beyond_the_maximum_is_clamped() {
        // Compared by bit pattern rather than by `==`: the assertion is that
        // clamping produces the *same* value, and bitwise identity states that
        // exactly, without asking whether two floats are near enough.
        assert_eq!(
            cell_units(MAX_DIMENSION + 1).to_bits(),
            cell_units(MAX_DIMENSION).to_bits()
        );
        assert_eq!(whole_cells(1e9), MAX_DIMENSION);
    }

    #[test]
    fn whole_cells_floors_rather_than_rounding() {
        assert_eq!(whole_cells(3.9), 3);
        assert_eq!(whole_cells(4.0), 4);
        assert_eq!(whole_cells(0.9), 0);
    }

    #[test]
    fn whole_cells_has_an_answer_for_every_float() {
        assert_eq!(whole_cells(f32::NAN), 0);
        assert_eq!(whole_cells(f32::NEG_INFINITY), 0);
        assert_eq!(whole_cells(-17.5), 0);
        assert_eq!(whole_cells(f32::INFINITY), MAX_DIMENSION);
    }

    #[test]
    fn channels_span_the_whole_byte_range() {
        assert_eq!(channel(0.0), 0);
        assert_eq!(channel(1.0), 255);
        assert_eq!(channel(0.5), 128);
    }

    #[test]
    fn channels_clamp_rather_than_wrap() {
        assert_eq!(channel(-1.0), 0);
        assert_eq!(channel(2.0), 255);
        assert_eq!(channel(f32::NAN), 0);
    }
}
