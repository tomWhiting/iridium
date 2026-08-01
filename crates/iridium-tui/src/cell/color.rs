//! Colour, and its degradation to terminals that cannot show all of it.
//!
//! A cell always stores the colour the theme asked for, at full 24-bit
//! precision. Degradation happens once, at the moment the damage diff is
//! computed, because the diff must compare colours *in the space they were
//! actually sent in*: on a 16-colour terminal two distinct truecolours that
//! collapse onto the same ANSI colour are not a visible change, and repainting
//! them would be pure waste.
//!
//! # Mapping rules
//!
//! Truecolour to the 256-colour palette picks the nearer of two candidates:
//! the closest entry of the 6x6x6 colour cube (indices 16..=231) and the
//! closest step of the 24-step grey ramp (indices 232..=255). The lowest 16
//! entries are deliberately **not** candidates — they are the ones a user's
//! terminal configuration is most likely to have redefined, so their real
//! on-screen colour is unknown and mapping onto them would be a guess. The
//! cube and the ramp have fixed, specified values.
//!
//! Truecolour to 16 colours has no such luxury and picks the nearest of the
//! xterm defaults for indices 0..=15.
//!
//! Distance is squared Euclidean distance in RGB. It is separable, which is
//! what makes the cube lookup an exact per-channel choice rather than a search
//! over 216 candidates; a perceptual metric would be marginally prettier and
//! would cost a 240-candidate scan per colour per frame.

/// How much colour the terminal being painted can actually show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ColorDepth {
    /// 24-bit direct colour. Nothing is degraded.
    #[default]
    TrueColor,
    /// The 256-entry indexed palette. Truecolour is mapped onto the colour
    /// cube or the grey ramp; palette indices pass through untouched.
    Ansi256,
    /// The 16 basic ANSI colours. Everything is mapped onto indices 0..=15.
    Ansi16,
}

/// A colour a cell can be painted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Color {
    /// The terminal's own default foreground or background (`SGR 39`/`SGR 49`).
    ///
    /// This is not the same as black or white: it is whatever the user's
    /// terminal is configured to use, and it survives every degradation tier
    /// unchanged.
    #[default]
    Default,
    /// An index into the terminal's 256-colour palette.
    Indexed(u8),
    /// 24-bit direct colour.
    Rgb(u8, u8, u8),
}

impl Color {
    /// A 24-bit colour from its red, green and blue components.
    pub const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self::Rgb(red, green, blue)
    }

    /// The red, green and blue components of this colour.
    ///
    /// Returns `None` for [`Color::Default`], whose on-screen value is decided
    /// by the terminal and cannot be known here.
    pub fn to_rgb(self) -> Option<(u8, u8, u8)> {
        match self {
            Self::Default => None,
            Self::Indexed(index) => Some(palette_rgb(index)),
            Self::Rgb(red, green, blue) => Some((red, green, blue)),
        }
    }

    /// This colour as the nearest colour the given terminal depth can show.
    ///
    /// Degrading an already-degraded colour is a no-op, so this is safe to
    /// apply more than once.
    #[must_use]
    pub fn degrade(self, depth: ColorDepth) -> Self {
        match depth {
            ColorDepth::TrueColor => self,
            ColorDepth::Ansi256 => match self {
                Self::Rgb(red, green, blue) => Self::Indexed(nearest_palette(red, green, blue)),
                Self::Default | Self::Indexed(_) => self,
            },
            ColorDepth::Ansi16 => match self {
                Self::Default => self,
                Self::Indexed(index) if index < 16 => self,
                Self::Indexed(index) => Self::Indexed(nearest_ansi16(palette_rgb(index))),
                Self::Rgb(red, green, blue) => Self::Indexed(nearest_ansi16((red, green, blue))),
            },
        }
    }
}

