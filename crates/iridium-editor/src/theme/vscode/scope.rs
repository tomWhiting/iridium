//! `TextMate` scopes in, highlight categories out.
//!
//! # ⭐ One table, two answers
//!
//! This used to be a chain of `starts_with` tests writing directly into
//! [`SyntaxColors`](crate::theme::SyntaxColors) fields. That shape can answer
//! "what colour" and cannot answer "what *category*", and a category is what an
//! imported `fontStyle` needs: emphasis is keyed by category, because "headings
//! are bold" must not bold every `fn` (see `theme::emphasis`).
//!
//! Keeping the old field chain for colour and adding a second scope table for
//! emphasis was the obvious move and is the wrong one: two tables mean a theme
//! whose comments are grey and whose comments are italic could disagree about
//! which scopes count as a comment. So there is one table, each row naming the
//! **categories** a scope governs, and colour reaches the right field through
//! `crate::syntax::slot_of` — the same forty-onto-fourteen map the renderer
//! uses.
//!
//! # ⚠️ A row names every category in the family, not one
//!
//! `keyword` maps to `Keyword` **and** `KeywordControl`. Mapping it to one
//! would give a theme saying "keywords are italic" a result where `if` leans
//! and `fn` does not — worse than not importing the style at all, and it would
//! be blamed on the theme. Every row's categories share one slot, which is what
//! keeps the colour half unchanged: see
//! `every_scope_row_names_categories_that_share_one_colour_slot`.
//!
//! # ⛔ Longest prefix wins, and the ordering defect that taught it
//!
//! The old chain tested prefixes **in listed order and returned on the first
//! hit**, so four rows were unreachable: `keyword.operator` sat below
//! `keyword`, `variable.other.property` and `variable.other.constant` below
//! `variable`, and `support.type.property-name` below `support.type`.
//!
//! ⭐ The damage was not that they were ignored. `keyword.operator` matched the
//! `keyword` row, so an imported theme's **operator colour was written into the
//! keyword field** — every keyword in the editor wearing the operator colour,
//! silently, in any theme whose operator rule came after its keyword rule.
//! Measured before the fix.
//!
//! Longest-prefix-wins removes the whole class: a row can no longer be shadowed
//! by a shorter one, so adding a row is safe without reading the table's order.
//! It is also what `TextMate` itself specifies — the most specific matching
//! selector wins — so this is fidelity rather than defensiveness.

use crate::HighlightType;

/// One `TextMate` scope prefix and the categories it governs.
type Row = (&'static str, &'static [HighlightType]);

