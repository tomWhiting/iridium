//! Field choice for one query against one path.
//!
//! The matching itself lives in [`match_field`], which knows nothing about
//! paths. What is path-specific — and therefore here — is *which strings of a
//! path are worth matching against*, and how much a hit in each is worth. It is
//! the same shape the command palette's field chooser has, deliberately: a file
//! list and a command list that ranked by different rules would put different
//! things first for the same query, and nobody reports that as a bug. They just
//! stop trusting the editor.
//!
//! # Positions always index the basename
//!
//! The palette reports *which field* matched, because a hit on an alias cannot
//! be underlined inside the title. A path list has the opposite problem: it
//! draws only the basename, indented under its folders, so a caller has one
//! string to highlight no matter which field won. [`PathMatch`] therefore
//! translates the winning assignment into basename positions itself, dropping
//! the characters that landed in the directory chain. A caller that wants to
//! know which field won can still ask; a caller that only wants to underline
//! what the user typed never has to.

use super::{FieldMatch, MAX_QUERY_CHARS, Query, match_field};

/// Which of a path's texts the query matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PathField {
    /// The final component — `panel.rs` of `src/file_tree/panel.rs`.
    Name,
    /// The whole path as given, separators and all.
    Path,
}

impl PathField {
    /// The stable wire name, for a host that renders the match.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Path => "path",
        }
    }

    /// The share of a raw match score this field keeps, as a percentage.
    ///
    /// A basename hit is worth full marks: the name is what someone is trying
    /// to remember, and the folders above it are where it happens to live. The
    /// path still scores, and well enough that typing part of a directory finds
    /// its contents, but a query that names the file beats a query that only
    /// brushes the chain leading to it.
    ///
    /// Integer percentages rather than floats, for the reason the palette's
    /// weights are integers: every face must compute bit-identical scores.
    #[must_use]
    pub const fn weight(self) -> i32 {
        match self {
            Self::Name => 100,
            Self::Path => 70,
        }
    }
}

/// A scored match of one query against one path.
#[derive(Debug, Clone, Copy)]
pub struct PathMatch {
    /// The score, higher being better, with the field's weight already
    /// applied. Comparable against other scores from this function only.
    pub score: i32,
    /// Which text won.
    pub field: PathField,
    /// Matched **character** indices into the *basename*, ascending. Only the
    /// first [`Self::len`] entries are meaningful.
    positions: [u32; MAX_QUERY_CHARS],
    /// How many of [`Self::positions`] were filled.
    len: usize,
}

impl PathMatch {
    /// The matched character indices within the basename, without the unused
    /// tail.
    ///
    /// Empty is a normal answer, not a failure: a path-field win whose
    /// characters all landed in the directory chain highlights nothing in the
    /// name, and a row that underlined something anyway would be lying about
    /// where the match was.
    #[must_use]
    pub fn matched_in_name(&self) -> &[u32] {
        &self.positions[..self.len]
    }
}

/// Scores `query` against `path`, taking the better of its two fields.
///
/// `path` is matched as given — relative to whatever the caller considers the
/// root — because the separators are part of what the user types and part of
/// what the matcher rewards: the scorer counts `/` and `\` as separators, so a
/// basename scores as a word start rather than as the tail of one long run.
///
/// `None` when the query is not a subsequence of either field, and also when
/// the query is empty — an empty query means "everything", which is a listing
/// decision for the caller rather than a match.
#[must_use]
pub fn match_path(query: &Query, path: &str) -> Option<PathMatch> {
    let name_start = basename_start(path);
    let name = path.get(name_start.byte..).unwrap_or("");

    let by_name = match_field(query, name).map(|found| (PathField::Name, found));
    let by_path = match_field(query, path).map(|found| (PathField::Path, found));

    // Ties break toward the name, which is the more authoritative field —
    // exactly as a title hit beats an alias hit of the same strength in the
    // palette.
    let (field, found) = match (by_name, by_path) {
        (Some((name_field, name_match)), Some((path_field, path_match))) => {
            let name_score = weighted(name_match.score, name_field);
            let path_score = weighted(path_match.score, path_field);
            if path_score > name_score {
                (path_field, path_match)
            } else {
                (name_field, name_match)
            }
        },
        (Some(only), None) | (None, Some(only)) => only,
        (None, None) => return None,
    };

    let (positions, len) = name_positions(&found, field, name_start.chars);
    Some(PathMatch {
        score: weighted(found.score, field),
        field,
        positions,
        len,
    })
}

/// Where a path's final component begins, in both bytes and characters.
///
/// Both are needed and neither derives from the other cheaply: the byte offset
/// slices the name out, and the character offset rebases the matcher's
/// character positions onto it.
///
/// A path ending in a separator has an *empty* final component, which is the
/// honest answer — `src/` names a directory, and the characters the user typed
/// are all in the chain.
fn basename_start(path: &str) -> Offset {
    let mut byte = 0;
    let mut chars = 0;
    let mut seen = 0;
    for (index, character) in path.char_indices() {
        seen += 1;
        if character == '/' || character == '\\' {
            byte = index + character.len_utf8();
            chars = seen;
        }
    }
    Offset { byte, chars }
}

/// A position in a string, counted both ways.
#[derive(Debug, Clone, Copy)]
struct Offset {
    /// The byte offset, always on a character boundary.
    byte: usize,
    /// The character offset.
    chars: usize,
}

/// `score` with `field`'s weight applied.
const fn weighted(score: i32, field: PathField) -> i32 {
    score * field.weight() / 100
}

/// Rebases a match's positions onto the basename, dropping what fell above it.
///
/// A name-field match is already basename-relative and passes through. A
/// path-field match is rebased by subtracting the basename's character offset,
/// and any position below it is discarded rather than clamped: clamping would
/// pile several matched characters onto the name's first glyph and underline a
/// character the user did not type.
fn name_positions(
    found: &FieldMatch,
    field: PathField,
    name_chars: usize,
) -> ([u32; MAX_QUERY_CHARS], usize) {
    let mut positions = [0_u32; MAX_QUERY_CHARS];
    let mut len = 0;
    let base = match field {
        PathField::Name => 0,
        PathField::Path => name_chars,
    };
    for &position in found.matched() {
        let Some(rebased) = (position as usize).checked_sub(base) else {
            continue;
        };
        // The bound is structural — `matched()` is at most `MAX_QUERY_CHARS`
        // long — but a guard is cheaper than a reason to trust that here.
        if len >= MAX_QUERY_CHARS {
            break;
        }
        positions[len] = u32::try_from(rebased).unwrap_or(u32::MAX);
        len += 1;
    }
    (positions, len)
}
