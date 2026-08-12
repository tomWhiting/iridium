//! Color definitions for themes.

use serde::{Deserialize, Serialize};

/// RGBA color with components in 0-1 range.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    /// Red component (0.0-1.0)
    pub r: f32,
    /// Green component (0.0-1.0)
    pub g: f32,
    /// Blue component (0.0-1.0)
    pub b: f32,
    /// Alpha component (0.0-1.0)
    pub a: f32,
}

impl Color {
    /// Creates a new color from RGBA values (0-1 range).
    #[must_use]
    pub const fn new(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a color from RGB values (0-1 range), with full opacity.
    #[must_use]
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Creates a color from a hex string (e.g., "#FF5500" or "FF5500").
    #[must_use]
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 && hex.len() != 8 {
            return None;
        }

        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        let a = if hex.len() == 8 {
            u8::from_str_radix(&hex[6..8], 16).ok()?
        } else {
            255
        };

        Some(Self {
            r: f32::from(r) / 255.0,
            g: f32::from(g) / 255.0,
            b: f32::from(b) / 255.0,
            a: f32::from(a) / 255.0,
        })
    }

    /// Converts to an array for shader uniforms.
    #[must_use]
    pub const fn to_array(self) -> [f32; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

impl Default for Color {
    fn default() -> Self {
        Self::rgb(0.0, 0.0, 0.0)
    }
}

/// Editor chrome colors.
///
/// Deserialization is forgiving: fields absent from a theme JSON fall back to
/// the dark preset via [`Default`], so adding new colors never breaks
/// existing theme files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorColors {
    /// Main editor background
    pub background: Color,
    /// Default text color
    pub foreground: Color,
    /// Selection highlight when focused
    pub selection: Color,
    /// Selection highlight when unfocused
    pub selection_inactive: Color,
    /// Cursor color
    pub cursor: Color,
    /// Line number color
    pub line_number: Color,
    /// Active line number color
    pub line_number_active: Color,
    /// Current line background highlight
    pub current_line: Color,
    /// Gutter background
    pub gutter: Color,
    /// Minimap background
    pub minimap_background: Color,
    /// The hairline frame around a floating panel, and the rule that closes
    /// the tab strip and the search bar.
    ///
    /// ⚠️ **Translucent by design, and composited by the face over whatever
    /// surface the panel sits on.** A border is a colour a theme should
    /// state — the classic-Mac variants genuinely disagree about it, wanting
    /// alphas of 0.55 and 0.85 where the dark preset wants 0.18 — but it is
    /// still ink over a surface, so a preset that stated an opaque colour
    /// here would frame every panel identically regardless of what it floats
    /// above. The desktop face composites; the terminal face has no hairline
    /// at all and ignores this.
    ///
    /// Added as the 25th field after the dark chrome shipped (D-5). Absent
    /// from every theme file written before it, which is why the container's
    /// `#[serde(default)]` matters: they fall back to the dark preset's value
    /// and keep the frame they already had.
    pub panel_border: Color,
    /// Search match highlight
    pub search_match: Color,
    /// Current search match highlight
    pub search_match_current: Color,
    /// Background tint for added lines in diff view
    pub diff_added_bg: Color,
    /// Background tint for deleted lines in diff view
    pub diff_deleted_bg: Color,
    /// Gutter color for added lines in diff view (+)
    pub diff_added_gutter: Color,
    /// Gutter color for deleted lines in diff view (-)
    pub diff_deleted_gutter: Color,
    /// Gutter bar color for added lines (change indicators)
    pub change_added: Color,
    /// Gutter bar color for modified lines (change indicators)
    pub change_modified: Color,
    /// Gutter bar color for deleted lines (change indicators)
    pub change_deleted: Color,
    /// Foreground color for inline blame ghost text
    pub blame_foreground: Color,
    /// Gutter marker color for error diagnostics
    pub diagnostic_error: Color,
    /// Gutter marker color for warning diagnostics
    pub diagnostic_warning: Color,
    /// Gutter marker color for info diagnostics
    pub diagnostic_info: Color,
    /// Gutter marker color for hint diagnostics
    pub diagnostic_hint: Color,
}

impl EditorColors {
    /// Creates dark theme editor colors.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            // Opaque #1a1a1a: the pixels the web demo shows behind the
            // editor, the gutter and the minimap. A transparent surface here
            // presents as black on any face whose surface is opaque.
            background: Color::from_hex("1a1a1a").unwrap_or_default(),
            foreground: Color::from_hex("d4d4d4").unwrap_or_default(),
            selection: Color::new(0.26, 0.42, 0.56, 0.5),
            selection_inactive: Color::new(0.26, 0.42, 0.56, 0.3),
            cursor: Color::from_hex("aeafad").unwrap_or_default(),
            line_number: Color::from_hex("858585").unwrap_or_default(),
            line_number_active: Color::from_hex("c6c6c6").unwrap_or_default(),
            current_line: Color::new(0.15, 0.15, 0.15, 0.3),
            gutter: Color::from_hex("1a1a1a").unwrap_or_default(),
            minimap_background: Color::from_hex("1a1a1a").unwrap_or_default(),
            // Exactly the derivation this field replaces — `foreground` at the
            // desktop face's old `HAIRLINE_ALPHA` of 0.18 — so the shipped
            // dark chrome renders the pixel it rendered before D-5. Written
            // as the parse of `d4d4d4` rather than as a repeated literal, and
            // pinned by `the_dark_panel_border_is_the_derivation_it_replaced`.
            panel_border: Color {
                a: 0.18,
                ..Color::from_hex("d4d4d4").unwrap_or_default()
            },
            search_match: Color::new(0.52, 0.37, 0.13, 0.6),
            search_match_current: Color::new(0.82, 0.67, 0.33, 0.8),
            diff_added_bg: Color::new(0.14, 0.40, 0.22, 0.25),
            diff_deleted_bg: Color::new(0.50, 0.15, 0.15, 0.25),
            diff_added_gutter: Color::new(0.30, 0.70, 0.40, 1.0),
            diff_deleted_gutter: Color::new(0.80, 0.30, 0.30, 1.0),
            change_added: Color::new(0.30, 0.70, 0.40, 1.0),
            change_modified: Color::new(0.80, 0.65, 0.20, 1.0),
            change_deleted: Color::new(0.80, 0.30, 0.30, 1.0),
            blame_foreground: Color::new(0.45, 0.45, 0.50, 0.6),
            diagnostic_error: Color::new(0.80, 0.30, 0.30, 1.0),
            diagnostic_warning: Color::new(0.80, 0.65, 0.20, 1.0),
            diagnostic_info: Color::new(0.30, 0.55, 0.80, 1.0),
            diagnostic_hint: Color::new(0.55, 0.55, 0.60, 1.0),
        }
    }

    /// Creates light theme editor colors — **Platinum**, faithful Mac OS 8.5.
    ///
    /// ⭐ **The document well is grey, not white, and that is the whole
    /// design.** `#EFEFEF` is the Platinum window well and the single
    /// strongest signal of the era; the panels sit a step *below* it at
    /// `#DDDDDD`, inverting the modern instinct to float chrome brighter than
    /// content. Classic dialogs floated over documents in a darker grey, and
    /// following that is what makes the face read as Mac OS rather than as a
    /// pale version of the dark theme.
    ///
    /// ⚠️ **The values live in [`classic::platinum_editor`], not here.** The
    /// map's §2.3 table is transcribed exactly once, beside its two siblings,
    /// where the sweeps that check all three can see it. A second
    /// transcription in this file is what a preset change would silently
    /// diverge from — and it very nearly did: a hand-written float form of the
    /// same table put `selection` at `#3354AB` against the table's `#3355AA`,
    /// a drift no test would have caught because both copies would have been
    /// "the light preset".
    ///
    /// [`classic::platinum_editor`]: super::classic::platinum_editor
    #[must_use]
    pub fn light() -> Self {
        super::classic::platinum_editor()
    }
}

