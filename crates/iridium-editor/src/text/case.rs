//! Word splitting and case restyling.
//!
//! Owned rather than delegated to `heck` or `convert_case`, both of which
//! would be new dependencies for what is a couple of hundred lines. The
//! deciding factor is not size, though: the content this editor exists to
//! serve is JSON and Markdown, so the input is routinely not ASCII and not
//! well-formed identifiers, and the exact behaviour on `"user_id"`, `"HTTPResponse"`,
//! `"café-au-lait"` and `"v2Beta"` is something worth pinning in this
//! repository's own tests rather than inheriting.
//!
//! # The model
//!
//! Everything here is two steps: split a string into [`words`], then re-join
//! them in a target [`CaseStyle`]. That split is the whole design — it is what
//! makes `snake_case` → `camelCase` a round trip through a common form rather
//! than N² direct conversions, and it is where every interesting decision
//! lives.

use std::borrow::Cow;

/// A target style for [`restyle`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaseStyle {
    /// `lower snake`: words lowercased, joined with `_`.
    Snake,
    /// `SCREAMING_SNAKE`: words uppercased, joined with `_`.
    ScreamingSnake,
    /// `kebab-case`: words lowercased, joined with `-`.
    Kebab,
    /// `camelCase`: first word lowercased, the rest capitalised, joined with
    /// nothing.
    Camel,
    /// `PascalCase`: every word capitalised, joined with nothing.
    Pascal,
    /// `Title Case`: every word capitalised, joined with a space.
    Title,
}

impl CaseStyle {
    /// The separator placed between words.
    const fn separator(self) -> &'static str {
        match self {
            Self::Snake | Self::ScreamingSnake => "_",
            Self::Kebab => "-",
            Self::Camel | Self::Pascal => "",
            Self::Title => " ",
        }
    }
}

/// Characters treated as word separators wherever they appear.
///
/// Anything else — including every non-ASCII character — is word material, so
/// `café-au-lait` splits into three words and `naïve` stays one.
const SEPARATORS: [char; 4] = ['_', '-', ' ', '\t'];

/// Splits `text` into words.
///
/// Word boundaries are, in order of precedence:
///
/// 1. any run of `_`, `-`, space or tab, which is consumed and never appears
///    in the output;
/// 2. a lowercase-or-digit followed by an uppercase (`userId` → `user`, `Id`);
/// 3. a run of uppercase followed by an uppercase-then-lowercase, which starts
///    the new word at the *last* uppercase (`HTTPResponse` → `HTTP`,
///    `Response`, not `HTTPR`, `esponse`).
///
/// Rule 3 is the one that separates a usable implementation from a naive one,
/// and it is why acronyms survive a round trip through `PascalCase`.
///
/// Digits are word material and never start a word on their own, so `v2Beta`
/// is `v2`, `Beta` and `utf8` stays one word — splitting on digits would turn
/// every version string in a JSON file into confetti.
///
/// Returns an empty vector for input with no word characters at all.
#[must_use]
pub fn words(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();

    for (index, &ch) in chars.iter().enumerate() {
        if SEPARATORS.contains(&ch) {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }

        // Rule 2: a case rise ends the previous word.
        let rises = ch.is_uppercase() && index > 0 && ends_a_word(chars[index - 1]);

        // Rule 3: the tail of an acronym belongs to the word that follows it,
        // but only when a lowercase actually follows — `HTTPS` on its own is
        // one word, `HTTPSConnection` is two.
        let acronym_tail = ch.is_uppercase()
            && index > 0
            && chars[index - 1].is_uppercase()
            && chars.get(index + 1).is_some_and(|next| next.is_lowercase());

        if (rises || acronym_tail) && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }

        current.push(ch);
    }

    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Whether `ch` can end a word that a following uppercase would break.
