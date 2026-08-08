//! The classic-Mac light presets — three variants, one of them shipped.
//!
//! These are the three light faces designed in `docs/design/LIGHT-THEME-MAP.md`
//! §2.3, transcribed field by field. **D-1 is ruled: Variant A wins**, so
//! [`platinum`] is no longer a candidate — it *is*
//! [`Theme::light`](super::Theme::light), reached through
//! [`EditorColors::light`] and [`SyntaxColors::light`], which delegate here
//! rather than carrying a second copy of the table. [`paper`] and
//! [`monochrome`] remain unshipped, kept in Rust until they move out to JSON
//! under `themes/`; their consumers are this module's own tests, the desktop
//! face's screenshot harness (`apps/iridium-desktop/tests/chrome_screenshots.rs`)
//! and the overlay's derivation tests.
//!
//! ⚠️ **One transcription, not two.** The tables are written here, beside each
//! other, where the sweeps that check all three can reach them. The
//! alternative — restating Variant A's rows inside
//! [`EditorColors::light`] — was tried and abandoned: the hand-written float
//! form put `selection` at `#3354AB` against the table's `#3355AA`, and
//! nothing would have caught it, because both copies would have been telling
//! the truth about "the light preset".
//!
//! # The three
//!
//! - [`platinum`] — Variant A, faithful Mac OS 8.5: a grey document well,
//!   canonical Platinum chrome a step below it, near-black frames and
//!   MPW/CodeWarrior inks.
//! - [`paper`] — Variant B, System 7 on good stock: warm off-white, minimal
//!   chrome, earthy ink, every grey warm-shifted.
//! - [`monochrome`] — Variant C, System 6: pure white and pure black, dither
//!   greys, one restrained accent, colour kept for the places where it
//!   carries meaning.
//!
//! # What the map states, and what is derived here
//!
//! The map's §2.3 tables state all 24 [`EditorColors`] fields and all 14
//! [`SyntaxColors`] fields for each variant. Four things the tables do not
//! spell out are derived here by the map's own stated rules, and are named so
//! a reader never has to guess which is which:
//!
//! 1. **Alpha where a table row gives a bare hex** is `1.0`. The tables write
//!    an explicit `@ x.xx` wherever a colour is meant to be translucent, so a
//!    bare hex is an opaque colour; §2.2 additionally *requires* opacity of
//!    `background`, `gutter` and `minimap_background`.
//! 2. **`Typography`** — the map (§2.1) records that both GPU faces embed one
//!    font and never read `typography.font_family`, so era typography is not
//!    available to this change and no variant states any. Each takes
//!    [`Typography::default`], exactly as
//!    [`Theme::light`](super::Theme::light) does.
//! 3. **`is_dark`** is `false` for all three. The map does not tabulate it,
//!    but §1.1 makes it load-bearing (it is what the compositor keys its
//!    fallback keyword bridge on), and all three are light faces.
//! 4. **`name`** — the map suggests `"Iridium Platinum"` for the winner (§3);
//!    the other two names follow the same form.
//!
//! # One contradiction inside the map, resolved toward the tables
//!
//! §2.2 asserts that in every variant "no two of the 14 [syntax colours are]
//! equal". The §2.3 tables do not satisfy that, and not by accident: all
//! three deliberately paint `tag` as `string` ("markup tags read as
//! strings-of-structure"), A and B paint `attribute` as `function`, and C
//! paints `attribute` as `type_name` and `variable` as `operator`. What §2.3
//! calls out in bold on every table is the narrower, real rule — `attribute`
//! must not equal `error`, the defect the current
//! [`SyntaxColors::light`](super::SyntaxColors::light) carries. This module
//! transcribes the tables verbatim and pins the narrower rule; the wider
//! claim in §2.2 is reported as a map defect rather than improvised around.

use super::{Color, EditorColors, SyntaxColors, Theme, Typography};