impl Default for EditorColors {
    /// The dark preset. Serves as the per-field fallback for theme JSON that
    /// omits fields (see the container-level `#[serde(default)]`).
    fn default() -> Self {
        Self::dark()
    }
}

/// Syntax highlighting colors.
///
/// Deserialization is forgiving: fields absent from a theme JSON fall back to
/// the dark preset via [`Default`], so adding new token colors never breaks
/// existing theme files.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyntaxColors {
    /// Keywords (if, else, fn, etc.)
    pub keyword: Color,
    /// String literals
    pub string: Color,
    /// Numeric literals
    pub number: Color,
    /// Comments
    pub comment: Color,
    /// Function names
    pub function: Color,
    /// Variable names
    pub variable: Color,
    /// Type names
    pub type_name: Color,
    /// Operators
    pub operator: Color,
    /// Punctuation
    pub punctuation: Color,
    /// Object properties
    pub property: Color,
    /// Constants
    pub constant: Color,
    /// HTML/XML tags
    pub tag: Color,
    /// HTML/XML attributes
    pub attribute: Color,
    /// Syntax errors
    pub error: Color,
}

/// One of [`SyntaxColors`]' fourteen fields, named so it can be passed around.
///
/// # Why this exists
///
/// Roughly forty highlight categories resolve onto fourteen colours, and that
/// map lives in `crate::syntax::slot_of`. Reading it — "what colour is a
/// `KeywordControl`?" — was always possible. **Writing through it was not**,
/// and that is what an imported VS Code theme needs: a `TextMate` scope names a
/// category, and the imported colour has to land in whatever field that
/// category is painted from.
///
/// Without a name for the field, the import would need its own copy of the
/// forty-onto-fourteen map — a second answer to a question that already has
/// one, which would drift the first time a category was added. With it, both
/// directions are the same map read two ways: `slot_of` says which slot, and
/// [`SyntaxColors::slot`] / [`SyntaxColors::slot_mut`] read or write it.
///
/// Not `#[non_exhaustive]`: a field added to `SyntaxColors` should fail to
/// compile here until somebody decides which categories paint from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SyntaxSlot {
    /// [`SyntaxColors::keyword`].
    Keyword,
    /// [`SyntaxColors::string`].
    String,
    /// [`SyntaxColors::number`].
    Number,
    /// [`SyntaxColors::comment`].
    Comment,
    /// [`SyntaxColors::function`].
    Function,
    /// [`SyntaxColors::variable`].
    Variable,
    /// [`SyntaxColors::type_name`].
    TypeName,
    /// [`SyntaxColors::operator`].
    Operator,
    /// [`SyntaxColors::punctuation`].
    Punctuation,
    /// [`SyntaxColors::property`].
    Property,
    /// [`SyntaxColors::constant`].
    Constant,
    /// [`SyntaxColors::tag`].
    Tag,
    /// [`SyntaxColors::attribute`].
    Attribute,
    /// [`SyntaxColors::error`].
    Error,
}