/// The six values a channel of the 6x6x6 colour cube can take.
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// The xterm default RGB values of the sixteen basic ANSI colours.
///
/// A terminal may have been configured to draw these differently; they are
/// used only where there is no alternative, which is the 16-colour tier.
const ANSI16_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (128, 0, 0),
    (0, 128, 0),
    (128, 128, 0),
    (0, 0, 128),
    (128, 0, 128),
    (0, 128, 128),
    (192, 192, 192),
    (128, 128, 128),
    (255, 0, 0),
    (0, 255, 0),
    (255, 255, 0),
    (0, 0, 255),
    (255, 0, 255),
    (0, 255, 255),
    (255, 255, 255),
];

/// The index of the nearest cube level to `value`, in `0..=5`.
///
/// The comparisons are the midpoints between adjacent entries of
/// [`CUBE_LEVELS`]; a value exactly on a midpoint takes the lower level.
const fn nearest_cube_level(value: u8) -> u8 {
    if value <= 47 {
        0
    } else if value <= 115 {
        1
    } else if value <= 155 {
        2
    } else if value <= 195 {
        3
    } else if value <= 235 {
        4
    } else {
        5
    }
}

/// The value a cube level index stands for.
const fn cube_level_value(level: u8) -> u8 {
    match level {
        0 => CUBE_LEVELS[0],
        1 => CUBE_LEVELS[1],
        2 => CUBE_LEVELS[2],
        3 => CUBE_LEVELS[3],
        4 => CUBE_LEVELS[4],
        _ => CUBE_LEVELS[5],
    }
}

/// The index of the grey-ramp step nearest to the colour whose three channels
/// sum to `channel_sum`, in `0..=23`.
///
/// The grey that best approximates a colour is the one at its channel mean:
/// the sum of squared channel differences is convex in the grey's value and is
/// minimised there. The mean is not generally an integer, so the comparison
/// happens in tripled space — step `k` is the grey `8 + 10k`, tripled
/// `24 + 30k` — rather than rounding the mean first and losing the answer.
///
/// A colour exactly between two steps takes the darker one, matching how
/// [`nearest_cube_level`] breaks its ties.
fn nearest_grey_step(channel_sum: u16) -> u8 {
    if channel_sum <= 24 {
        return 0;
    }
    let step = (channel_sum - 24 + 14) / 30;
    u8::try_from(step.min(23)).unwrap_or(23)
}

/// The grey a ramp step stands for.
const fn grey_step_value(step: u8) -> u8 {
    // `step` is at most 23, so this cannot overflow.
    8 + 10 * step
}

/// The squared Euclidean distance between two colours in RGB.
fn squared_distance(left: (u8, u8, u8), right: (u8, u8, u8)) -> u32 {
    let red = u32::from(left.0.abs_diff(right.0));
    let green = u32::from(left.1.abs_diff(right.1));
    let blue = u32::from(left.2.abs_diff(right.2));
    red * red + green * green + blue * blue
}

/// The RGB value of a 256-colour palette index.
fn palette_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0..=15 => ANSI16_RGB[usize::from(index)],
        16..=231 => {
            let offset = index - 16;
            (
                cube_level_value(offset / 36),
                cube_level_value((offset / 6) % 6),
                cube_level_value(offset % 6),
            )
        },
        232..=255 => {
            let grey = grey_step_value(index - 232);
            (grey, grey, grey)
        },
    }
}

/// The palette index that best matches a 24-bit colour.
///
/// Only the colour cube and the grey ramp are candidates; see the module
/// documentation for why the lowest sixteen entries are excluded. A tie is
/// resolved in favour of the cube, whose entries are the more saturated of the
/// two and so the less likely to flatten a colour into grey.
fn nearest_palette(red: u8, green: u8, blue: u8) -> u8 {
    let cube_level = (
        nearest_cube_level(red),
        nearest_cube_level(green),
        nearest_cube_level(blue),
    );
    let cube_rgb = (
        cube_level_value(cube_level.0),
        cube_level_value(cube_level.1),
        cube_level_value(cube_level.2),
    );
    let cube_index = 16 + 36 * cube_level.0 + 6 * cube_level.1 + cube_level.2;

    let channel_sum = u16::from(red) + u16::from(green) + u16::from(blue);
    let grey_step = nearest_grey_step(channel_sum);
    let grey = grey_step_value(grey_step);
    let grey_index = 232 + grey_step;

    let source = (red, green, blue);
    if squared_distance(source, (grey, grey, grey)) < squared_distance(source, cube_rgb) {
        grey_index
    } else {
        cube_index
    }
}