/// An opaque colour from 8-bit sRGB channels.
///
/// The variant tables are written in hex, and hex is how they are reviewed;
/// building colours from the channel bytes keeps the source readable against
/// the table without routing every value through a fallible parse whose
/// failure mode is a silent black.
fn rgb8(r: u8, g: u8, b: u8) -> Color {
    Color::new(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        1.0,
    )
}

/// A colour from 8-bit sRGB channels at a stated alpha — the `#RRGGBB @ a.aa`
/// rows of the variant tables.
fn rgba8(r: u8, g: u8, b: u8, a: f32) -> Color {
    Color::new(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        a,
    )
}

/// Which candidate light preset: the three variants of the light-theme map's
/// §2.3, as a value, so a caller can iterate them without naming each
/// constructor.
///
/// This exists for the renderers and the tests that must cover all three; it
/// is not a runtime selection mechanism and nothing persists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassicVariant {
    /// Variant A — "Platinum", faithful Mac OS 8.5.
    Platinum,
    /// Variant B — "Paper", System 7 on good stock.
    Paper,
    /// Variant C — "Monochrome", System 6.
    Monochrome,
}

impl ClassicVariant {
    /// Every variant, in the map's own order.
    pub const ALL: [Self; 3] = [Self::Platinum, Self::Paper, Self::Monochrome];

    /// A short lowercase identifier, suitable for a file name.
    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Platinum => "platinum",
            Self::Paper => "paper",
            Self::Monochrome => "monochrome",
        }
    }

    /// The variant's theme.
    #[must_use]
    pub fn theme(self) -> Theme {
        match self {
            Self::Platinum => platinum(),
            Self::Paper => paper(),
            Self::Monochrome => monochrome(),
        }
    }
}

// =============================================================================
// Variant A — "Platinum" · faithful Mac OS 8.5
// =============================================================================

/// Variant A's editor chrome — the light-theme map §2.3, table "Variant A ·
/// Editor colours", transcribed row for row.
///
/// ⭐ **This is the shipped light preset**, reached through
/// [`EditorColors::light`], and the only transcription of the table. It is
/// `pub(super)` rather than `pub` so that stays true: a caller outside this
/// module gets the preset by its name, not by its variant.
pub(super) fn platinum_editor() -> EditorColors {
    EditorColors {
        // The Platinum window well — grey, not white, the single strongest
        // era signal.
        background: rgb8(0xEF, 0xEF, 0xEF),
        // Chicago-era body text is pure black ink; 18.3:1.
        foreground: rgb8(0x00, 0x00, 0x00),
        // The Appearance control panel's default blue, translucent so syntax
        // survives under it.
        selection: rgba8(0x33, 0x55, 0xAA, 0.30),
        // The same accent, halved — one accent, two strengths.
        selection_inactive: rgba8(0x33, 0x55, 0xAA, 0.14),
        cursor: rgb8(0x00, 0x00, 0x00),
        // The 50% dither grey, the one grey the 1-bit era actually had.
        line_number: rgb8(0x80, 0x80, 0x80),
        line_number_active: rgb8(0x00, 0x00, 0x00),
        // Canonical Platinum grey — and, by the overlay's derivation, the
        // panel and strip surface, which is exactly right: panels *are*
        // Platinum chrome.
        current_line: rgb8(0xDD, 0xDD, 0xDD),
        gutter: rgb8(0xDD, 0xDD, 0xDD),
        minimap_background: rgb8(0xDD, 0xDD, 0xDD),
        // The Appearance panel's "Gold" accent, the era's canonical
        // alternative, and the same gold driven hard for the current match.
        search_match: rgba8(0xFF, 0xD7, 0x5F, 0.55),
        search_match_current: rgba8(0xFF, 0xA4, 0x00, 0.85),
        // Dark saturated ink, not pastel.
        diff_added_bg: rgba8(0x2E, 0x8B, 0x57, 0.16),
        diff_deleted_bg: rgba8(0xA5, 0x2A, 0x2A, 0.16),
        diff_added_gutter: rgb8(0x1F, 0x7A, 0x45),
        diff_deleted_gutter: rgb8(0xA3, 0x20, 0x20),
        // One green, one dark goldenrod, one red, used consistently.
        change_added: rgb8(0x1F, 0x7A, 0x45),
        change_modified: rgb8(0xB8, 0x86, 0x0B),
        change_deleted: rgb8(0xA3, 0x20, 0x20),
        // Ghost text as the dither grey, quieted.
        blame_foreground: rgba8(0x80, 0x80, 0x80, 0.75),
        diagnostic_error: rgb8(0xA3, 0x20, 0x20),
        diagnostic_warning: rgb8(0xB8, 0x86, 0x0B),
        diagnostic_info: rgb8(0x2A, 0x4E, 0x8C),
        // A quieter grey than `line_number`, so hints recede.
        diagnostic_hint: rgb8(0x6B, 0x6B, 0x6B),
    }
}

