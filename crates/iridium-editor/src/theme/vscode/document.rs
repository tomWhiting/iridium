//! The VS Code theme JSON shapes, and the conversion into a [`Theme`].
//!
//! Every field is optional in VS Code itself, so every field here is
//! `#[serde(default)]` — see [`VsCodeTheme::states_any_colour`] for why that
//! faithfulness needs a guard beside it.
//!
//! The scope vocabulary and the `fontStyle` vocabulary live in this module's
//! two siblings; see the parent module note for the division.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::theme::slot_of;
use crate::theme::{Color, EditorColors, SyntaxColors, SyntaxEmphasis, Theme, Typography};

use super::{font_style, scope};

/// A VS Code theme definition.
///
/// This represents the JSON structure of a VS Code color theme file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsCodeTheme {
    /// Theme name
    #[serde(default)]
    pub name: String,

    /// Theme type: "dark", "light", or "hc" (high contrast)
    #[serde(rename = "type", default)]
    pub theme_type: String,

    /// Editor UI colors
    #[serde(default)]
    pub colors: HashMap<String, String>,

    /// Syntax highlighting token colors
    #[serde(rename = "tokenColors", default)]
    pub token_colors: Vec<VsCodeTokenColor>,
}

impl VsCodeTheme {
    /// Whether this document states anything an editor could wear.
    ///
    /// ⭐ **The line that makes the two-parser order work.** Every field above
    /// is `#[serde(default)]`, faithfully, because every field is optional in
    /// VS Code itself — which means *any* JSON object deserializes into this
    /// struct. That is harmless on its own and a defect in context:
    /// `iridium_config::theme` offers a file to the strict native parser first
    /// and this one second, precisely so a native theme with a typo is
    /// reported rather than swallowed. A second parser that accepts everything
    /// makes the first one's complaint unreachable.
    ///
    /// So the narrowest possible test: at least one of `colors` or
    /// `tokenColors`. Both kinds of real theme pass — UI-only themes carry
    /// `colors` and no `tokenColors`, `TextMate` conversions carry the reverse —
    /// while a document with neither cannot change a single pixel, so
    /// accepting it could only ever produce silence. `name` and `type` are
    /// deliberately not enough: they describe a theme without being one.
    #[must_use]
    pub fn states_any_colour(&self) -> bool {
        !self.colors.is_empty() || !self.token_colors.is_empty()
    }
}

/// A syntax highlighting rule in VS Code themes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsCodeTokenColor {
    /// Human-readable name for this rule
    #[serde(default)]
    pub name: Option<String>,

    /// Scopes this rule applies to (can be string or array)
    #[serde(default)]
    pub scope: VsCodeScope,

    /// Style settings for matching tokens
    #[serde(default)]
    pub settings: VsCodeTokenSettings,
}

/// Scope specification for a token color rule.
///
/// In VS Code themes, scope can be either a single string or an array of strings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VsCodeScope {
    /// Single scope string
    Single(String),
    /// Multiple scopes
    Multiple(Vec<String>),
    /// No scope (matches everything)
    #[default]
    None,
}

impl VsCodeScope {
    /// Returns all scopes as a slice of strings.
    fn to_vec(&self) -> Vec<&str> {
        match self {
            Self::Single(s) => vec![s.as_str()],
            Self::Multiple(v) => v.iter().map(String::as_str).collect(),
            Self::None => vec![],
        }
    }
}

/// Style settings for a token color rule.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VsCodeTokenSettings {
    /// Foreground color
    #[serde(default)]
    pub foreground: Option<String>,

    /// Background color (rarely used)
    #[serde(default)]
    pub background: Option<String>,

    /// Font style (italic, bold, underline)
    #[serde(rename = "fontStyle", default)]
    pub font_style: Option<String>,
}

