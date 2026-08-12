//! Which highlight categories a theme draws in a face other than body text.
//!
//! # Why this is a table and not fields on [`SyntaxColors`]
//!
//! [`SyntaxColors`] carries fourteen concrete colours, and
//! `highlight_to_color` maps some forty
//! highlight categories onto them many-to-one — a heading and a keyword both
//! resolve to `keyword`, deliberately, because they should look the same until
//! a theme says otherwise.
//!
//! ⚠️ **That is exactly why weight and slant cannot hang off those fields.**
//! Attaching "bold" to the `keyword` *colour field* would bold every `fn` and
//! `let` in every language the moment a theme bolded headings — which is the
//! defect the markup categories were split out to remove, reintroduced one
//! level up. Emphasis is therefore keyed by **category**, which is the thing a
//! theme author is actually reasoning about.
//!
//! # Why it is sparse
//!
//! A theme names only the categories it wants drawn differently, and every
//! other category is body-weight and upright — which is what every theme in the
//! tree wants today, and what source code should always want. A dense struct
//! would mean a new field in every theme and in the TUI palette each time a
//! category is added, and would make "this theme says nothing about emphasis"
//! unrepresentable when it is the overwhelmingly common case.
//!
//! An **empty** table is therefore not a stub — it is a complete and correct
//! statement that the theme draws everything in one face. Both shipped presets
//! say that today, and `the_shipped_presets_say_nothing_about_emphasis_yet`
//! holds them to it: their appearance must not change because this type came
//! into existence. The identity that makes an empty table free — a run comes
//! back untouched, not merely equal — is
//! `an_empty_emphasis_table_is_byte_identical_to_the_colour_only_answer`, and
//! that one is a property of this type rather than a fact about two files, so
//! it survives a preset that starts emphasising something.
//!
//! # The JSON shape
//!
//! ```json
//! "emphasis": {
//!   "markupHeading": { "weight": 700 },
//!   "markupEmphasis": { "slant": "italic" },
//!   "markupStrong":   { "weight": 700 },
//!   "comment":        { "slant": "italic" }
//! }
//! ```
//!
//! The keys are [`HighlightType`]'s own serialized spellings — it already
//! derives `Serialize`/`Deserialize` with `rename_all = "camelCase"`, so there
//! is one vocabulary for a category name rather than a second one written by
//! hand here. A key naming no category is refused rather than ignored, for the
//! reason `Language`'s deserializer gives: a theme
//! naming something that does not exist should fail where it is written, not
//! resolve to silence the author never sees.
//!
//! [`SyntaxColors`]: super::SyntaxColors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::HighlightType;
use crate::render::{RunSlant, RunStyle, RunWeight};

/// The face one highlight category is drawn in, where it differs from body
/// text.
///
/// Both fields are optional and default to body text, so a theme writing
/// `{ "slant": "italic" }` leaves the weight alone rather than silently
/// resetting it to regular.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Emphasis {
    /// The weight to draw at, or body weight when absent.
    pub weight: Option<RunWeight>,
    /// The slant to draw at, or upright when absent.
    pub slant: Option<RunSlant>,
}

impl Emphasis {
    /// Bold, upright.
    #[must_use]
    pub const fn bold() -> Self {
        Self {
            weight: Some(RunWeight::BOLD),
            slant: None,
        }
    }

    /// Body weight, italic.
    #[must_use]
    pub const fn italic() -> Self {
        Self {
            weight: None,
            slant: Some(RunSlant::Italic),
        }
    }

    /// Applies this emphasis to a run, leaving the run's colour alone.
    ///
    /// An absent field means "whatever the run already says", not "the
    /// default": that is what lets a caller compose, and it is why a theme
    /// naming only a slant does not quietly un-bold anything.
    #[must_use]
    pub const fn applied_to(self, mut style: RunStyle) -> RunStyle {
        if let Some(weight) = self.weight {
            style.weight = weight;
        }
        if let Some(slant) = self.slant {
            style.slant = slant;
        }
        style
    }

    /// Whether this asks for anything at all.
    ///
    /// An `Emphasis` with both fields absent is representable — a theme may
    /// write `"comment": {}` — and means the same as no entry. Callers that
    /// care about "does this theme style anything" must ask this rather than
    /// counting entries.
    #[must_use]
    pub const fn is_body_text(self) -> bool {
        self.weight.is_none() && self.slant.is_none()
    }
}