/// Variant A's code inks — MPW/CodeWarrior colours, the map §2.3's "Variant A
/// · Syntax colours".
///
/// The shipped light preset's inks, reached through [`SyntaxColors::light`];
/// `pub(super)` for the reason given on [`platinum_editor`].
pub(super) fn platinum_syntax() -> SyntaxColors {
    SyntaxColors {
        // CodeWarrior's navy.
        keyword: rgb8(0x00, 0x00, 0x7F),
        // Maroon, the era's string colour.
        string: rgb8(0x7F, 0x00, 0x00),
        // Dark teal, distinct from both green and blue.
        number: rgb8(0x00, 0x5F, 0x5F),
        // MPW's forest green — present but recessive.
        comment: rgb8(0x00, 0x70, 0x00),
        // Aubergine, the one violet.
        function: rgb8(0x5C, 0x3D, 0x99),
        // Ink, barely off `foreground` — variables are the page.
        variable: rgb8(0x1A, 0x1A, 0x1A),
        // Steel blue, distinguishable from `keyword` at a glance.
        type_name: rgb8(0x00, 0x5F, 0x87),
        operator: rgb8(0x00, 0x00, 0x00),
        // One step back from ink, so structure recedes.
        punctuation: rgb8(0x3A, 0x3A, 0x3A),
        property: rgb8(0x2A, 0x4E, 0x8C),
        // Burnt umber.
        constant: rgb8(0x7A, 0x3E, 0x00),
        // Markup tags read as strings-of-structure.
        tag: rgb8(0x7F, 0x00, 0x00),
        // Distinct from `error` — the current light preset's defect, not
        // repeated.
        attribute: rgb8(0x5C, 0x3D, 0x99),
        // The only pure-ish red in the syntax set.
        error: rgb8(0xB0, 0x00, 0x00),
    }
}

/// Variant A — "Platinum": the editor as it might have shipped in 1998.
///
/// ⭐ **The shipped light preset.** D-1 ruled Variant A, so this is exactly
/// what [`Theme::light`](super::Theme::light) returns — same name, same
/// fields, asserted below. It stays a named function because the screenshot
/// harness and the overlay's derivation tests iterate the three variants
/// together, and Variant A must be in that sweep on the same terms as the
/// other two.
#[must_use]
pub fn platinum() -> Theme {
    Theme {
        name: "Iridium Platinum".to_string(),
        is_dark: false,
        editor: platinum_editor(),
        syntax: platinum_syntax(),
        typography: Typography::default(),
    }
}

// =============================================================================
// Variant B — "Paper" · System 7 on good stock
// =============================================================================