impl SyntaxColors {
    /// The colour in one slot.
    #[must_use]
    pub const fn slot(&self, slot: SyntaxSlot) -> Color {
        match slot {
            SyntaxSlot::Keyword => self.keyword,
            SyntaxSlot::String => self.string,
            SyntaxSlot::Number => self.number,
            SyntaxSlot::Comment => self.comment,
            SyntaxSlot::Function => self.function,
            SyntaxSlot::Variable => self.variable,
            SyntaxSlot::TypeName => self.type_name,
            SyntaxSlot::Operator => self.operator,
            SyntaxSlot::Punctuation => self.punctuation,
            SyntaxSlot::Property => self.property,
            SyntaxSlot::Constant => self.constant,
            SyntaxSlot::Tag => self.tag,
            SyntaxSlot::Attribute => self.attribute,
            SyntaxSlot::Error => self.error,
        }
    }

    /// The colour in one slot, to write through.
    pub const fn slot_mut(&mut self, slot: SyntaxSlot) -> &mut Color {
        match slot {
            SyntaxSlot::Keyword => &mut self.keyword,
            SyntaxSlot::String => &mut self.string,
            SyntaxSlot::Number => &mut self.number,
            SyntaxSlot::Comment => &mut self.comment,
            SyntaxSlot::Function => &mut self.function,
            SyntaxSlot::Variable => &mut self.variable,
            SyntaxSlot::TypeName => &mut self.type_name,
            SyntaxSlot::Operator => &mut self.operator,
            SyntaxSlot::Punctuation => &mut self.punctuation,
            SyntaxSlot::Property => &mut self.property,
            SyntaxSlot::Constant => &mut self.constant,
            SyntaxSlot::Tag => &mut self.tag,
            SyntaxSlot::Attribute => &mut self.attribute,
            SyntaxSlot::Error => &mut self.error,
        }
    }

