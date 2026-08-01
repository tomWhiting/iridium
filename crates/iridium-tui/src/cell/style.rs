//! The style a cell is painted with: two colours and a set of attributes.

use core::fmt;
use core::ops::{BitOr, BitOrAssign};

use super::color::{Color, ColorDepth};

/// The text attributes a cell can carry, as a bit set.
///
/// These are the attributes that every terminal worth supporting can express
/// as a single SGR parameter. Anything richer — underline colour, curly
/// underlines, hyperlinks — is a capability question and belongs to the
/// driver, not to the cell.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Attributes(u8);

impl Attributes {
    /// No attributes at all.
    pub const NONE: Self = Self(0);
    /// Bold, `SGR 1`.
    pub const BOLD: Self = Self(1 << 0);
    /// Faint, `SGR 2`.
    pub const DIM: Self = Self(1 << 1);
    /// Italic, `SGR 3`.
    pub const ITALIC: Self = Self(1 << 2);
    /// Underlined, `SGR 4`.
    pub const UNDERLINE: Self = Self(1 << 3);
    /// Foreground and background swapped, `SGR 7`.
    pub const REVERSE: Self = Self(1 << 4);
    /// Struck through, `SGR 9`.
    pub const STRIKETHROUGH: Self = Self(1 << 5);
    /// Every attribute this type can express.
    pub const ALL: Self = Self(0b0011_1111);