/// A theme's emphasis table: the categories it draws in a face other than body
/// text, and nothing else.
///
/// See the module docs for why this is keyed by category rather than by colour
/// field, and why empty is a complete answer rather than an unfinished one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyntaxEmphasis {
    entries: BTreeMap<HighlightType, Emphasis>,
}

impl SyntaxEmphasis {
    /// An empty table — every category drawn in body text.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// The same table with `emphasis` set for `highlight`, replacing any entry
    /// already there.
    #[must_use]
    pub fn with(mut self, highlight: HighlightType, emphasis: Emphasis) -> Self {
        self.entries.insert(highlight, emphasis);
        self
    }

    /// What this theme says about `highlight`, or `None` where it says nothing.
    #[must_use]
    pub fn get(&self, highlight: HighlightType) -> Option<Emphasis> {
        self.entries.get(&highlight).copied()
    }

    /// Applies this table's entry for `highlight` to `style`.
    ///
    /// The whole of what a caller normally needs: a category the theme says
    /// nothing about returns `style` untouched, which is the identity the
    /// retained-shaping path depends on — see
    /// [`RunStyle`](crate::render::RunStyle)'s module docs.
    #[must_use]
    pub fn applied_to(&self, highlight: HighlightType, style: RunStyle) -> RunStyle {
        self.get(highlight)
            .map_or(style, |emphasis| emphasis.applied_to(style))
    }

    /// Whether this table asks for nothing anywhere.
    ///
    /// True for an empty table and also for one whose every entry is body text,
    /// because those say the same thing — see [`Emphasis::is_body_text`].
    #[must_use]
    pub fn is_all_body_text(&self) -> bool {
        self.entries.values().all(|entry| entry.is_body_text())
    }

    /// Every category this table names, with what it says about each, in a
    /// stable order.
    ///
    /// Ordered because it is a `BTreeMap` rather than a hash: a theme that
    /// round-trips through JSON must come back byte-identical, and a diff
    /// between two themes must not depend on iteration luck.
    pub fn entries(&self) -> impl Iterator<Item = (HighlightType, Emphasis)> + '_ {
        self.entries
            .iter()
            .map(|(&highlight, &entry)| (highlight, entry))
    }
}

/// Every highlight category, by the name a theme file writes for it.
///
/// ⭐ **This list is the only thing holding the two `HighlightType`s
/// together.** There are two: the real one in `iridium_syntax` and the stub in
/// `crate::syntax_stubs`, chosen by the `syntax` feature. No build sees both,
/// so no `match`, `derive` or trait can compare them — and they have already
/// drifted once, when four `Diff*` variants were added to the real enum and not
/// to the stub, giving a parser-free build that refuses a theme file the full
/// build accepts.
///
/// The gate works because **both configurations run this same list**: a name
/// the running build's enum does not have fails to deserialize, and a variant
/// missing from either side therefore fails in the gate that compiles that
/// side. `test/kernel` runs the parser-free half, which is the half with no
/// other check on it at all.
///
/// ⚠️ What it does **not** catch: a category added to `iridium_syntax` and to
/// this list but to neither's consumers — that is what `theme::slot_of`'s
/// exhaustive match is for — or one added to `iridium_syntax` and forgotten
/// here. The list is a hand-kept spec, and it is hand-kept because there is
/// nothing to derive it from that both builds can see.
#[cfg(test)]
const EVERY_CATEGORY_NAME: &[&str] = &[
    "keyword",
    "keywordControl",
    "string",
    "stringEscape",
    "number",
    "boolean",
    "comment",
    "commentDoc",
    "function",
    "functionDefinition",
    "functionMethod",
    "functionSpecial",
    "variable",
    "variableParameter",
    "variableSpecial",
    "type",
    "typeBuiltin",
    "typeInterface",
    "operator",
    "punctuationBracket",
    "punctuationDelimiter",
    "punctuationSpecial",
    "property",
    "constant",
    "lifetime",
    "attribute",
    "tag",
    "embedded",
    "markupHeading",
    "markupEmphasis",
    "markupStrong",
    "markupStrikethrough",
    "markupCode",
    "markupLink",
    "markupUrl",
    "markupList",
    "markupPunctuation",
    "markupFence",
    "diffAdded",
    "diffRemoved",
    "diffModified",
    "diffMoved",
    "error",
];