    /// Creates dark theme syntax colors.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            keyword: Color::from_hex("569cd6").unwrap_or_default(),
            string: Color::from_hex("ce9178").unwrap_or_default(),
            number: Color::from_hex("b5cea8").unwrap_or_default(),
            comment: Color::from_hex("6a9955").unwrap_or_default(),
            function: Color::from_hex("dcdcaa").unwrap_or_default(),
            variable: Color::from_hex("9cdcfe").unwrap_or_default(),
            type_name: Color::from_hex("4ec9b0").unwrap_or_default(),
            operator: Color::from_hex("d4d4d4").unwrap_or_default(),
            punctuation: Color::from_hex("d4d4d4").unwrap_or_default(),
            property: Color::from_hex("9cdcfe").unwrap_or_default(),
            constant: Color::from_hex("4fc1ff").unwrap_or_default(),
            tag: Color::from_hex("569cd6").unwrap_or_default(),
            attribute: Color::from_hex("9cdcfe").unwrap_or_default(),
            error: Color::from_hex("f44747").unwrap_or_default(),
        }
    }

    /// Creates light theme syntax colors — **Platinum**'s MPW/CodeWarrior inks.
    ///
    /// The palette a Mac developer actually saw in 1998, not a lightened
    /// version of the dark preset. Navy keywords, maroon strings and forest
    /// green comments are MPW's; the aubergine on functions and the steel blue
    /// on types are what keep the two apart at a glance, which matters more on
    /// a grey well than on a black one.
    ///
    /// ⚠️ Read against [`EditorColors::light`]'s `#EFEFEF`, not against white.
    /// Every colour here clears 4.5:1 on that grey and several would not clear
    /// it on a darker one, so this preset and that background are a **pair**:
    /// changing one without re-checking the other breaks the promise
    /// `the_light_syntax_palette_is_legible_on_its_own_background` keeps.
    ///
    /// ⚠️ The values live in [`classic::platinum_syntax`], for the reason
    /// given on [`EditorColors::light`].
    ///
    /// [`classic::platinum_syntax`]: super::classic::platinum_syntax
    #[must_use]
    pub fn light() -> Self {
        super::classic::platinum_syntax()
    }
}

impl Default for SyntaxColors {
    /// The dark preset. Serves as the per-field fallback for theme JSON that
    /// omits fields (see the container-level `#[serde(default)]`).
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::wcag::contrast;

    #[test]
    fn color_from_hex() {
        let color = Color::from_hex("#FF5500").unwrap();
        assert!((color.r - 1.0).abs() < 0.01);
        assert!((color.g - 0.333).abs() < 0.01);
        assert!((color.b - 0.0).abs() < 0.01);
    }

    #[test]
    fn color_from_hex_with_alpha() {
        let color = Color::from_hex("FF550080").unwrap();
        assert!((color.a - 0.502).abs() < 0.01);
    }

    #[test]
    fn the_dark_preset_surfaces_are_opaque() {
        // The preset must state the pixels the web demo actually shows: the
        // demo page paints #1a1a1a behind the canvas, so the editor surface,
        // the gutter and the minimap all read as opaque #1a1a1a. An opaque
        // native surface presents a transparent preset as black.
        let colors = EditorColors::dark();
        let expected = Color::from_hex("1a1a1a").unwrap();
        assert!((expected.a - 1.0).abs() < f32::EPSILON);
        assert_eq!(colors.background, expected);
        assert_eq!(colors.gutter, expected);
        assert_eq!(colors.minimap_background, expected);
    }