///
/// Separators are excluded because they end the word themselves, so an
/// uppercase after one must not be treated as a *second* boundary.
fn ends_a_word(ch: char) -> bool {
    (ch.is_lowercase() || ch.is_numeric()) && !SEPARATORS.contains(&ch)
}

/// Re-joins `text`'s [`words`] in the given style.
///
/// Text with no word characters is returned unchanged, so running a transform
/// over a selection of punctuation or whitespace is a no-op rather than a
/// deletion.
#[must_use]
pub fn restyle(text: &str, style: CaseStyle) -> String {
    let words = words(text);
    if words.is_empty() {
        return text.to_owned();
    }

    let mut out = String::with_capacity(text.len());
    for (index, word) in words.iter().enumerate() {
        if index > 0 {
            out.push_str(style.separator());
        }
        match style {
            CaseStyle::Snake | CaseStyle::Kebab => out.push_str(&lower(word)),
            CaseStyle::ScreamingSnake => out.push_str(&upper(word)),
            CaseStyle::Pascal | CaseStyle::Title => out.push_str(&capitalize(word)),
            CaseStyle::Camel => {
                if index == 0 {
                    out.push_str(&lower(word));
                } else {
                    out.push_str(&capitalize(word));
                }
            },
        }
    }
    out
}

/// Uppercases every character.
///
/// Borrows when the input is already uppercase, which is the common case for
/// the idempotent second press of a transform key.
#[must_use]
pub fn upper(text: &str) -> Cow<'_, str> {
    if text.chars().all(|ch| !ch.is_lowercase()) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.to_uppercase())
    }
}

/// Lowercases every character, borrowing when already lowercase.
#[must_use]
pub fn lower(text: &str) -> Cow<'_, str> {
    if text.chars().all(|ch| !ch.is_uppercase()) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.to_lowercase())
    }
}

