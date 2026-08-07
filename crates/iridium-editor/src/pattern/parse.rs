//! Reading a query field into the thing it means.

use regex::{Regex, RegexBuilder};

use crate::fuzzy::Query;

/// The character that turns the rest of a query into a regular expression.
///
/// `/` because it is what `less`, `vi` and every pager since have used for
/// "what follows is a search", and because it is the one character that
/// cannot begin a useful fuzzy query against a path: the paths being matched
/// are relative to the root, so none of them starts with a separator.
pub const REGEX_SIGIL: char = '/';

/// What a filter query means.
///
/// Parsing is separate from matching because it is *expensive and repeated*:
/// compiling a regular expression costs orders of magnitude more than testing
/// one string with it, and the rows under a query are re-filtered every time
/// a directory listing lands. A caller holds one of these across the reads
/// and rebuilds it only when the text changes.
#[derive(Debug, Clone)]
pub enum Pattern {
    /// Nothing was asked for. The caller shows its own unfiltered list.
    ///
    /// A bare [`REGEX_SIGIL`] lands here too — see the module documentation.
    Unfiltered,
    /// Rank rows by fuzzy subsequence match.
    ///
    /// Boxed for the same reason [`Self::Regex`] is: a [`Query`] holds its
    /// characters twice over in fixed arrays — folded to compare, raw to
    /// reward exact case — which is 272 bytes against the 24 the next
    /// largest variant needs. One allocation per keystroke rather than that
    /// much stack copied on every move of the enum.
    Fuzzy(Box<Query>),
    /// Keep the rows a regular expression matches.
    Regex(Box<Regex>),
    /// A regular expression that does not compile, with a short reason.
    ///
    /// Overwhelmingly this is a query half-typed rather than a query wrong,
    /// which is why it is a variant and not an error returned to a caller who
    /// would have to decide what to do about it on every keystroke.
    Invalid(String),
}

impl Pattern {
    /// Reads `text` as a query.
    ///
    /// Never fails: an uncompilable regular expression becomes
    /// [`Self::Invalid`] carrying the reason, because a filter field is
    /// always in some state and "the user is halfway through typing a
    /// character class" is one of them.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let Some(expression) = text.strip_prefix(REGEX_SIGIL) else {
            let query = Query::new(text);
            return if query.is_empty() {
                Self::Unfiltered
            } else {
                Self::Fuzzy(Box::new(query))
            };
        };
        if expression.is_empty() {
            return Self::Unfiltered;
        }
        match compile(expression) {
            Ok(regex) => Self::Regex(Box::new(regex)),
            Err(error) => Self::Invalid(short_reason(&error)),
        }
    }

    /// Whether the rows should come from this pattern rather than from the
    /// caller's own unfiltered list.
    ///
    /// True for [`Self::Invalid`], deliberately. A half-typed pattern must
    /// not put the whole tree back on screen for the one keystroke between
    /// `/[a-` and `/[a-z]`, because that flash reads as the filter having
    /// given up.
    #[must_use]
    pub const fn is_narrowing(&self) -> bool {
        !matches!(self, Self::Unfiltered)
    }

    /// Whether this pattern could match something not yet loaded.
    ///
    /// The question a caller asks before going and reading more of the disk,
    /// and **not** the same question as [`Self::is_narrowing`] — conflating
    /// them is how a panel ends up crawling a filesystem on behalf of a
    /// pattern that cannot match anything at all.
    #[must_use]
    pub const fn can_match(&self) -> bool {
        matches!(self, Self::Fuzzy(_) | Self::Regex(_))
    }

    /// The reason this pattern does not compile, if that is what it is.
    #[must_use]
    pub fn invalid_reason(&self) -> Option<&str> {
        match self {
            Self::Invalid(reason) => Some(reason),
            _ => None,
        }
    }
}

/// Compiles one expression, case-insensitively.
///
/// The builder's flag rather than a `(?i)` prefix pasted onto the pattern:
/// the prefix would be wrong the moment someone opens their own group first,
/// and it would appear in error messages at an offset the user did not type.
/// An inline `(?-i)` still overrides it, which is the documented escape.
fn compile(expression: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(expression).case_insensitive(true).build()
}

/// The one line of a regex error that says what is actually wrong.
///
/// `regex`'s own message is several lines — a heading, the pattern, a caret
/// under the offending character, then `error: <reason>`. That is right for a
/// terminal and wrong for a single row in a panel, so the reason is lifted
/// out.
///
/// Best-effort **by construction**: if that shape ever changes, the whole
/// message is used with its whitespace collapsed. Ugly, and never wrong —
/// which is the correct way round for something whose only job is to tell
/// someone why their pattern was rejected.
fn short_reason(error: &regex::Error) -> String {
    let full = error.to_string();
    if let Some(reason) = full
        .lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix("error: "))
    {
        return reason.to_owned();
    }
    full.split_whitespace().collect::<Vec<_>>().join(" ")
}