/// Variant B's editor chrome — the light-theme map §2.3, table "Variant B ·
/// Editor colours", transcribed row for row.
fn paper_editor() -> EditorColors {
    EditorColors {
        // Warm paper — off-white, never a light source.
        background: rgb8(0xFB, 0xF8, 0xF1),
        // Warm near-black; ink, not grey.
        foreground: rgb8(0x24, 0x21, 0x1C),
        // A muted period blue, warm-compatible, and the same halved.
        selection: rgba8(0x4A, 0x6F, 0xA5, 0.26),
        selection_inactive: rgba8(0x4A, 0x6F, 0xA5, 0.12),
        cursor: rgb8(0x24, 0x21, 0x1C),
        // Warm grey, clearly subordinate.
        line_number: rgb8(0xA6, 0x9E, 0x90),
        line_number_active: rgb8(0x24, 0x21, 0x1C),
        // A warm card one step off the paper — the panel and strip surface.
        current_line: rgb8(0xF2, 0xEC, 0xE0),
        // Between paper and card; distinct from `background`.
        gutter: rgb8(0xF5, 0xF0, 0xE6),
        minimap_background: rgb8(0xF5, 0xF0, 0xE6),
        // Aged-gold highlighter, and the same gold driven.
        search_match: rgba8(0xE8, 0xC4, 0x6A, 0.55),
        search_match_current: rgba8(0xD9, 0x8E, 0x28, 0.85),
        // Soft on paper.
        diff_added_bg: rgba8(0x4E, 0x8B, 0x57, 0.14),
        diff_deleted_bg: rgba8(0xA8, 0x5A, 0x5A, 0.14),
        diff_added_gutter: rgb8(0x3E, 0x7A, 0x4B),
        diff_deleted_gutter: rgb8(0x9E, 0x40, 0x40),
        change_added: rgb8(0x3E, 0x7A, 0x4B),
        change_modified: rgb8(0xA8, 0x82, 0x32),
        change_deleted: rgb8(0x9E, 0x40, 0x40),
        // The gutter grey as ghost text.
        blame_foreground: rgba8(0xA6, 0x9E, 0x90, 0.85),
        diagnostic_error: rgb8(0x9E, 0x30, 0x30),
        diagnostic_warning: rgb8(0xA8, 0x82, 0x32),
        diagnostic_info: rgb8(0x3E, 0x60, 0x99),
        // Warm grey, recessive.
        diagnostic_hint: rgb8(0x8A, 0x83, 0x75),
    }
}

/// Variant B's code inks — earthy colours, the map §2.3's "Variant B · Syntax
/// colours".
fn paper_syntax() -> SyntaxColors {
    SyntaxColors {
        // Warm-shifted navy.
        keyword: rgb8(0x26, 0x47, 0x8D),
        // Burnt sienna.
        string: rgb8(0x8A, 0x33, 0x24),
        // Deep viridian.
        number: rgb8(0x1D, 0x6A, 0x5A),
        // Olive-green — readable, recessive.
        comment: rgb8(0x4A, 0x7A, 0x3D),
        // Tobacco brown.
        function: rgb8(0x7A, 0x4E, 0x1E),
        // Warm ink.
        variable: rgb8(0x33, 0x30, 0x2A),
        // Slate teal.
        type_name: rgb8(0x2A, 0x60, 0x70),
        // Warm grey-black.
        operator: rgb8(0x4A, 0x45, 0x3C),
        // One step further back — structure recedes.
        punctuation: rgb8(0x6B, 0x64, 0x59),
        // Dusty blue.
        property: rgb8(0x3A, 0x5A, 0x7A),
        // Amber-brown.
        constant: rgb8(0x8A, 0x4B, 0x10),
        // As strings.
        tag: rgb8(0x8A, 0x33, 0x24),
        // Distinct from `error`.
        attribute: rgb8(0x7A, 0x4E, 0x1E),
        // The single red.
        error: rgb8(0x9E, 0x30, 0x30),
    }
}

/// Variant B — "Paper": the one designed to be lived in eight hours a day.
///
/// A candidate preset pending the owner's D-1 ruling; not reachable from any
/// runtime path and not a replacement for
/// [`Theme::light`](super::Theme::light).
#[must_use]
pub fn paper() -> Theme {
    Theme {
        name: "Iridium Paper".to_string(),
        is_dark: false,
        editor: paper_editor(),
        syntax: paper_syntax(),
        typography: Typography::default(),
    }
}