    /// Every attribute paired with its name, for [`fmt::Debug`].
    const NAMED: [(Self, &'static str); 6] = [
        (Self::BOLD, "BOLD"),
        (Self::DIM, "DIM"),
        (Self::ITALIC, "ITALIC"),
        (Self::UNDERLINE, "UNDERLINE"),
        (Self::REVERSE, "REVERSE"),
        (Self::STRIKETHROUGH, "STRIKETHROUGH"),
    ];

    /// The raw bits of this set.
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// A set from raw bits, discarding any bit this type does not define.
    pub const fn from_bits_truncate(bits: u8) -> Self {
        Self(bits & Self::ALL.0)
    }

    /// Whether every attribute in `other` is present in this set.
    ///
    /// An empty `other` is contained in every set.
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether this set is empty.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// This set with every attribute in `other` added.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The attributes present in both sets.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// This set with every attribute in `other` removed.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

impl BitOr for Attributes {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl BitOrAssign for Attributes {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

impl fmt::Debug for Attributes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Attributes(")?;
        if self.is_empty() {
            f.write_str("NONE")?;
        } else {
            let mut written = false;
            for (flag, name) in Self::NAMED {
                if self.contains(flag) {
                    if written {
                        f.write_str(" | ")?;
                    }
                    f.write_str(name)?;
                    written = true;
                }
            }
        }
        f.write_str(")")
    }
}

/// Everything about a cell except its content.
///
/// Both halves of a double-width glyph always carry the same style: the
/// terminal paints the whole glyph with whatever was in effect when its
/// leading half was written, so letting the halves disagree would describe a
/// screen that cannot exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// The colour the glyph is drawn in.
    pub foreground: Color,
    /// The colour behind the glyph.
    pub background: Color,
    /// The attributes applied to the glyph.
    pub attributes: Attributes,
}

impl Style {
    /// The terminal's own colours with no attributes.
    pub const DEFAULT: Self = Self {
        foreground: Color::Default,
        background: Color::Default,
        attributes: Attributes::NONE,
    };

    /// A style from its three parts.
    pub const fn new(foreground: Color, background: Color, attributes: Attributes) -> Self {
        Self {
            foreground,
            background,
            attributes,
        }
    }

    /// This style with a different foreground.
    #[must_use]
    pub const fn with_foreground(mut self, foreground: Color) -> Self {
        self.foreground = foreground;
        self
    }

    /// This style with a different background.
    #[must_use]
    pub const fn with_background(mut self, background: Color) -> Self {
        self.background = background;
        self
    }

    /// This style with the given attributes added.
    #[must_use]
    pub const fn with_attributes(mut self, attributes: Attributes) -> Self {
        self.attributes = self.attributes.union(attributes);
        self
    }

    /// This style as the nearest style the given terminal depth can show.
    ///
    /// Only colour degrades. Attributes are left alone: whether a terminal
    /// honours italics is a capability question the driver answers, and
    /// silently dropping an attribute here would make the diff believe the
    /// screen shows something it does not.
    #[must_use]
    pub fn degrade(self, depth: ColorDepth) -> Self {
        Self {
            foreground: self.foreground.degrade(depth),
            background: self.background.degrade(depth),
            attributes: self.attributes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_attributes_contain_nothing_but_the_empty_set() {
        let none = Attributes::NONE;
        assert!(none.is_empty());
        assert!(none.contains(Attributes::NONE));
        assert!(!none.contains(Attributes::BOLD));
    }

    #[test]
    fn attributes_combine_and_separate() {
        let set = Attributes::BOLD | Attributes::ITALIC;
        assert!(set.contains(Attributes::BOLD));
        assert!(set.contains(Attributes::ITALIC));
        assert!(!set.contains(Attributes::UNDERLINE));
        assert!(set.contains(Attributes::BOLD | Attributes::ITALIC));

        let removed = set.difference(Attributes::BOLD);
        assert!(!removed.contains(Attributes::BOLD));
        assert!(removed.contains(Attributes::ITALIC));

        assert_eq!(
            set.intersection(Attributes::ITALIC | Attributes::REVERSE),
            Attributes::ITALIC
        );
    }

    #[test]
    fn every_attribute_has_a_distinct_bit() {
        let mut seen = 0u8;
        for (flag, _) in Attributes::NAMED {
            assert_eq!(seen & flag.bits(), 0, "duplicate bit {:#b}", flag.bits());
            seen |= flag.bits();
        }
        assert_eq!(seen, Attributes::ALL.bits());
    }

    #[test]
    fn unknown_bits_are_discarded() {
        assert_eq!(Attributes::from_bits_truncate(0xFF), Attributes::ALL);
        assert_eq!(
            Attributes::from_bits_truncate(0b1000_0001),
            Attributes::BOLD
        );
    }

    #[test]
    fn attribute_debug_lists_names() {
        assert_eq!(format!("{:?}", Attributes::NONE), "Attributes(NONE)");
        assert_eq!(
            format!("{:?}", Attributes::BOLD | Attributes::UNDERLINE),
            "Attributes(BOLD | UNDERLINE)"
        );
    }

    #[test]
    fn assignment_operator_adds_attributes() {
        let mut set = Attributes::BOLD;
        set |= Attributes::DIM;
        assert_eq!(set, Attributes::BOLD | Attributes::DIM);
    }

    #[test]
    fn style_degradation_touches_only_colour() {
        let style = Style::new(
            Color::rgb(255, 0, 0),
            Color::rgb(0, 0, 0),
            Attributes::BOLD | Attributes::ITALIC,
        );
        let degraded = style.degrade(ColorDepth::Ansi16);
        assert_eq!(degraded.foreground, Color::Indexed(9));
        assert_eq!(degraded.background, Color::Indexed(0));
        assert_eq!(degraded.attributes, style.attributes);
    }

    #[test]
    fn default_style_is_the_terminal_default() {
        assert_eq!(Style::default(), Style::DEFAULT);
        assert_eq!(Style::DEFAULT.foreground, Color::Default);
        assert_eq!(Style::DEFAULT.background, Color::Default);
        assert!(Style::DEFAULT.attributes.is_empty());
    }

    #[test]
    fn builders_replace_colours_and_add_attributes() {
        let style = Style::DEFAULT
            .with_foreground(Color::Indexed(1))
            .with_background(Color::Indexed(2))
            .with_attributes(Attributes::BOLD)
            .with_attributes(Attributes::DIM);
        assert_eq!(style.foreground, Color::Indexed(1));
        assert_eq!(style.background, Color::Indexed(2));
        assert_eq!(style.attributes, Attributes::BOLD | Attributes::DIM);
    }
}