/// The scope table.
///
/// Order here is presentational only — [`categories_for`] takes the **longest**
/// matching prefix, not the first. Grouped by family for reading.
///
/// ⚠️ **This is deliberately the same coverage the field chain had, plus the
/// four rows that were unreachable.** Broadening it — `constant.language.boolean`
/// reaching `Boolean`, markup scopes reaching the markup categories, the many
/// scopes no row mentions at all — is VS Code import *fidelity* and belongs
/// with #87. Doing it here would mix "the style is finally carried" with "the
/// colours moved", and the second would be blamed on the first.
const ROWS: &[Row] = &[
    // Keywords. `storage.type` and `storage.modifier` are TextMate's spelling
    // of `const`, `static`, `public` and friends.
    (
        "keyword",
        &[HighlightType::Keyword, HighlightType::KeywordControl],
    ),
    (
        "storage.type",
        &[HighlightType::Keyword, HighlightType::KeywordControl],
    ),
    (
        "storage.modifier",
        &[HighlightType::Keyword, HighlightType::KeywordControl],
    ),
    // ⭐ Was unreachable behind `keyword`, and clobbered the keyword colour.
    ("keyword.operator", &[HighlightType::Operator]),
    // Strings. An escape sequence is inside a string and paints from the same
    // slot, so a theme styling strings styles the escapes within them.
    (
        "string",
        &[HighlightType::String, HighlightType::StringEscape],
    ),
    // Numbers.
    ("constant.numeric", &[HighlightType::Number]),
    // Comments — including doc comments, which paint from the same slot.
    (
        "comment",
        &[HighlightType::Comment, HighlightType::CommentDoc],
    ),
    // Functions, in all four spellings Iridium distinguishes.
    ("entity.name.function", FUNCTIONS),
    ("support.function", FUNCTIONS),
    ("meta.function-call", FUNCTIONS),
    // Variables.
    ("variable", VARIABLES),
    // Types. `Lifetime` rides the type slot and so rides this row.
    ("entity.name.type", TYPES),
    ("support.type", TYPES),
    ("entity.name.class", TYPES),
    ("support.class", TYPES),
    // Punctuation, in all three spellings.
    ("punctuation", PUNCTUATION),
    // Properties. ⭐ Two of these three were unreachable — `variable.other.property`
    // behind `variable`, `support.type.property-name` behind `support.type`.
    ("variable.other.property", &[HighlightType::Property]),
    ("entity.name.tag.yaml", &[HighlightType::Property]),
    ("support.type.property-name", &[HighlightType::Property]),
    // Constants. ⭐ `variable.other.constant` was unreachable behind `variable`.
    ("constant.language", &[HighlightType::Constant]),
    ("constant.other", &[HighlightType::Constant]),
    ("variable.other.constant", &[HighlightType::Constant]),
    // Tags.
    ("entity.name.tag", &[HighlightType::Tag]),
    // Attributes.
    ("entity.other.attribute-name", &[HighlightType::Attribute]),
    // Errors.
    ("invalid", &[HighlightType::Error]),
];

/// Every category painted from the function slot.
const FUNCTIONS: &[HighlightType] = &[
    HighlightType::Function,
    HighlightType::FunctionDefinition,
    HighlightType::FunctionMethod,
    HighlightType::FunctionSpecial,
];

/// Every category painted from the variable slot.
///
/// `Embedded` is here because it paints from the variable slot; a theme that
/// colours variables therefore colours embedded regions, which is what it did
/// before this table existed.
const VARIABLES: &[HighlightType] = &[
    HighlightType::Variable,
    HighlightType::VariableParameter,
    HighlightType::VariableSpecial,
    HighlightType::Embedded,
];

/// Every category painted from the type slot.
const TYPES: &[HighlightType] = &[
    HighlightType::Type,
    HighlightType::TypeBuiltin,
    HighlightType::TypeInterface,
    HighlightType::Lifetime,
];

/// Every category painted from the punctuation slot.
const PUNCTUATION: &[HighlightType] = &[
    HighlightType::PunctuationBracket,
    HighlightType::PunctuationDelimiter,
    HighlightType::PunctuationSpecial,
];

/// The categories a `TextMate` scope governs, or none where no row claims it.
///
/// **Longest matching prefix wins** — see the module note for the shipped
/// defect that rule removes. A scope no row claims returns an empty slice,
/// which is the correct handling: VS Code themes name hundreds of scopes and a
/// row is a decision somebody made, not a gap somebody left.
#[must_use]
pub fn categories_for(scope: &str) -> &'static [HighlightType] {
    let scope = scope.to_lowercase();

    ROWS.iter()
        .filter(|(prefix, _)| scope.starts_with(prefix))
        .max_by_key(|(prefix, _)| prefix.len())
        .map_or(&[][..], |(_, categories)| *categories)
}

/// The table itself, for the checks that must hold over every row.
///
/// Exposed to the sibling modules rather than kept private because the properties worth
/// asserting — a row repaints one colour field, no prefix is claimed twice —
/// are properties of *every* row, and a test that walked a hand-copied list
/// would agree with itself while the table drifted.
#[cfg(test)]
pub(super) fn rows() -> impl Iterator<Item = &'static Row> {
    ROWS.iter()
}