// =============================================================================
// Variant C — "Monochrome" · System 6, in colour only where it must be
// =============================================================================

/// Variant C's editor chrome — the light-theme map §2.3, table "Variant C ·
/// Editor colours", transcribed row for row.
///
/// The `diff_*`, `change_*` and `diagnostic_*` families keep full functional
/// hue on purpose: §2.2's last rule holds that a diff whose additions and
/// deletions differ only in grey is a defect, not a style.
fn monochrome_editor() -> EditorColors {
    EditorColors {
        // The 1-bit page and the 1-bit ink.
        background: rgb8(0xFF, 0xFF, 0xFF),
        foreground: rgb8(0x00, 0x00, 0x00),
        // The one accent — System 7's pale highlight blue — and the same
        // halved.
        selection: rgba8(0x7A, 0x93, 0xB8, 0.36),
        selection_inactive: rgba8(0x7A, 0x93, 0xB8, 0.16),
        cursor: rgb8(0x00, 0x00, 0x00),
        // The 50% dither.
        line_number: rgb8(0x80, 0x80, 0x80),
        line_number_active: rgb8(0x00, 0x00, 0x00),
        // The 90% dither — panel and strip surface, far enough from the page
        // to survive a terminal, where there is no hairline to help.
        current_line: rgb8(0xE6, 0xE6, 0xE6),
        // One dither step below the panel; distinct from `background`.
        gutter: rgb8(0xE0, 0xE0, 0xE0),
        minimap_background: rgb8(0xE0, 0xE0, 0xE0),
        // A 25% dither wash, against a current match dark enough that black
        // text on it still reads.
        search_match: rgba8(0x80, 0x80, 0x80, 0.30),
        search_match_current: rgba8(0x30, 0x30, 0x30, 0.45),
        diff_added_bg: rgba8(0x2E, 0x7D, 0x32, 0.13),
        diff_deleted_bg: rgba8(0xB7, 0x1C, 0x1C, 0.13),
        diff_added_gutter: rgb8(0x1B, 0x5E, 0x20),
        diff_deleted_gutter: rgb8(0xB7, 0x1C, 0x1C),
        change_added: rgb8(0x1B, 0x5E, 0x20),
        change_modified: rgb8(0x8D, 0x6E, 0x00),
        change_deleted: rgb8(0xB7, 0x1C, 0x1C),
        blame_foreground: rgba8(0x80, 0x80, 0x80, 0.85),
        // Meaning outranks style.
        diagnostic_error: rgb8(0xB7, 0x1C, 0x1C),
        diagnostic_warning: rgb8(0x8D, 0x6E, 0x00),
        diagnostic_info: rgb8(0x1F, 0x3A, 0x6E),
        diagnostic_hint: rgb8(0x60, 0x60, 0x60),
    }
}

/// Variant C's code inks — near-monochrome, hue as a whisper, the map §2.3's
/// "Variant C · Syntax colours".
fn monochrome_syntax() -> SyntaxColors {
    SyntaxColors {
        // Near-black navy — reads as *weight*, not colour.
        keyword: rgb8(0x10, 0x10, 0x70),
        // Near-black maroon.
        string: rgb8(0x6A, 0x1B, 0x1B),
        // Near-black teal.
        number: rgb8(0x1B, 0x4D, 0x4D),
        // The era's comment: grey, not green.
        comment: rgb8(0x5A, 0x5A, 0x5A),
        // Ink, one step lifted.
        function: rgb8(0x30, 0x30, 0x30),
        variable: rgb8(0x00, 0x00, 0x00),
        // Near-black steel.
        type_name: rgb8(0x1B, 0x3A, 0x5A),
        operator: rgb8(0x00, 0x00, 0x00),
        // Recedes.
        punctuation: rgb8(0x4A, 0x4A, 0x4A),
        property: rgb8(0x20, 0x20, 0x20),
        // Near-black umber.
        constant: rgb8(0x4A, 0x2A, 0x00),
        // As strings.
        tag: rgb8(0x6A, 0x1B, 0x1B),
        // Distinct from `error`.
        attribute: rgb8(0x1B, 0x3A, 0x5A),
        // The one place C permits a real red — an error must shout.
        error: rgb8(0xB7, 0x1C, 0x1C),
    }
}

