//! One scored palette result, and the order results are shown in.

use std::cmp::Ordering;

use crate::commands::{CommandId, CommandMeta};
use crate::fuzzy::MAX_QUERY_CHARS;

/// Which of a command's texts the query matched.
///
/// Reported alongside the match positions because the positions index into
/// *that* text: a hit on an alias cannot be highlighted inside the title, and a
/// palette that pretends otherwise underlines the wrong characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchField {
    /// The human-facing title.
    Title,
    /// One of the command's [`aliases`](CommandMeta::aliases).
    Alias,
    /// The id's final segment — `duplicateUp` of `lines.duplicateUp`.
    Id,
    /// The palette category label.
    Category,
    /// The longer description.
    Description,
}

impl MatchField {
    /// The stable wire name, for a host that renders the match.
    ///
    /// Spelled here rather than in each binding crate so the terminal, the napi
    /// host and the browser all name the same thing the same way.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Alias => "alias",
            Self::Id => "id",
            Self::Category => "category",
            Self::Description => "description",
        }
    }

    /// The share of a raw match score this field keeps, as a percentage.
    ///
    /// A title hit is worth full marks; the further a field is from what the
    /// command is *called*, the less a hit in it says. Integer percentages rather
    /// than floats so every face computes bit-identical scores — "the same query
    /// puts a different command first in the terminal than in the browser" is a
    /// bug a user feels immediately.
    #[must_use]
    pub const fn weight(self) -> i32 {
        match self {
            Self::Title => 100,
            Self::Alias => 90,
            Self::Id => 85,
            Self::Category => 60,
            Self::Description => 50,
        }
    }
}

/// A command that matched a palette query, with its score and match positions.
///
/// Borrows the registry it came from: building one costs nothing, and the caller
/// already holds the registry for as long as it is rendering results.
#[derive(Debug, Clone)]
pub struct PaletteEntry<'a> {
    meta: &'a CommandMeta,
    score: i32,
    field: Option<MatchField>,
    text: &'a str,
    matches: [u32; MAX_QUERY_CHARS],
    match_len: usize,
}

impl<'a> PaletteEntry<'a> {
    /// Builds an entry for a command that matched `field`.
    pub(super) const fn matched(
        meta: &'a CommandMeta,
        score: i32,
        field: MatchField,
        text: &'a str,
        matches: [u32; MAX_QUERY_CHARS],
        match_len: usize,
    ) -> Self {
        Self {
            meta,
            score,
            field: Some(field),
            text,
            matches,
            match_len,
        }
    }

    /// Builds an entry for a command listed without a query to match.
    ///
    /// An empty query lists everything, so every command is an entry with no
    /// matched field and no positions — the score is then whatever recency alone
    /// contributes.
    pub(super) fn listed(meta: &'a CommandMeta, score: i32) -> Self {
        Self {
            meta,
            score,
            field: None,
            text: meta.title(),
            matches: [0; MAX_QUERY_CHARS],
            match_len: 0,
        }
    }

    /// Adds the recency bonus to an already-scored entry.
    pub(super) const fn boosted(mut self, bonus: i32) -> Self {
        self.score += bonus;
        self
    }

    /// The matched command's metadata.
    #[must_use]
    pub const fn meta(&self) -> &'a CommandMeta {
        self.meta
    }

    /// The matched command's id.
    #[must_use]
    pub const fn id(&self) -> &'a CommandId {
        self.meta.id()
    }

    /// The matched command's title.
    #[must_use]
    pub fn title(&self) -> &'a str {
        self.meta.title()
    }

    /// The rank score: higher sorts first. Not meaningful in absolute terms.
    #[must_use]
    pub const fn score(&self) -> i32 {
        self.score
    }

    /// Which text matched, or `None` when the query was empty.
    #[must_use]
    pub const fn matched_field(&self) -> Option<MatchField> {
        self.field
    }

    /// The exact text [`Self::matches`] indexes into.
    ///
    /// The title, one specific alias, the id's last segment, the category or the
    /// description — whichever won. A palette highlighting the match must render
    /// *this* string, not the title.
    #[must_use]
    pub const fn matched_text(&self) -> &'a str {
        self.text
    }

    /// The matched **character** positions within [`Self::matched_text`],
    /// ascending.
    ///
    /// Character positions, not byte offsets: a title containing any non-ASCII
    /// character would otherwise underline the wrong glyphs. A host indexing
    /// UTF-16 (a browser) must convert.
    #[must_use]
    pub fn matches(&self) -> &[u32] {
        &self.matches[..self.match_len]
    }
}

/// Orders two results: best score first, then title, then id.
///
/// A **total** order over distinct commands — ids are unique, so the third key
/// always decides — which is what keeps results from jittering between two
/// equally good matches as the user types.
#[must_use]
pub fn compare_ranked(left: &PaletteEntry<'_>, right: &PaletteEntry<'_>) -> Ordering {
    right
        .score
        .cmp(&left.score)
        .then_with(|| left.title().cmp(right.title()))
        .then_with(|| left.id().cmp(right.id()))
}