#[cfg(test)]
mod tests {
    use super::{ROWS, categories_for};
    use crate::HighlightType;
    use crate::theme::slot_of;

    /// ⭐ The four rows the old chain could never reach, named individually.
    ///
    /// Each was not merely dropped — it matched a **shorter** row and wrote the
    /// imported colour into that row's field. `keyword.operator` is the one
    /// that shows the cost: a theme's operator colour landed on every keyword.
    #[test]
    fn the_four_shadowed_scopes_reach_their_own_categories() {
        for (scope, expected) in [
            ("keyword.operator", HighlightType::Operator),
            ("variable.other.property", HighlightType::Property),
            ("support.type.property-name", HighlightType::Property),
            ("variable.other.constant", HighlightType::Constant),
        ] {
            assert_eq!(
                categories_for(scope),
                &[expected],
                "@{scope} is shadowed by a shorter row again"
            );
        }
    }

    /// The shorter rows still work — the point is specificity, not inversion.
    #[test]
    fn the_shorter_rows_still_claim_what_they_always_did() {
        assert!(categories_for("keyword").contains(&HighlightType::Keyword));
        assert!(categories_for("variable").contains(&HighlightType::Variable));
        assert!(categories_for("support.type").contains(&HighlightType::Type));
    }

    /// ⚠️ A scope is a **prefix**, so the full dotted scopes a real theme
    /// writes must resolve, not just the bare family names.
    #[test]
    fn a_full_dotted_scope_resolves_through_its_prefix() {
        assert!(
            categories_for("keyword.control.flow.rust").contains(&HighlightType::KeywordControl)
        );
        assert_eq!(
            categories_for("keyword.operator.arithmetic.js"),
            &[HighlightType::Operator],
            "a longer operator scope must still beat the bare `keyword` row"
        );
        assert!(categories_for("string.quoted.double.python").contains(&HighlightType::String));
    }

    /// ⭐ **The property that keeps the colour half unchanged.** A row's
    /// categories all paint from one slot, so writing an imported colour to
    /// every category in a row writes to exactly one field — which is what the
    /// old chain did. A row that mixed slots would silently repaint a second
    /// field, and the import would be blamed for a colour nobody chose.
    #[test]
    fn every_scope_row_names_categories_that_share_one_colour_slot() {
        for (scope, categories) in ROWS {
            assert!(
                !categories.is_empty(),
                "the {scope} row claims a scope and governs nothing"
            );
            let first = slot_of(categories[0]);
            for &category in *categories {
                assert_eq!(
                    slot_of(category),
                    first,
                    "the {scope} row mixes colour slots: {category:?} does not \
                     paint from the same field as {:?}",
                    categories[0]
                );
            }
        }
    }

    /// A scope nothing claims governs nothing, rather than falling into the
    /// last row tested — which is how a first-match chain fails open.
    #[test]
    fn an_unclaimed_scope_governs_nothing() {
        for scope in ["", "meta.brace.round", "markup.heading", "source.rust"] {
            assert!(
                categories_for(scope).is_empty(),
                "{scope} should be claimed by no row"
            );
        }
    }

    /// Case is folded — themes write `Keyword` and `String` in the wild.
    #[test]
    fn scope_matching_folds_case() {
        assert_eq!(
            categories_for("KEYWORD.Operator"),
            &[HighlightType::Operator]
        );
    }

    /// No two rows may name the same prefix. Duplicates would make
    /// `max_by_key`'s choice between them arbitrary, and a table where the
    /// answer depends on which equal-length row `max_by_key` happens to keep is
    /// a table nobody can reason about.
    #[test]
    fn no_prefix_is_claimed_twice() {
        let mut prefixes: Vec<&str> = ROWS.iter().map(|(prefix, _)| *prefix).collect();
        prefixes.sort_unstable();
        let total = prefixes.len();
        prefixes.dedup();
        assert_eq!(
            prefixes.len(),
            total,
            "two rows claim the same scope prefix"
        );
    }
}
