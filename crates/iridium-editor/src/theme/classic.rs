//! The classic-Mac light preset — Variant A, and where the other two went.
//!
//! `docs/design/LIGHT-THEME-MAP.md` §2.3 designed three light faces. **D-1 is
//! ruled: Variant A wins**, so [`platinum`] is no longer a candidate — it *is*
//! [`Theme::light`](super::Theme::light), reached through
//! [`EditorColors::light`] and [`SyntaxColors::light`], which delegate here
//! rather than carrying a second copy of the table.
//!
//! # Variants B and C are files, not Rust
//!
//! D-1's tail moved "Paper" and "Monochrome" out to `themes/paper.json` and
//! `themes/monochrome.json`, which is exactly what the ruling priced: one file
//! each and nothing in the binary. They are reached the way any theme a user
//! writes is reached — `iridium --theme ./themes/paper.json` — so shipping
//! them is also a standing demonstration that the native format can carry a
//! whole theme, checked by the sweeps below rather than asserted in prose.
//!
//! Their design rationale is not restated here. It never belonged here: the
//! commentary that used to sit beside those tables was itself a paraphrase of
//! §2.3, which remains the one place that says *why* Paper's greys are
//! warm-shifted and why Monochrome keeps a real red for `error`.
//!
//! ⚠️ **One transcription, not two.** The two files were **generated from the
//! tables that used to stand in this module**, never retyped from the map, and
//! the tests below hold the shipped bytes to the map's own hexes. The
//! alternative was tried and abandoned once already: a hand-written float form
//! put `selection` at `#3354AB` against the table's `#3355AA`, and nothing
//! would have caught it, because both copies would have been telling the truth
//! about "the light preset".
//!
//! # What the map states, and what is derived here
//!
//! The map's §2.3 tables state all 24 [`EditorColors`] fields and all 14
//! [`SyntaxColors`] fields for each variant. Four things the tables do not
//! spell out were derived by the map's own stated rules — they are named here
//! because they are as true of the two files as of the preset below, and a
//! reader should never have to guess which is which:
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
//! must not equal `error`, the defect the light preset used to carry. The
//! tables were transcribed verbatim and the narrower rule is pinned; the wider
//! claim in §2.2 is reported as a map defect rather than improvised around.