    #[test]
    fn editor_colors_parse_with_missing_fields() {
        // Theme JSON written before newer fields existed must keep parsing,
        // with absent fields falling back to the dark preset.
        let partial = r#"{
            "background": {"r": 0.1, "g": 0.1, "b": 0.1, "a": 1.0},
            "foreground": {"r": 0.9, "g": 0.9, "b": 0.9, "a": 1.0}
        }"#;
        let colors: EditorColors = serde_json::from_str(partial).unwrap();
        assert!((colors.background.r - 0.1).abs() < f32::EPSILON);
        assert_eq!(
            colors.diagnostic_error,
            EditorColors::dark().diagnostic_error
        );
        assert_eq!(colors.diff_added_bg, EditorColors::dark().diff_added_bg);
    }

    #[test]
    fn a_diagnostic_never_wears_a_token_colour() {
        // `error` marks broken syntax; `attribute` marks a perfectly good
        // token. Painting them the same colour means a malformed node in
        // markup is indistinguishable from a correct one, which is a defect
        // in a preset and not a style choice. The dark preset already keeps
        // them apart; both presets must.
        for (name, colors) in [
            ("dark", SyntaxColors::dark()),
            ("light", SyntaxColors::light()),
        ] {
            assert_ne!(
                colors.attribute, colors.error,
                "the {name} preset paints attributes in the error colour"
            );
        }
    }

    #[test]
    fn separators_are_never_louder_than_the_code_they_separate() {
        // Operators and punctuation carry no meaning of their own, so neither
        // may out-shout body text. The defect this catches is a preset that
        // paints every comma *darker* than the code around it — which the old
        // light preset did, leaving them at pure black while body text sat at
        // #333333.
        //
        // ⚠️ Stated as a contrast ceiling, not as equality with `foreground`.
        // Equality was the original assertion and it was too strong: Platinum
        // deliberately steps `punctuation` back to #3A3A3A under #000000 ink,
        // so structure recedes. That is the rule being kept — quieter is
        // always fine, louder never is — and equality could only express it by
        // forbidding the quieter half too.
        for (name, editor, syntax) in [
            ("dark", EditorColors::dark(), SyntaxColors::dark()),
            ("light", EditorColors::light(), SyntaxColors::light()),
        ] {
            let ink = contrast(editor.foreground, editor.background);
            for (field, color) in [
                ("operators", syntax.operator),
                ("punctuation", syntax.punctuation),
            ] {
                let separator = contrast(color, editor.background);
                assert!(
                    separator <= ink + 0.01,
                    "the {name} preset's {field} are {separator:.2}:1 against the page \
                     where body text is {ink:.2}:1 — a separator louder than the code"
                );
            }
        }
    }

    /// The pair promise on [`SyntaxColors::light`], kept where it is made.
    ///
    /// `classic`'s sweep asks the same question of all three light faces —
    /// this preset plus the two shipped as files; this asks it of the two
    /// public constructors a caller
    /// actually reaches, which is a different claim. They agree today only
    /// because `light()` delegates — and the delegation is precisely the thing
    /// a future edit might replace with a copy.
    ///
    /// 4.5:1 is WCAG AA for body-size text, and every one of the 14 inks must
    /// clear it against `#EFEFEF`. `comment` is the tight one by design, at
    /// roughly 5.4:1: a comment should be readable when looked at and quiet
    /// when not, and there is not much room between those.
    #[test]
    fn the_light_syntax_palette_is_legible_on_its_own_background() {
        let background = EditorColors::light().background;
        let syntax = SyntaxColors::light();

        for (field, color) in [
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
        ] {
            let ratio = contrast(color, background);
            assert!(
                ratio >= 4.5,
                "the light preset's {field} is {ratio:.2}:1 against its own page, below 4.5:1"
            );
        }
    }

    #[test]
    fn syntax_colors_parse_with_missing_fields() {
        let partial = r#"{
            "keyword": {"r": 0.3, "g": 0.6, "b": 0.8, "a": 1.0}
        }"#;
        let colors: SyntaxColors = serde_json::from_str(partial).unwrap();
        assert!((colors.keyword.r - 0.3).abs() < f32::EPSILON);
        assert_eq!(colors.string, SyntaxColors::dark().string);
    }
}