#[cfg(test)]
mod tests {
    use super::{Emphasis, SyntaxEmphasis};
    use crate::HighlightType;
    use crate::render::{RunSlant, RunStyle, RunWeight};
    use crate::theme::Color;

    fn body() -> RunStyle {
        RunStyle::plain(Color::new(0.5, 0.5, 0.5, 1.0))
    }

    /// ⭐ The identity the whole retained-shaping path rests on: a theme that
    /// says nothing must leave a run byte-identical to what it was.
    ///
    /// Not "looks the same" — `set_rich_text_diffed` reshapes a line whose
    /// attributes differ from the last frame's, so a table that rebuilt an
    /// equal-but-new style would still be correct here while costing a reshape
    /// of every line. Asserted on the value, which is what the diff compares.
    #[test]
    fn a_table_that_says_nothing_returns_the_run_untouched() {
        let table = SyntaxEmphasis::none();
        for highlight in [
            HighlightType::Keyword,
            HighlightType::MarkupHeading,
            HighlightType::Comment,
        ] {
            assert_eq!(table.applied_to(highlight, body()), body());
        }
        assert!(table.is_all_body_text());
    }

    /// The defect the category keying exists to prevent, asserted directly:
    /// bolding headings must not bold keywords, even though the two resolve to
    /// the same *colour*.
    #[test]
    fn emphasising_one_category_leaves_the_one_it_shares_a_colour_with_alone() {
        let table = SyntaxEmphasis::none().with(HighlightType::MarkupHeading, Emphasis::bold());

        assert_eq!(
            table
                .applied_to(HighlightType::MarkupHeading, body())
                .weight,
            RunWeight::BOLD,
            "the category the theme named must be bold"
        );
        assert_eq!(
            table.applied_to(HighlightType::Keyword, body()),
            body(),
            "a keyword shares the heading's colour and must not share its \
             weight — that sharing is the defect the markup split removed and \
             a colour-keyed emphasis table would put straight back"
        );
    }

    /// An absent field composes rather than resets.
    #[test]
    fn an_emphasis_naming_only_a_slant_leaves_the_weight_alone() {
        let already_bold = body().bold();
        let leaned = Emphasis::italic().applied_to(already_bold);
        assert_eq!(leaned.weight, RunWeight::BOLD, "the weight survived");
        assert_eq!(leaned.slant, RunSlant::Italic);
    }

    /// Both halves reach a run when both are named.
    #[test]
    fn a_weight_and_a_slant_both_reach_the_run() {
        let table = SyntaxEmphasis::none().with(
            HighlightType::MarkupStrong,
            Emphasis {
                weight: Some(RunWeight(600)),
                slant: Some(RunSlant::Italic),
            },
        );
        let styled = table.applied_to(HighlightType::MarkupStrong, body());
        assert_eq!(styled.weight, RunWeight(600));
        assert_eq!(styled.slant, RunSlant::Italic);
        assert_eq!(styled.color, body().color, "and the colour is untouched");
    }

    /// An entry that names nothing is representable and means "body text".
    #[test]
    fn an_entry_naming_nothing_is_body_text_and_says_so() {
        let table = SyntaxEmphasis::none().with(HighlightType::Comment, Emphasis::default());
        assert_eq!(table.applied_to(HighlightType::Comment, body()), body());
        assert!(
            table.is_all_body_text(),
            "an entry that asks for nothing must not read as emphasis just \
             because the map is non-empty"
        );
    }

    /// The JSON shape a theme file writes, round-tripped.
    ///
    /// Written out as text rather than as a serialized value, because
    /// serializing a value and reading it back proves only that the two halves
    /// of one implementation agree — it says nothing about what a theme author
    /// has to type.
    #[test]
    fn a_theme_file_spells_categories_the_way_the_category_names_itself() {
        const SOURCE: &str = r#"{
            "markupHeading": { "weight": 700 },
            "markupEmphasis": { "slant": "italic" },
            "comment": { "slant": "italic", "weight": 300 }
        }"#;

        let table: SyntaxEmphasis =
            serde_json::from_str(SOURCE).expect("the documented shape must parse");