/// Converts a VS Code theme to an Iridium theme.
pub fn convert_vscode_theme(vscode: &VsCodeTheme) -> Theme {
    let is_dark = vscode.theme_type.to_lowercase() != "light";

    // Start with defaults based on theme type
    let mut editor = if is_dark {
        EditorColors::dark()
    } else {
        EditorColors::light()
    };
    let mut syntax = if is_dark {
        SyntaxColors::dark()
    } else {
        SyntaxColors::light()
    };

    // Convert editor colors
    convert_editor_colors(&vscode.colors, &mut editor);

    // ⭐ Colour and emphasis come out of ONE pass over the same scope table,
    // so a theme cannot end up styling a different set of scopes than it
    // colours. See `scope`'s module note for why two tables were refused.
    let mut emphasis = SyntaxEmphasis::none();
    convert_token_colors(&vscode.token_colors, &mut syntax, &mut emphasis);

    Theme {
        name: if vscode.name.is_empty() {
            "Imported VS Code Theme".to_string()
        } else {
            vscode.name.clone()
        },
        is_dark,
        editor,
        emphasis,
        syntax,
        typography: Typography::default(),
    }
}

/// Converts VS Code editor UI colors to Iridium editor colors.
fn convert_editor_colors(colors: &HashMap<String, String>, editor: &mut EditorColors) {
    // Map VS Code color keys to Iridium colors
    if let Some(color) = colors
        .get("editor.background")
        .and_then(|c| Color::from_hex(c))
    {
        editor.background = color;
    }
    if let Some(color) = colors
        .get("editor.foreground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.foreground = color;
    }
    if let Some(color) = colors
        .get("editor.selectionBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.selection = color;
    }
    if let Some(color) = colors
        .get("editor.inactiveSelectionBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.selection_inactive = color;
    }
    if let Some(color) = colors
        .get("editorCursor.foreground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.cursor = color;
    }
    if let Some(color) = colors
        .get("editorLineNumber.foreground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.line_number = color;
    }
    if let Some(color) = colors
        .get("editorLineNumber.activeForeground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.line_number_active = color;
    }
    if let Some(color) = colors
        .get("editor.lineHighlightBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.current_line = color;
    }
    if let Some(color) = colors
        .get("editorGutter.background")
        .and_then(|c| Color::from_hex(c))
    {
        editor.gutter = color;
    }
    if let Some(color) = colors
        .get("minimap.background")
        .and_then(|c| Color::from_hex(c))
    {
        editor.minimap_background = color;
    }
    if let Some(color) = colors
        .get("editor.findMatchBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.search_match = color;
    }
    if let Some(color) = colors
        .get("editor.findMatchHighlightBackground")
        .and_then(|c| Color::from_hex(c))
    {
        editor.search_match_current = color;
    }
}