/// The basic ANSI colour index that best matches a 24-bit colour.
///
/// A tie is resolved in favour of the lower index, which makes the mapping
/// total and deterministic.
fn nearest_ansi16(color: (u8, u8, u8)) -> u8 {
    let mut best_index = 0u8;
    let mut best_distance = u32::MAX;
    for index in 0u8..16 {
        let distance = squared_distance(color, ANSI16_RGB[usize::from(index)]);
        if distance < best_distance {
            best_distance = distance;
            best_index = index;
        }
    }
    best_index
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The nearest cube level, found by exhaustive search rather than by the
    /// midpoint comparisons the implementation uses.
    fn brute_force_cube_level(value: u8) -> u8 {
        let mut best = 0u8;
        let mut best_distance = u16::MAX;
        for (index, level) in CUBE_LEVELS.iter().enumerate() {
            let distance = u16::from(value.abs_diff(*level));
            if distance < best_distance {
                best_distance = distance;
                best = u8::try_from(index).unwrap_or(0);
            }
        }
        best
    }

    /// The nearest palette entry, found by exhaustive search over the cube and
    /// the grey ramp.
    fn brute_force_palette(color: (u8, u8, u8)) -> u8 {
        let mut best = 16u8;
        let mut best_distance = u32::MAX;
        for index in 16u8..=255 {
            let distance = squared_distance(color, palette_rgb(index));
            if distance < best_distance {
                best_distance = distance;
                best = index;
            }
        }
        best
    }

    #[test]
    fn cube_level_matches_exhaustive_search() {
        for value in 0u8..=255 {
            assert_eq!(
                nearest_cube_level(value),
                brute_force_cube_level(value),
                "cube level for {value}"
            );
        }
    }

    #[test]
    fn grey_step_matches_exhaustive_search() {
        for channel_sum in 0u16..=765 {
            let mut best = 0u8;
            let mut best_distance = u16::MAX;
            for step in 0u8..24 {
                let tripled = 3 * u16::from(grey_step_value(step));
                let distance = channel_sum.abs_diff(tripled);
                if distance < best_distance {
                    best_distance = distance;
                    best = step;
                }
            }
            assert_eq!(
                nearest_grey_step(channel_sum),
                best,
                "grey step for a channel sum of {channel_sum}"
            );
        }
    }

    #[test]
    fn palette_round_trips_through_its_own_rgb() {
        // Every cube and ramp entry must map back onto itself, or the diff
        // would repaint cells that are already correct.
        for index in 16u8..=255 {
            let rgb = palette_rgb(index);
            assert_eq!(
                nearest_palette(rgb.0, rgb.1, rgb.2),
                index,
                "palette index {index} with rgb {rgb:?}"
            );
        }
    }

    #[test]
    fn palette_mapping_is_never_worse_than_exhaustive_search() {
        // The separable per-channel cube choice plus the grey candidate must
        // be exactly as good as scanning all 240 candidates.
        for red in (0u16..=255).step_by(17) {
            for green in (0u16..=255).step_by(11) {
                for blue in (0u16..=255).step_by(7) {
                    let color = (
                        u8::try_from(red).unwrap_or(0),
                        u8::try_from(green).unwrap_or(0),
                        u8::try_from(blue).unwrap_or(0),
                    );
                    let ours = nearest_palette(color.0, color.1, color.2);
                    let best = brute_force_palette(color);
                    assert_eq!(
                        squared_distance(color, palette_rgb(ours)),
                        squared_distance(color, palette_rgb(best)),
                        "colour {color:?} mapped to {ours} but {best} is nearer"
                    );
                }
            }
        }
    }

    #[test]
    fn truecolor_depth_changes_nothing() {
        let colors = [
            Color::Default,
            Color::Indexed(0),
            Color::Indexed(200),
            Color::rgb(1, 2, 3),
        ];
        for color in colors {
            assert_eq!(color.degrade(ColorDepth::TrueColor), color);
        }
    }

    #[test]
    fn default_survives_every_depth() {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
        ] {
            assert_eq!(Color::Default.degrade(depth), Color::Default);
        }
    }

    #[test]
    fn rgb_degrades_to_a_palette_index_at_256_colours() {
        // Pure red is an exact cube entry: 5,0,0 -> 16 + 180 = 196.
        assert_eq!(
            Color::rgb(255, 0, 0).degrade(ColorDepth::Ansi256),
            Color::Indexed(196)
        );
        // A mid grey is nearer the ramp than any cube entry.
        assert_eq!(
            Color::rgb(118, 118, 118).degrade(ColorDepth::Ansi256),
            Color::Indexed(243)
        );
    }

    #[test]
    fn palette_indices_pass_through_at_256_colours() {
        for index in [0u8, 15, 16, 231, 232, 255] {
            assert_eq!(
                Color::Indexed(index).degrade(ColorDepth::Ansi256),
                Color::Indexed(index)
            );
        }
    }

    #[test]
    fn rgb_degrades_to_a_basic_colour_at_16_colours() {
        assert_eq!(
            Color::rgb(255, 0, 0).degrade(ColorDepth::Ansi16),
            Color::Indexed(9)
        );
        assert_eq!(
            Color::rgb(130, 0, 0).degrade(ColorDepth::Ansi16),
            Color::Indexed(1)
        );
        assert_eq!(
            Color::rgb(0, 0, 0).degrade(ColorDepth::Ansi16),
            Color::Indexed(0)
        );
        assert_eq!(
            Color::rgb(255, 255, 255).degrade(ColorDepth::Ansi16),
            Color::Indexed(15)
        );
    }

    #[test]
    fn basic_colours_pass_through_at_16_colours() {
        for index in 0u8..16 {
            assert_eq!(
                Color::Indexed(index).degrade(ColorDepth::Ansi16),
                Color::Indexed(index)
            );
        }
    }

    #[test]
    fn high_palette_indices_collapse_onto_basic_colours_at_16_colours() {
        for index in 16u8..=255 {
            let degraded = Color::Indexed(index).degrade(ColorDepth::Ansi16);
            match degraded {
                Color::Indexed(basic) => assert!(basic < 16, "index {index} became {basic}"),
                other => panic!("index {index} became {other:?}"),
            }
        }
        // Cube entry 196 is pure red and must land on bright red.
        assert_eq!(
            Color::Indexed(196).degrade(ColorDepth::Ansi16),
            Color::Indexed(9)
        );
    }

    #[test]
    fn degradation_is_idempotent() {
        for depth in [ColorDepth::Ansi256, ColorDepth::Ansi16] {
            for red in (0u16..=255).step_by(23) {
                for green in (0u16..=255).step_by(29) {
                    for blue in (0u16..=255).step_by(31) {
                        let color = Color::rgb(
                            u8::try_from(red).unwrap_or(0),
                            u8::try_from(green).unwrap_or(0),
                            u8::try_from(blue).unwrap_or(0),
                        );
                        let once = color.degrade(depth);
                        assert_eq!(once.degrade(depth), once, "{color:?} at {depth:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn to_rgb_reports_the_palette_value() {
        assert_eq!(Color::Default.to_rgb(), None);
        assert_eq!(Color::rgb(4, 5, 6).to_rgb(), Some((4, 5, 6)));
        assert_eq!(Color::Indexed(196).to_rgb(), Some((255, 0, 0)));
        assert_eq!(Color::Indexed(232).to_rgb(), Some((8, 8, 8)));
        assert_eq!(Color::Indexed(255).to_rgb(), Some((238, 238, 238)));
    }
}