/// Variant C — "Monochrome": the 1-bit machine, with colour kept for meaning.
///
/// A candidate preset pending the owner's D-1 ruling; not reachable from any
/// runtime path and not a replacement for
/// [`Theme::light`](super::Theme::light).
#[must_use]
pub fn monochrome() -> Theme {
    Theme {
        name: "Iridium Monochrome".to_string(),
        is_dark: false,
        editor: monochrome_editor(),
        syntax: monochrome_syntax(),
        typography: Typography::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::{ClassicVariant, monochrome, paper, platinum, platinum_editor, platinum_syntax};
    use crate::theme::{Color, EditorColors, SyntaxColors, Theme};

    /// The colour a hex string from the map's tables denotes, parsed by the
    /// crate's own parser rather than by this module's constructors — so an
    /// assertion against it checks the transcription, not itself.
    fn stated(hex: &str) -> Color {
        Color::from_hex(hex).expect("the map's tables are six-digit hex")
    }

    use crate::theme::wcag::{channel_distance, contrast};

    /// Every variant's 14 syntax colours, named, for the sweeps below.
    fn syntax_fields(theme: &Theme) -> [(&'static str, Color); 14] {
        let syntax = &theme.syntax;
        [
            ("keyword", syntax.keyword),
            ("string", syntax.string),
            ("number", syntax.number),
            ("comment", syntax.comment),
            ("function", syntax.function),
            ("variable", syntax.variable),
            ("type_name", syntax.type_name),
            ("operator", syntax.operator),
            ("punctuation", syntax.punctuation),
            ("property", syntax.property),
            ("constant", syntax.constant),
            ("tag", syntax.tag),
            ("attribute", syntax.attribute),
            ("error", syntax.error),
        ]
    }

    /// The pixel-honest check the map asks for, at the cheapest honest level:
    /// each variant's editor background is the hex its §2.3 table states.
    ///
    /// The compositor takes `theme.editor.background` as its clear colour
    /// verbatim, so this value *is* the colour the page renders as; a variant
    /// that silently drifted here would render a colour nobody chose.
    #[test]
    fn each_variant_states_the_background_its_table_states() {
        assert_eq!(platinum().editor.background, stated("#EFEFEF"));
        assert_eq!(paper().editor.background, stated("#FBF8F1"));
        assert_eq!(monochrome().editor.background, stated("#FFFFFF"));
    }

    /// The other colours a shot is judged on, likewise pinned to the tables:
    /// the ink, the gutter, the panel/strip surface and the keyword.
    #[test]
    fn each_variant_states_the_chrome_its_table_states() {
        let a = platinum();
        assert_eq!(a.editor.foreground, stated("#000000"));
        assert_eq!(a.editor.gutter, stated("#DDDDDD"));
        assert_eq!(a.editor.current_line, stated("#DDDDDD"));
        assert_eq!(a.syntax.keyword, stated("#00007F"));

        let b = paper();
        assert_eq!(b.editor.foreground, stated("#24211C"));
        assert_eq!(b.editor.gutter, stated("#F5F0E6"));
        assert_eq!(b.editor.current_line, stated("#F2ECE0"));
        assert_eq!(b.syntax.keyword, stated("#26478D"));

        let c = monochrome();
        assert_eq!(c.editor.foreground, stated("#000000"));
        assert_eq!(c.editor.gutter, stated("#E0E0E0"));
        assert_eq!(c.editor.current_line, stated("#E6E6E6"));
        assert_eq!(c.syntax.keyword, stated("#101070"));
    }

    #[test]
    fn every_variant_is_a_named_light_theme() {
        for variant in ClassicVariant::ALL {
            let theme = variant.theme();
            assert!(
                !theme.is_dark,
                "{} must report itself light — the compositor keys its \
                 fallback bridge on this flag",
                variant.slug()
            );
            assert!(
                theme.name.starts_with("Iridium "),
                "{} has an off-form name: {}",
                variant.slug(),
                theme.name
            );
        }
    }

    /// The light twin of `the_dark_preset_surfaces_are_opaque`: a transparent
    /// surface presents as black on any face whose swapchain is opaque.
    #[test]
    fn every_variant_surface_is_opaque() {
        for variant in ClassicVariant::ALL {
            let editor = variant.theme().editor;
            for (field, color) in [
                ("background", editor.background),
                ("gutter", editor.gutter),
                ("minimap_background", editor.minimap_background),
                ("current_line", editor.current_line),
            ] {
                assert!(
                    (color.a - 1.0).abs() < f32::EPSILON,
                    "{}'s {field} must be opaque, is alpha {}",
                    variant.slug(),
                    color.a
                );
            }
        }
    }

    /// The terminal face falls back to reverse video for its statusline when
    /// the gutter cannot be told from the background — the rule is the
    /// theme's, so it is asserted here too.
    #[test]
    fn every_variant_gutter_is_distinct_from_its_background() {
        for variant in ClassicVariant::ALL {
            let editor = variant.theme().editor;
            assert_ne!(
                editor.gutter,
                editor.background,
                "{}'s gutter must not equal its background",
                variant.slug()
            );
        }
    }

    /// `current_line` is also the overlay's panel and strip surface. The
    /// current light preset holds its panel apart from the page by 8/255,
    /// which no terminal can show; the map requires at least 17.
    #[test]
    fn every_variant_panel_surface_stands_apart_from_the_page() {
        for variant in ClassicVariant::ALL {
            let editor = variant.theme().editor;
            let distance = channel_distance(editor.current_line, editor.background);
            assert!(
                distance >= 17.0,
                "{}'s panel surface is only {distance:.0}/255 from the page",
                variant.slug()
            );
        }
    }

    #[test]
    fn every_variant_distinguishes_the_current_search_match() {
        for variant in ClassicVariant::ALL {
            let editor = variant.theme().editor;
            assert_ne!(
                editor.search_match,
                editor.search_match_current,
                "{}: the editor must be able to say which match Enter leaves",
                variant.slug()
            );
        }
    }

    /// The defect the current light preset carries (`attribute` and `error`
    /// both `#ff0000`) must not survive into any variant: two token classes
    /// that cannot be told apart are not a style.
    #[test]
    fn no_variant_paints_attribute_as_error() {
        for variant in ClassicVariant::ALL {
            let syntax = variant.theme().syntax;
            assert_ne!(
                syntax.attribute,
                syntax.error,
                "{}: attribute and error must be distinguishable",
                variant.slug()
            );
        }
    }

    /// The era rule made testable: body text is ink, not grey text, and the
    /// caret is a black bar that cannot vanish into the page.
    #[test]
    fn every_variant_writes_in_ink() {
        for variant in ClassicVariant::ALL {
            let editor = variant.theme().editor;
            let text = contrast(editor.foreground, editor.background);
            assert!(
                text >= 12.0,
                "{}: body text is {text:.2}:1 against the page, below the 12:1 the map states",
                variant.slug()
            );
            let caret = contrast(editor.cursor, editor.background);
            assert!(
                caret >= 15.0,
                "{}: the caret is {caret:.2}:1 against the page, below the 15:1 the map states",
                variant.slug()
            );
        }
    }

    /// Every one of the 14 syntax colours must clear WCAG AA against its own
    /// variant's page. A light theme whose comments wash out is not usable
    /// whatever its character.
    #[test]
    fn every_syntax_colour_is_legible_on_its_own_surface() {
        for variant in ClassicVariant::ALL {
            let theme = variant.theme();
            let background = theme.editor.background;
            for (field, color) in syntax_fields(&theme) {
                let ratio = contrast(color, background);
                assert!(
                    ratio >= 4.5,
                    "{}: {field} is {ratio:.2}:1 against the page, below 4.5:1",
                    variant.slug()
                );
            }
        }
    }

    /// The three variants must actually be three: identical pages would make
    /// the rendered choice meaningless.
    #[test]
    fn the_three_variants_are_genuinely_different() {
        let themes = [platinum(), paper(), monochrome()];
        for (index, first) in themes.iter().enumerate() {
            for second in themes.iter().skip(index + 1) {
                assert_ne!(
                    first.editor, second.editor,
                    "{} and {} share an editor palette",
                    first.name, second.name
                );
                assert_ne!(
                    first.syntax, second.syntax,
                    "{} and {} share a syntax palette",
                    first.name, second.name
                );
            }
        }
    }

    /// D-1's ruling, made mechanical: Variant A **is** the shipped light
    /// preset, whole — name, chrome, inks and `is_dark` alike.
    ///
    /// ⭐ This replaces `no_variant_has_replaced_the_shipped_light_preset`,
    /// which asserted the exact opposite and was written to hold only until
    /// D-1 was ruled. Inverting it rather than deleting it is the point: the
    /// same line that used to guard "nothing has shipped yet" now guards
    /// "*this* is what shipped", so the transition cannot happen silently in
    /// either direction.
    ///
    /// Whole-`Theme` equality rather than a field sweep, because the failure
    /// this is really aimed at is partial adoption — Platinum's colours worn
    /// under the old name, or under `is_dark: true`, which is what the
    /// compositor keys its fallback keyword bridge on.
    #[test]
    fn platinum_is_the_shipped_light_preset() {
        let shipped = Theme::light();
        assert_eq!(shipped.name, "Iridium Platinum");
        assert!(!shipped.is_dark);
        assert_eq!(shipped, platinum(), "Theme::light must be Variant A whole");

        // And the delegation is what makes that true: the two public halves
        // must come from the same transcription, not from a copy of it that
        // happens to agree today.
        assert_eq!(EditorColors::light(), platinum_editor());
        assert_eq!(SyntaxColors::light(), platinum_syntax());
    }

    /// The two that did not win must stay off the shipped path.
    ///
    /// Without this, "Variant A is the light preset" would still pass if
    /// Paper had *also* been wired in somewhere — and the sweeps above, which
    /// ask each variant only about itself, would not notice.
    #[test]
    fn the_unshipped_variants_are_unshipped() {
        let shipped = Theme::light();
        for variant in [ClassicVariant::Paper, ClassicVariant::Monochrome] {
            assert_ne!(
                variant.theme().editor,
                shipped.editor,
                "{} is not the ruled preset and must not be reachable as one",
                variant.slug()
            );
        }
    }

    /// The channel constructors must agree with the crate's hex parser on
    /// every byte, or every table assertion above would be checking the
    /// wrong thing.
    #[test]
    fn the_channel_constructors_agree_with_the_hex_parser() {
        for value in 0..=u8::MAX {
            let text = format!("{value:02X}{value:02X}{value:02X}");
            assert_eq!(super::rgb8(value, value, value), stated(&text));
        }
        // The translucent form must differ from the opaque one in alpha and
        // in nothing else.
        let translucent = super::rgba8(0x12, 0x34, 0x56, 0.5);
        let opaque = stated("#123456");
        assert_eq!(
            super::rgba8(0x12, 0x34, 0x56, 1.0),
            opaque,
            "the channels must match the parser exactly"
        );
        assert!((translucent.a - 0.5).abs() < f32::EPSILON);
        assert!(
            channel_distance(translucent, opaque) < 1.0,
            "alpha is the only thing rgba8 changes"
        );
    }
}