use super::{Color, EditorColors, SyntaxColors, SyntaxEmphasis, Theme, Typography};

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
        // A crisp near-black frame: 0.55 of the ink, where the dark chrome
        // uses 0.18. The map (§2.6) states the alpha rather than a composited
        // hex, because what a frame must do is hold a panel off the page it
        // floats over — and 0.18 of anything is a modern half-tone rule, not
        // a Platinum frame.
        panel_border: rgba8(0x00, 0x00, 0x00, 0.55),
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
        emphasis: SyntaxEmphasis::none(),
        typography: Typography::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::{platinum, platinum_editor, platinum_syntax};
    use crate::theme::wcag::{channel_distance, contrast};
    use crate::theme::{Color, EditorColors, SyntaxColors, Theme};

    /// The colour a hex string from the map's tables denotes, parsed by the
    /// crate's own parser rather than by this module's constructors — so an
    /// assertion against it checks the transcription, not itself.
    fn stated(hex: &str) -> Color {
        Color::from_hex(hex).expect("the map's tables are six-digit hex")
    }

    /// Where the shipped theme files live, relative to this crate.
    ///
    /// A path rather than an `include_str!`, deliberately: embedding them
    /// would put the bytes back in the binary, which is the cost D-1 ruled
    /// against, and would make these tests pass against a copy rather than
    /// against what a `--theme` path actually opens.
    const THEMES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../themes");

    /// The text of one shipped theme file.
    fn theme_text(slug: &str) -> String {
        let path = format!("{THEMES}/{slug}.json");
        std::fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("`{path}` is a shipped asset and must be readable: {error}")
        })
    }

    /// One shipped theme file, parsed the way `--theme <path>` parses it.
    fn shipped(slug: &str) -> Theme {
        Theme::from_json(&theme_text(slug))
            .unwrap_or_else(|error| panic!("`{slug}.json` must parse as a native theme: {error}"))
    }

    /// Every theme file under `themes/`, by slug, in a stable order.
    ///
    /// ⚠️ The directory is *listed*, not enumerated by name. A third theme
    /// dropped in beside these two joins every sweep below without anyone
    /// remembering to add it — which is the whole point of the rules being
    /// sweeps rather than assertions about two known files. The flip side is
    /// that an empty directory would make every sweep pass while checking
    /// nothing, so the two the ruling named are required back by name.
    fn shipped_slugs() -> Vec<String> {
        let mut slugs: Vec<String> = std::fs::read_dir(THEMES)
            .unwrap_or_else(|error| panic!("`{THEMES}` must be readable: {error}"))
            .map(|entry| entry.expect("a readable directory entry").path())
            .filter(|path| path.extension().is_some_and(|it| it == "json"))
            .filter_map(|path| {
                path.file_stem()
                    .and_then(|it| it.to_str())
                    .map(str::to_owned)
            })
            .collect();
        slugs.sort();

        for required in ["paper", "monochrome"] {
            assert!(
                slugs.iter().any(|slug| slug == required),
                "`themes/{required}.json` is D-1's own deliverable and must be shipped; \
                 `themes/` holds {slugs:?}"
            );
        }
        slugs
    }

    /// Every light face this change is answerable for: the preset that ships
    /// in the binary, plus every file under `themes/`.
    ///
    /// The rules below are the *map's* rules, and the map wrote them about all
    /// three variants — so they are asked of all three still, whichever side
    /// of the binary each now lives on.
    fn light_faces() -> Vec<(String, Theme)> {
        let mut faces = vec![("platinum".to_owned(), platinum())];
        for slug in shipped_slugs() {
            let theme = shipped(&slug);
            faces.push((slug, theme));
        }
        faces
    }

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
    /// each face's editor background is the hex its §2.3 table states.
    ///
    /// The compositor takes `theme.editor.background` as its clear colour
    /// verbatim, so this value *is* the colour the page renders as; a face
    /// that silently drifted here would render a colour nobody chose. For the
    /// two files that means the **shipped bytes** are held to the map — not
    /// the Rust they were generated from, which no longer exists to disagree
    /// with them.
    #[test]
    fn each_face_states_the_background_its_table_states() {
        assert_eq!(platinum().editor.background, stated("#EFEFEF"));
        assert_eq!(shipped("paper").editor.background, stated("#FBF8F1"));
        assert_eq!(shipped("monochrome").editor.background, stated("#FFFFFF"));
    }

    /// The other colours a shot is judged on, likewise pinned to the tables:
    /// the ink, the gutter, the panel/strip surface and the keyword.
    #[test]
    fn each_face_states_the_chrome_its_table_states() {
        let a = platinum();
        assert_eq!(a.editor.foreground, stated("#000000"));
        assert_eq!(a.editor.gutter, stated("#DDDDDD"));
        assert_eq!(a.editor.current_line, stated("#DDDDDD"));
        assert_eq!(a.syntax.keyword, stated("#00007F"));

        let b = shipped("paper");
        assert_eq!(b.editor.foreground, stated("#24211C"));
        assert_eq!(b.editor.gutter, stated("#F5F0E6"));
        assert_eq!(b.editor.current_line, stated("#F2ECE0"));
        assert_eq!(b.syntax.keyword, stated("#26478D"));

        let c = shipped("monochrome");
        assert_eq!(c.editor.foreground, stated("#000000"));
        assert_eq!(c.editor.gutter, stated("#E0E0E0"));
        assert_eq!(c.editor.current_line, stated("#E6E6E6"));
        assert_eq!(c.syntax.keyword, stated("#101070"));
    }

    /// ⭐ Each shipped file states **every** field, in the form the serialiser
    /// produces.
    ///
    /// This is the guard none of the sweeps can be. `EditorColors` and
    /// `SyntaxColors` both carry a container-level `#[serde(default)]`, so a
    /// field missing from one of these files does not fail to parse — it
    /// silently takes the **dark** preset's value. Paper with a cold blue
    /// selection it never asked for, and no error anywhere. The same
    /// defaulting swallows a misspelt key, which serde discards in silence.
    ///
    /// Re-serialising the parsed theme catches both at once: a missing field
    /// comes back, a misspelt one does not come back, and either way the bytes
    /// differ from what is on disk. It also pins the files to canonical
    /// output, so regenerating one is a no-op rather than a diff.
    #[test]
    fn every_shipped_file_states_every_field() {
        for slug in shipped_slugs() {
            let text = theme_text(&slug);
            let round_tripped = format!(
                "{}\n",
                shipped(&slug)
                    .to_json()
                    .expect("a parsed theme re-serialises")
            );
            assert_eq!(
                round_tripped, text,
                "`themes/{slug}.json` is not what the serialiser writes for the theme it \
                 parses as — a field it omits is silently taking the dark preset's value, or \
                 a key it misspells is being discarded"
            );
        }
    }

    #[test]
    fn every_face_is_a_named_light_theme() {
        for (slug, theme) in light_faces() {
            assert!(
                !theme.is_dark,
                "{slug} must report itself light — the compositor keys its \
                 fallback bridge on this flag"
            );
            assert!(
                theme.name.starts_with("Iridium "),
                "{slug} has an off-form name: {}",
                theme.name
            );
        }
    }

    /// The light twin of `the_dark_preset_surfaces_are_opaque`: a transparent
    /// surface presents as black on any face whose swapchain is opaque.
    #[test]
    fn every_face_surface_is_opaque() {
        for (slug, theme) in light_faces() {
            let editor = theme.editor;
            for (field, color) in [
                ("background", editor.background),
                ("gutter", editor.gutter),
                ("minimap_background", editor.minimap_background),
                ("current_line", editor.current_line),
            ] {
                assert!(
                    (color.a - 1.0).abs() < f32::EPSILON,
                    "{slug}'s {field} must be opaque, is alpha {}",
                    color.a
                );
            }
        }
    }

    /// The terminal face falls back to reverse video for its statusline when
    /// the gutter cannot be told from the background — the rule is the
    /// theme's, so it is asserted here too.
    #[test]
    fn every_face_gutter_is_distinct_from_its_background() {
        for (slug, theme) in light_faces() {
            assert_ne!(
                theme.editor.gutter, theme.editor.background,
                "{slug}'s gutter must not equal its background"
            );
        }
    }

    /// `current_line` is also the overlay's panel and strip surface. The light
    /// preset this replaced held its panel apart from the page by 8/255, which
    /// no terminal can show; the map requires at least 17.
    #[test]
    fn every_face_panel_surface_stands_apart_from_the_page() {
        for (slug, theme) in light_faces() {
            let distance = channel_distance(theme.editor.current_line, theme.editor.background);
            assert!(
                distance >= 17.0,
                "{slug}'s panel surface is only {distance:.0}/255 from the page"
            );
        }
    }

    #[test]
    fn every_face_distinguishes_the_current_search_match() {
        for (slug, theme) in light_faces() {
            assert_ne!(
                theme.editor.search_match, theme.editor.search_match_current,
                "{slug}: the editor must be able to say which match Enter leaves"
            );
        }
    }

    /// The defect the old light preset carried (`attribute` and `error` both
    /// `#ff0000`) must not survive into any face: two token classes that
    /// cannot be told apart are not a style.
    #[test]
    fn no_face_paints_attribute_as_error() {
        for (slug, theme) in light_faces() {
            assert_ne!(
                theme.syntax.attribute, theme.syntax.error,
                "{slug}: attribute and error must be distinguishable"
            );
        }
    }

    /// The era rule made testable: body text is ink, not grey text, and the
    /// caret is a black bar that cannot vanish into the page.
    #[test]
    fn every_face_writes_in_ink() {
        for (slug, theme) in light_faces() {
            let editor = theme.editor;
            let text = contrast(editor.foreground, editor.background);
            assert!(
                text >= 12.0,
                "{slug}: body text is {text:.2}:1 against the page, below the 12:1 the map states"
            );
            let caret = contrast(editor.cursor, editor.background);
            assert!(
                caret >= 15.0,
                "{slug}: the caret is {caret:.2}:1 against the page, below the 15:1 the map states"
            );
        }
    }

    /// Every one of the 14 syntax colours must clear WCAG AA against its own
    /// face's page. A light theme whose comments wash out is not usable
    /// whatever its character.
    #[test]
    fn every_syntax_colour_is_legible_on_its_own_surface() {
        for (slug, theme) in light_faces() {
            let background = theme.editor.background;
            for (field, color) in syntax_fields(&theme) {
                let ratio = contrast(color, background);
                assert!(
                    ratio >= 4.5,
                    "{slug}: {field} is {ratio:.2}:1 against the page, below 4.5:1"
                );
            }
        }
    }

    /// The three must actually be three: identical pages would make the choice
    /// between them meaningless, and would mean one of the files was generated
    /// from the wrong table.
    #[test]
    fn the_three_faces_are_genuinely_different() {
        let faces = light_faces();
        for (index, (first_slug, first)) in faces.iter().enumerate() {
            for (second_slug, second) in faces.iter().skip(index + 1) {
                assert_ne!(
                    first.editor, second.editor,
                    "{first_slug} and {second_slug} share an editor palette"
                );
                assert_ne!(
                    first.syntax, second.syntax,
                    "{first_slug} and {second_slug} share a syntax palette"
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

    /// The two that did not win must stay off the built-in path.
    ///
    /// Without this, "Variant A is the light preset" would still pass if Paper
    /// had *also* been wired in somewhere — and the sweeps above, which ask
    /// each face only about itself, would not notice.
    ///
    /// Note what this is now, post-D-1's-tail: not "these are unreachable" but
    /// "these are reachable only as files". `--theme ./themes/paper.json` is
    /// meant to work, and does; what must never happen is `--theme light`
    /// quietly returning one of them.
    #[test]
    fn the_files_are_not_the_built_in_light_preset() {
        let built_in = Theme::light();
        for slug in shipped_slugs() {
            assert_ne!(
                shipped(&slug).editor,
                built_in.editor,
                "themes/{slug}.json is not the ruled preset and must not be reachable as one"
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
