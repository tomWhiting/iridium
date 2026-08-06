//! The normalized query a fuzzy match runs against.

/// The longest query the matcher considers.
///
/// Longer queries are truncated, which can only make the result set *larger*
/// than the user intended, never smaller. Thirty-two characters is far beyond
/// any command title or path segment, and the cap is what lets match positions
/// live in a fixed array.
pub const MAX_QUERY_CHARS: usize = 32;

/// Folds one character for case-insensitive comparison.
///
/// Takes the first character of the full Unicode lowercase mapping. A handful of
/// characters lowercase to more than one (`İ` becomes `i` plus a combining dot),
/// and this treats them as their first: right for search, and it keeps the
/// comparison a single `char` against a single `char`.
pub(super) fn fold(character: char) -> char {
    character.to_lowercase().next().unwrap_or(character)
}

/// A search query, normalized and capped.
///
/// Holds both the folded and the original characters: folded to compare, original
/// so an exactly-cased match can be rewarded without a second pass.
#[derive(Debug, Clone)]
pub struct Query {
    pub(super) folded: [char; MAX_QUERY_CHARS],
    pub(super) raw: [char; MAX_QUERY_CHARS],
    pub(super) len: usize,
    truncated: bool,
}

impl Query {
    /// Normalizes `text` into a query: trimmed, capped at [`MAX_QUERY_CHARS`].
    ///
    /// Interior spaces are kept, because multi-word aliases such as `one cursor`
    /// are meant to be matched with the space typed.
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut folded = ['\0'; MAX_QUERY_CHARS];
        let mut raw = ['\0'; MAX_QUERY_CHARS];
        let mut len = 0;
        let mut truncated = false;
        for character in text.trim().chars() {
            if len == MAX_QUERY_CHARS {
                truncated = true;
                break;
            }
            raw[len] = character;
            folded[len] = fold(character);
            len += 1;
        }
        Self {
            folded,
            raw,
            len,
            truncated,
        }
    }

    /// The number of characters the matcher will use.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the query matches everything, because nothing was typed.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether characters beyond [`MAX_QUERY_CHARS`] were discarded.
    #[must_use]
    pub const fn is_truncated(&self) -> bool {
        self.truncated
    }
}
