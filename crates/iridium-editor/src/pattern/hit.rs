//! Running a pattern against one path.

use regex::Regex;

use super::parse::Pattern;
use crate::fuzzy::{PathField, basename_start, match_path};

/// The score a regular-expression match starts from, before the field's
/// weight is applied.
///
/// A regular expression does not rank: it matches or it does not, and every
/// hit is as good as every other. The number exists so that a name hit still
/// outranks a hit that only landed in the folders above it — which is what
/// puts the selection on `engine/engine.rs` rather than on the first file
/// inside `engine/` — and it reuses [`PathField`]'s weights so that the two
/// halves of the query field prefer the same thing.
const REGEX_BASE: i32 = 100;

/// What a pattern found in one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// How good the match is, higher being better. Comparable only against
    /// other hits from the same pattern.
    pub score: i32,
    /// Matched **character** indices into the path's final component,
    /// ascending.
    ///
    /// Empty is a normal answer twice over: a match that landed entirely in
    /// the folders above the name has nothing to underline in it, and so does
    /// a regular expression that matched the empty string. A row that
    /// underlined something anyway would be lying about where the match was.
    pub positions: Vec<u32>,
}

impl Pattern {
    /// Runs this pattern against one path, relative to whatever the caller
    /// treats as the root.
    ///
    /// `None` when nothing matched, and also whenever the pattern is not one
    /// that can match — [`Pattern::Unfiltered`] means the caller should be
    /// showing its own list instead, and [`Pattern::Invalid`] cannot match
    /// anything until it compiles.
    #[must_use]
    pub fn find(&self, path: &str) -> Option<Hit> {
        match self {
            Self::Unfiltered | Self::Invalid(_) => None,
            Self::Fuzzy(query) => match_path(query, path).map(|found| Hit {
                score: found.score,
                positions: found.matched_in_name().to_vec(),
            }),
            Self::Regex(regex) => regex_hit(regex, path),
        }
    }
}

/// The name first, then the whole path — see the module documentation.
fn regex_hit(regex: &Regex, path: &str) -> Option<Hit> {
    let start = basename_start(path);
    let name = path.get(start.byte..).unwrap_or("");

    if let Some(found) = regex.find(name) {
        return Some(Hit {
            score: weighted(PathField::Name),
            positions: char_positions(name, found.start(), found.end(), 0),
        });
    }
    let found = regex.find(path)?;
    Some(Hit {
        score: weighted(PathField::Path),
        positions: char_positions(path, found.start(), found.end(), start.chars),
    })
}

/// [`REGEX_BASE`] with a field's weight applied.
const fn weighted(field: PathField) -> i32 {
    REGEX_BASE * field.weight() / 100
}

/// The character indices of a byte range, rebased onto the basename.
///
/// Byte offsets in, character indices out, because a caller underlining a
/// match highlights glyphs: a byte range through a multi-byte character would
/// underline half of one. Positions below `base` are **dropped rather than
/// clamped**, exactly as the fuzzy matcher drops them — clamping would pile
/// several matched characters onto the name's first glyph and underline a
/// character nobody's pattern touched.
///
/// An empty range yields no positions, which is the honest reading of a
/// regular expression that matched the empty string: it matched, and there is
/// nothing to point at.
fn char_positions(haystack: &str, start: usize, end: usize, base: usize) -> Vec<u32> {
    haystack
        .char_indices()
        .enumerate()
        .filter(|&(_, (byte, _))| byte >= start && byte < end)
        .filter_map(|(index, _)| index.checked_sub(base))
        .map(|index| u32::try_from(index).unwrap_or(u32::MAX))
        .collect()
}