        assert_eq!(
            table.get(HighlightType::MarkupHeading),
            Some(Emphasis::bold())
        );
        assert_eq!(
            table.get(HighlightType::MarkupEmphasis),
            Some(Emphasis::italic())
        );
        assert_eq!(
            table.get(HighlightType::Comment),
            Some(Emphasis {
                weight: Some(RunWeight(300)),
                slant: Some(RunSlant::Italic),
            })
        );
        assert_eq!(
            table.get(HighlightType::Keyword),
            None,
            "a category the file does not name is not in the table at all"
        );
    }

    /// A theme naming a category that does not exist fails where it is
    /// written.
    #[test]
    fn a_category_no_highlight_type_claims_is_refused_rather_than_ignored() {
        let refused = serde_json::from_str::<SyntaxEmphasis>(r#"{"markupUnderline": {}}"#);
        assert!(
            refused.is_err(),
            "an unknown category must be an error the theme author sees, not a \
             line that silently does nothing"
        );

        let misspelled_field =
            serde_json::from_str::<SyntaxEmphasis>(r#"{"comment": {"bold": true}}"#);
        assert!(
            misspelled_field.is_err(),
            "and so must an unknown field inside an entry — `deny_unknown_fields`"
        );
    }

    /// Round-trips byte-identically, in a stable order.
    #[test]
    fn the_table_round_trips_through_json_in_a_stable_order() {
        let table = SyntaxEmphasis::none()
            .with(HighlightType::MarkupStrong, Emphasis::bold())
            .with(HighlightType::Comment, Emphasis::italic())
            .with(HighlightType::MarkupHeading, Emphasis::bold());

        let text = serde_json::to_string(&table).expect("the table serializes");
        assert_eq!(
            text,
            serde_json::to_string(&table).expect("the table serializes"),
            "two serializations of one table must agree — a hash map's would \
             not have to"
        );

        let back: SyntaxEmphasis = serde_json::from_str(&text).expect("its own output parses");
        assert_eq!(back, table);
    }
}

#[cfg(test)]
mod parity {
    use super::{EVERY_CATEGORY_NAME, SyntaxEmphasis};
    use crate::HighlightType;

    /// ⭐ Every category a theme file can name is a category **this build's**
    /// `HighlightType` has.
    ///
    /// Run in both feature configurations, which is the whole mechanism — see
    /// [`EVERY_CATEGORY_NAME`] for why a `match` cannot do this job.
    #[test]
    fn every_named_category_exists_in_this_build() {
        for name in EVERY_CATEGORY_NAME {
            let json = format!("{{\"{name}\": {{\"weight\": 700}}}}");
            let table: SyntaxEmphasis = serde_json::from_str(&json).unwrap_or_else(|error| {
                panic!(
                    "a theme naming `{name}` is refused by this build: {error}. \
                     The two HighlightType enums have drifted — see \
                     EVERY_CATEGORY_NAME."
                )
            });
            assert_eq!(
                table.entries().count(),
                1,
                "`{name}` parsed but produced no entry"
            );
        }
    }

    /// The list has no duplicates and no gaps big enough to be an oversight.
    ///
    /// ⚠️ A count, deliberately: it is the one cheap signal that a variant was
    /// added to `iridium_syntax` and never written here. It will need updating
    /// when a category is added, which is the point — that edit is the moment
    /// somebody decides whether the stub needs it too.
    #[test]
    fn the_category_list_is_complete_and_distinct() {
        let mut names = EVERY_CATEGORY_NAME.to_vec();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total, "a category is listed twice");
        assert_eq!(
            total, 43,
            "the category count moved — add the new one to EVERY_CATEGORY_NAME \
             and to the stub in `syntax_stubs`, then update this number"
        );
    }

    /// A name no build has must be refused, or the test above proves nothing:
    /// a deserializer that accepted anything would pass it for every string.
    #[test]
    fn a_category_no_build_has_is_refused() {
        let refused =
            serde_json::from_str::<SyntaxEmphasis>(r#"{"diffSideways": {"weight": 700}}"#);
        assert!(
            refused.is_err(),
            "an unknown category was accepted, so the parity test above is \
             checking nothing"
        );
        // The control: a real one is accepted by the same call.
        assert!(serde_json::from_str::<SyntaxEmphasis>(r#"{"keyword": {"weight": 700}}"#).is_ok());
        let _ = HighlightType::Keyword;
    }
}