/// Converts VS Code token colors to Iridium syntax colours and emphasis.
///
/// One pass, both answers. Rules are applied **in document order**, which is VS
/// Code's own rule for equally-specific selectors: a later rule overrides an
/// earlier one.
///
/// ⚠️ **`foreground` and `fontStyle` are handled independently within a rule.**
/// A rule carrying only a colour must not clear an earlier rule's italic, and a
/// rule carrying only a `fontStyle` must not blank a colour. That is what VS
/// Code does, and it is why this is two `if let`s rather than one `else`
/// chain — the old code's `let ... else { continue }` on `foreground` skipped
/// the whole rule, which would have dropped every style-only rule in the file.
fn convert_token_colors(
    token_colors: &[VsCodeTokenColor],
    syntax: &mut SyntaxColors,
    emphasis: &mut SyntaxEmphasis,
) {
    for token in token_colors {
        let color = token
            .settings
            .foreground
            .as_deref()
            .and_then(Color::from_hex);
        let style = token.settings.font_style.as_deref().map(font_style::parse);

        if color.is_none() && style.is_none() {
            continue;
        }

        for scope_name in token.scope.to_vec() {
            for &category in scope::categories_for(scope_name) {
                if let Some(color) = color {
                    *syntax.slot_mut(slot_of(category)) = color;
                }
                if let Some(style) = style {
                    // ⭐ Always written, never skipped when the emphasis is
                    // body text. `"fontStyle": ""` means *explicitly regular*
                    // and exists to cancel a broader rule; treating it as
                    // "nothing to say" would leave that rule's bold in place.
                    // `SyntaxEmphasis::with` replaces, which is exactly the
                    // override semantics wanted.
                    *emphasis = std::mem::take(emphasis).with(category, style);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HighlightType;
    use crate::render::{RunSlant, RunWeight};

    #[test]
    fn convert_minimal_vscode_theme() {
        let vscode = VsCodeTheme {
            name: "Test Theme".to_string(),
            theme_type: "dark".to_string(),
            colors: HashMap::new(),
            token_colors: vec![],
        };

        let theme = convert_vscode_theme(&vscode);
        assert_eq!(theme.name, "Test Theme");
        assert!(theme.is_dark);
    }

    #[test]
    fn convert_light_theme() {
        let vscode = VsCodeTheme {
            name: "Light Test".to_string(),
            theme_type: "light".to_string(),
            colors: HashMap::new(),
            token_colors: vec![],
        };

        let theme = convert_vscode_theme(&vscode);
        assert!(!theme.is_dark);
    }

    #[test]
    fn convert_editor_background() {
        let mut colors = HashMap::new();
        colors.insert("editor.background".to_string(), "#282c34".to_string());

        let vscode = VsCodeTheme {
            name: "With BG".to_string(),
            theme_type: "dark".to_string(),
            colors,
            token_colors: vec![],
        };

        let theme = convert_vscode_theme(&vscode);
        // Verify background was converted (exact values depend on hex parsing)
        assert!(theme.editor.background.r > 0.1);
    }

    #[test]
    fn convert_token_colors_keyword() {
        let token_colors = vec![VsCodeTokenColor {
            name: Some("Keywords".to_string()),
            scope: VsCodeScope::Single("keyword".to_string()),
            settings: VsCodeTokenSettings {
                foreground: Some("#ff0000".to_string()),
                background: None,
                font_style: None,
            },
        }];

        let vscode = VsCodeTheme {
            name: "Tokens Test".to_string(),
            theme_type: "dark".to_string(),
            colors: HashMap::new(),
            token_colors,
        };

        let theme = convert_vscode_theme(&vscode);
        // Red keyword color
        assert!((theme.syntax.keyword.r - 1.0).abs() < 0.01);
        assert!(theme.syntax.keyword.g < 0.01);
        assert!(theme.syntax.keyword.b < 0.01);
    }

    #[test]
    fn convert_multiple_scopes() {
        let token_colors = vec![VsCodeTokenColor {
            name: Some("Strings".to_string()),
            scope: VsCodeScope::Multiple(vec!["string".to_string(), "string.quoted".to_string()]),
            settings: VsCodeTokenSettings {
                foreground: Some("#00ff00".to_string()),
                background: None,
                font_style: None,
            },
        }];

        let vscode = VsCodeTheme {
            name: "Multi Scope".to_string(),
            theme_type: "dark".to_string(),
            colors: HashMap::new(),
            token_colors,
        };

        let theme = convert_vscode_theme(&vscode);
        // Green string color
        assert!(theme.syntax.string.g > 0.9);
    }

    /// ⭐ **Every scope row repaints exactly one colour field.**
    ///
    /// The property that keeps the colour half of this change invisible. A row
    /// names several *categories* — `keyword` names both `Keyword` and
    /// `KeywordControl` so a family leans together — and the import writes the
    /// colour to each of them. If any row's categories spanned two slots, one
    /// imported colour would silently repaint a second field, and the theme
    /// author would be blamed for a colour they never wrote.
    ///
    /// Asserted through the whole conversion rather than against the table, so
    /// it holds against what the import actually does rather than against what
    /// the table looks like.
    #[test]
    fn importing_one_scope_repaints_exactly_one_colour_field() {
        for (scope_name, _) in scope::rows() {
            let vscode = VsCodeTheme {
                name: String::new(),
                theme_type: "dark".to_string(),
                colors: HashMap::new(),
                token_colors: vec![VsCodeTokenColor {
                    name: None,
                    scope: VsCodeScope::Single((*scope_name).to_string()),
                    settings: VsCodeTokenSettings {
                        foreground: Some("#010203".to_string()),
                        background: None,
                        font_style: None,
                    },
                }],
            };
            let imported = convert_vscode_theme(&vscode).syntax;
            let base = SyntaxColors::dark();

            let moved: Vec<&str> = [
                ("keyword", imported.keyword != base.keyword),
                ("string", imported.string != base.string),
                ("number", imported.number != base.number),
                ("comment", imported.comment != base.comment),
                ("function", imported.function != base.function),
                ("variable", imported.variable != base.variable),
                ("type_name", imported.type_name != base.type_name),
                ("operator", imported.operator != base.operator),
                ("punctuation", imported.punctuation != base.punctuation),
                ("property", imported.property != base.property),
                ("constant", imported.constant != base.constant),
                ("tag", imported.tag != base.tag),
                ("attribute", imported.attribute != base.attribute),
                ("error", imported.error != base.error),
            ]
            .into_iter()
            .filter_map(|(name, changed)| changed.then_some(name))
            .collect();

            assert_eq!(
                moved.len(),
                1,
                "the {scope_name} row repainted {moved:?} — a scope must reach \
                 exactly one colour field or an import moves a colour nobody \
                 asked it to"
            );
        }
    }

    /// ⭐ **The assertion that did not exist, and is the whole of #110.**
    ///
    /// A VS Code theme drawing comments in italic and keywords in bold — which
    /// is most of the popular ones — imported as a theme that did neither, and
    /// nothing anywhere said so. `fontStyle` was deserialized from day one and
    /// read by nothing.
    ///
    /// Written against the JSON rather than against hand-built structs, because
    /// the defect was in the path from a file on disk to a `Theme` and a struct
    /// literal skips the half that was broken.
    #[test]
    fn an_imported_theme_wears_the_font_styles_it_asked_for() {
        let json = r##"{
            "name": "Styled",
            "type": "dark",
            "tokenColors": [
                { "scope": "comment", "settings": { "fontStyle": "italic" } },
                { "scope": "keyword", "settings": { "foreground": "#ff0000", "fontStyle": "bold" } }
            ]
        }"##;
        let vscode: VsCodeTheme = serde_json::from_str(json).expect("the fixture parses");
        let theme = convert_vscode_theme(&vscode);

        assert!(
            !theme.emphasis.is_all_body_text(),
            "the theme asked for italic comments and bold keywords and the \
             import produced a theme that says nothing about either"
        );

        let comment = theme
            .emphasis
            .get(HighlightType::Comment)
            .expect("comments were styled");
        assert_eq!(comment.slant, Some(RunSlant::Italic));
        assert_eq!(comment.weight, Some(RunWeight::NORMAL));

        let keyword = theme
            .emphasis
            .get(HighlightType::Keyword)
            .expect("keywords were styled");
        assert_eq!(keyword.weight, Some(RunWeight::BOLD));

        // ⚠️ The colour on the same rule must still arrive. A style-only rule
        // and a style-and-colour rule take different branches, and the second
        // is the one a regression would most likely drop.
        assert!((theme.syntax.keyword.r - 1.0).abs() < 0.01);
    }

    /// ⭐ The trap named in `font_style`: a whole family must lean together.
    ///
    /// `keyword` and `keyword.control` are two categories painted from one
    /// colour field. A scope row naming only `Keyword` would give this theme a
    /// result where `fn` is bold and `if` is not — in the same file, on
    /// adjacent lines — which is worse than not importing the style at all and
    /// would be blamed on the theme.
    #[test]
    fn a_style_on_a_family_reaches_every_category_in_it() {
        let json = r#"{
            "name": "Bold Keywords",
            "type": "dark",
            "tokenColors": [
                { "scope": "keyword", "settings": { "fontStyle": "bold" } }
            ]
        }"#;
        let vscode: VsCodeTheme = serde_json::from_str(json).expect("the fixture parses");
        let theme = convert_vscode_theme(&vscode);

        for category in [HighlightType::Keyword, HighlightType::KeywordControl] {
            assert_eq!(
                theme.emphasis.get(category).and_then(|e| e.weight),
                Some(RunWeight::BOLD),
                "{category:?} did not get the weight the theme put on `keyword`"
            );
        }
    }

    /// ⭐ The empty-string cancel, end to end.
    ///
    /// `keyword` and `storage.type` are two `TextMate` scopes that Iridium's
    /// coarser model puts in **one** category pair, so the second rule really
    /// does overwrite the first. The theme bolds keywords and then writes
    /// `"fontStyle": ""` on storage keywords, which is a pattern real themes
    /// use constantly.
    ///
    /// ⚠️ Reading `""` as "this rule says nothing about style" leaves the bold
    /// in place, which is the failure `font_style`'s module note is about —
    /// observed here through the whole import rather than at the parser.
    ///
    /// ⚠️ **A stated fidelity limit, not a bug.** VS Code would keep `if` bold
    /// and draw `const` regular, because it distinguishes the two scopes.
    /// Iridium has one `Keyword` category for both, so it cannot hold that
    /// difference and the last rule wins. Widening the category set is #87.
    #[test]
    fn a_later_empty_font_style_cancels_an_earlier_one() {
        let json = r#"{
            "name": "Cancelling",
            "type": "dark",
            "tokenColors": [
                { "scope": "keyword", "settings": { "fontStyle": "bold" } },
                { "scope": "storage.type", "settings": { "fontStyle": "" } }
            ]
        }"#;
        let vscode: VsCodeTheme = serde_json::from_str(json).expect("the fixture parses");
        let theme = convert_vscode_theme(&vscode);

        assert_eq!(
            theme
                .emphasis
                .get(HighlightType::Keyword)
                .and_then(|e| e.weight),
            Some(RunWeight::NORMAL),
            "the second rule wrote an empty fontStyle to cancel the bold and \
             the import left the bold in place"
        );
    }

    /// ⚠️ A rule carrying only a `fontStyle` must not be skipped.
    ///
    /// The old loop began `let Some(foreground) = ... else { continue }`, so a
    /// style-only rule never reached the scope map at all. Every theme in the
    /// wild has some.
    #[test]
    fn a_rule_with_no_foreground_is_not_skipped() {
        let json = r#"{
            "name": "Style Only",
            "type": "dark",
            "tokenColors": [
                { "scope": "comment", "settings": { "fontStyle": "italic" } }
            ]
        }"#;
        let vscode: VsCodeTheme = serde_json::from_str(json).expect("the fixture parses");
        let theme = convert_vscode_theme(&vscode);

        assert_eq!(
            theme
                .emphasis
                .get(HighlightType::Comment)
                .and_then(|e| e.slant),
            Some(RunSlant::Italic)
        );
        assert_eq!(
            theme.syntax.comment,
            SyntaxColors::dark().comment,
            "a rule with no foreground must not disturb the colour either"
        );
    }

    /// ⭐ **The shipped colour defect, end to end.**
    ///
    /// `keyword.operator` was shadowed by the `keyword` row in a first-match
    /// chain, so this theme's operator colour was written into the **keyword**
    /// field: every keyword in the editor wearing the operator's colour, in any
    /// theme whose operator rule came after its keyword rule. Measured before
    /// the fix.
    #[test]
    fn an_operator_colour_does_not_overwrite_the_keyword_colour() {
        let json = r##"{
            "name": "Operators",
            "type": "dark",
            "tokenColors": [
                { "scope": "keyword", "settings": { "foreground": "#112233" } },
                { "scope": "keyword.operator", "settings": { "foreground": "#ffcc00" } }
            ]
        }"##;
        let vscode: VsCodeTheme = serde_json::from_str(json).expect("the fixture parses");
        let theme = convert_vscode_theme(&vscode);

        assert!(
            (theme.syntax.keyword.b - 0.2).abs() < 0.02,
            "the keyword colour was overwritten by the operator rule: {:?}",
            theme.syntax.keyword
        );
        assert!(
            (theme.syntax.operator.r - 1.0).abs() < 0.01 && theme.syntax.operator.b < 0.01,
            "the operator colour never arrived: {:?}",
            theme.syntax.operator
        );
    }

    #[test]
    fn parse_vscode_json() {
        let json = r##"{
            "name": "JSON Theme",
            "type": "dark",
            "colors": {
                "editor.background": "#1e1e1e"
            },
            "tokenColors": [
                {
                    "scope": "keyword",
                    "settings": {
                        "foreground": "#569cd6"
                    }
                }
            ]
        }"##;

        let vscode: VsCodeTheme = serde_json::from_str(json).unwrap();
        assert_eq!(vscode.name, "JSON Theme");
        assert_eq!(vscode.token_colors.len(), 1);
    }
}