/// Swaps the case of every cased character, leaving everything else alone.
#[must_use]
pub fn swap(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_uppercase() {
            out.extend(ch.to_lowercase());
        } else if ch.is_lowercase() {
            out.extend(ch.to_uppercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Uppercases the first character and lowercases the rest.
///
/// The first *character*, not the first byte, and via `to_uppercase` rather
/// than `to_ascii_uppercase`, so `ß` becomes `SS` and `ﬁ` becomes `FI` — a
/// byte-wise implementation would corrupt both.
#[must_use]
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    let mut out: String = first.to_uppercase().collect();
    out.push_str(&chars.as_str().to_lowercase());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separators_split_and_disappear() {
        assert_eq!(words("user_id"), ["user", "id"]);
        assert_eq!(words("user-id"), ["user", "id"]);
        assert_eq!(words("user id"), ["user", "id"]);
        assert_eq!(words("user\tid"), ["user", "id"]);
        // Runs collapse, and leading/trailing separators contribute nothing.
        assert_eq!(words("__user___id__"), ["user", "id"]);
        assert_eq!(words(" - _ "), Vec::<String>::new());
    }

    #[test]
    fn a_case_rise_ends_a_word() {
        assert_eq!(words("userId"), ["user", "Id"]);
        assert_eq!(words("parseJSONValue"), ["parse", "JSON", "Value"]);
    }

    #[test]
    fn an_acronym_keeps_its_letters() {
        // The boundary falls before the LAST uppercase, not the first.
        assert_eq!(words("HTTPResponse"), ["HTTP", "Response"]);
        assert_eq!(words("XMLHTTPRequest"), ["XMLHTTP", "Request"]);
        // With no lowercase following, there is no boundary at all.
        assert_eq!(words("HTTPS"), ["HTTPS"]);
        assert_eq!(words("ID"), ["ID"]);
    }

    #[test]
    fn digits_are_word_material() {
        // Splitting on digits would shred every version string in a JSON file.
        assert_eq!(words("utf8"), ["utf8"]);
        assert_eq!(words("v2Beta"), ["v2", "Beta"]);
        assert_eq!(words("sha256_hash"), ["sha256", "hash"]);
    }

    #[test]
    fn non_ascii_is_word_material() {
        assert_eq!(words("naïve"), ["naïve"]);
        assert_eq!(words("café-au-lait"), ["café", "au", "lait"]);
        // Non-ASCII case rises count, because they are case rises.
        assert_eq!(words("größeWert"), ["größe", "Wert"]);
    }

    #[test]
    fn every_style_round_trips_a_multi_word_name() {
        for input in ["user_id", "userId", "UserId", "user-id", "User Id"] {
            assert_eq!(restyle(input, CaseStyle::Snake), "user_id", "from {input}");
            assert_eq!(restyle(input, CaseStyle::Kebab), "user-id", "from {input}");
            assert_eq!(restyle(input, CaseStyle::Camel), "userId", "from {input}");
            assert_eq!(restyle(input, CaseStyle::Pascal), "UserId", "from {input}");
            assert_eq!(restyle(input, CaseStyle::Title), "User Id", "from {input}");
            assert_eq!(
                restyle(input, CaseStyle::ScreamingSnake),
                "USER_ID",
                "from {input}"
            );
        }
    }

    #[test]
    fn an_acronym_survives_a_pascal_round_trip() {
        // The failure this pins: HTTPResponse -> HttpResponse -> HTTPRESPONSE.
        let snake = restyle("HTTPResponse", CaseStyle::Snake);
        assert_eq!(snake, "http_response");
        assert_eq!(restyle(&snake, CaseStyle::Pascal), "HttpResponse");
        // And the acronym itself is not mangled on the way through.
        assert_eq!(
            restyle("parseJSONValue", CaseStyle::Snake),
            "parse_json_value"
        );
    }

    #[test]
    fn text_with_no_words_is_returned_unchanged() {
        // A transform over punctuation must be a no-op, never a deletion.
        for input in ["", "   ", "---", "{}", "\t\t"] {
            for style in [
                CaseStyle::Snake,
                CaseStyle::ScreamingSnake,
                CaseStyle::Kebab,
                CaseStyle::Camel,
                CaseStyle::Pascal,
                CaseStyle::Title,
            ] {
                assert_eq!(restyle(input, style), input, "{input:?} in {style:?}");
            }
        }
    }

    #[test]
    fn restyling_is_idempotent() {
        for style in [
            CaseStyle::Snake,
            CaseStyle::ScreamingSnake,
            CaseStyle::Kebab,
            CaseStyle::Camel,
            CaseStyle::Pascal,
            CaseStyle::Title,
        ] {
            let once = restyle("parseJSONValue", style);
            let twice = restyle(&once, style);
            assert_eq!(once, twice, "{style:?} is not idempotent");
        }
    }

    #[test]
    fn capitalize_handles_multi_character_uppercase() {
        // `ß` uppercases to two characters; a byte-wise implementation would
        // either panic or produce mojibake.
        assert_eq!(capitalize("ßeta"), "SSeta");
        assert_eq!(capitalize("ément"), "Ément");
        assert_eq!(capitalize("aBC"), "Abc");
        assert_eq!(capitalize(""), "");
    }

    #[test]
    fn swap_leaves_uncased_characters_alone() {
        assert_eq!(swap("Hello, World! 42"), "hELLO, wORLD! 42");
        assert_eq!(swap("café"), "CAFÉ");
        // Swapping twice returns the original whenever no character changes
        // width — which is the case for every character here.
        assert_eq!(swap(&swap("MiXeD")), "MiXeD");
    }

    #[test]
    fn upper_and_lower_borrow_when_there_is_nothing_to_do() {
        assert!(matches!(upper("ABC 123"), Cow::Borrowed(_)));
        assert!(matches!(lower("abc 123"), Cow::Borrowed(_)));
        assert!(matches!(upper("abc"), Cow::Owned(_)));
        assert!(matches!(lower("ABC"), Cow::Owned(_)));
    }
}
