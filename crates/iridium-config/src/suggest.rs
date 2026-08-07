//! "Did you mean" for a misspelled name.
//!
//! A report that says only `unknown setting "tab_widht"` is correct and
//! useless: the user already knows they wrote something, and what they need is
//! the spelling that works. Since the loader is holding the list of real names
//! at the moment it decides a name is not in it, naming the nearest one costs
//! a comparison and saves a trip to the documentation.

/// How far apart two names may be and still be offered as a correction.
///
/// Three edits is enough for a transposition, a dropped letter and a wrong
/// one; beyond that the "nearest" name stops being a plausible intent and
/// starts being noise, and a confident wrong suggestion is worse than none.
const MAX_DISTANCE: usize = 3;

/// The name in `candidates` closest to `name`, when one is close enough.
///
/// Compared case-insensitively over `char`s rather than bytes, so a name with
/// a non-ASCII character is measured in letters rather than in the number of
/// bytes UTF-8 happened to spend on them.
#[must_use]
pub fn nearest<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let subject: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    candidates
        .iter()
        .map(|candidate| {
            let other: Vec<char> = candidate.chars().flat_map(char::to_lowercase).collect();
            (distance(&subject, &other), *candidate)
        })
        .filter(|(distance, _)| *distance <= MAX_DISTANCE)
        // `min_by_key` keeps the first of equal keys, and the candidate list
        // is in declaration order, so the suggestion is stable rather than
        // whichever name the iterator happened to reach first.
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, candidate)| candidate)
}

/// The Levenshtein distance between two sequences of characters.
///
/// Two rows rather than a full matrix: the distance is all that is wanted, not
/// the edits that produce it, and a name is short enough that the allocation
/// is the only cost worth naming.
fn distance(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }

    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];

    for (row, left_char) in left.iter().enumerate() {
        current[0] = row + 1;
        for (column, right_char) in right.iter().enumerate() {
            let substitution = usize::from(left_char != right_char);
            current[column + 1] = (previous[column] + substitution)
                .min(previous[column + 1] + 1)
                .min(current[column] + 1);
        }
        core::mem::swap(&mut previous, &mut current);
    }

    // The swap at the end of the last row leaves the answer in `previous`.
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::{distance, nearest};

    /// The names a real `[editor]` table is checked against.
    const SETTINGS: &[&str] = &[
        "tab_width",
        "insert_spaces",
        "font_size",
        "line_height",
        "show_minimap",
    ];

    #[test]
    fn a_transposition_is_corrected() {
        assert_eq!(nearest("tab_widht", SETTINGS), Some("tab_width"));
    }

    #[test]
    fn a_dropped_letter_is_corrected() {
        assert_eq!(nearest("fontsize", SETTINGS), Some("font_size"));
    }

    #[test]
    fn case_alone_is_not_a_difference() {
        assert_eq!(nearest("TabWidth", SETTINGS), Some("tab_width"));
    }

    #[test]
    fn a_name_nothing_resembles_gets_no_suggestion() {
        // A confident wrong suggestion sends the user to change a line that
        // was never the problem, which costs more than saying nothing.
        assert_eq!(nearest("colour_scheme", SETTINGS), None);
    }

    #[test]
    fn an_empty_name_suggests_nothing_rather_than_the_shortest_name() {
        assert_eq!(nearest("", SETTINGS), None);
    }

    #[test]
    fn distance_counts_letters_rather_than_bytes() {
        // Two names differing by one accented letter are one edit apart, not
        // two — which is what a byte-wise comparison would report, and is
        // enough to push a real typo past the threshold.
        let left: Vec<char> = "café".chars().collect();
        let right: Vec<char> = "cafe".chars().collect();
        assert_eq!(distance(&left, &right), 1);
    }

    #[test]
    fn distance_from_nothing_is_the_length() {
        let word: Vec<char> = "width".chars().collect();
        assert_eq!(distance(&[], &word), 5);
        assert_eq!(distance(&word, &[]), 5);
    }
}
