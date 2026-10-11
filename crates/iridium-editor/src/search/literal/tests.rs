//! Differential checks against the previous Unicode regex search semantics.
use std::ops::Range;

use regex::RegexBuilder;

use super::FoldedLiteral;
use crate::search::{SearchOptions, SearchState};
use crate::{Document, Editor};

fn ranges(query: &str, text: &str) -> Result<Vec<Range<usize>>, String> {
    Ok(FoldedLiteral::new(query)?.matches(text).collect())
}

fn regex_ranges(query: &str, text: &str) -> Result<Vec<Range<usize>>, String> {
    if query.is_empty() {
        return Ok(Vec::new());
    }
    let regex = RegexBuilder::new(&regex::escape(query))
        .case_insensitive(true)
        .build()
        .map_err(|error| error.to_string())?;
    let mut found = Vec::new();
    let mut offset = 0;
    while let Some(matched) = regex.find_at(text, offset) {
        found.push(matched.range());
        let character = text[matched.start()..]
            .chars()
            .next()
            .ok_or("empty literal match")?;
        offset = matched.start() + character.len_utf8();
    }
    Ok(found)
}

#[test]
fn empty_query_and_haystack_have_no_matches() -> Result<(), String> {
    assert!(ranges("", "text")?.is_empty());
    assert!(ranges("query", "")?.is_empty());
    assert!(ranges("longer", "short")?.is_empty());
    Ok(())
}

#[test]
fn folding_preserves_overlaps_and_original_byte_offsets() -> Result<(), String> {
    assert_eq!(ranges("kK", "\u{212a}kK")?, vec![0..4, 3..5]);
    assert_eq!(ranges("ß", "ẞß")?, vec![0..3, 3..5]);
    assert_eq!(ranges("σσ", "Σςσ")?, vec![0..4, 2..6]);
    Ok(())
}

#[test]
fn folding_does_not_expand_normalize_or_interpret_regex_metacharacters() -> Result<(), String> {
    for (query, text) in [
        ("i", "İıiI"),
        ("ss", "ßẞssSS"),
        ("é", "e\u{301}Éé"),
        ("a.[x]$", "A.[X]$ aZx"),
        ("ſ", "sSſ"),
    ] {
        assert_eq!(
            ranges(query, text)?,
            regex_ranges(query, text)?,
            "{query:?} / {text:?}"
        );
    }
    Ok(())
}

#[test]
fn fallback_prefixes_keep_overlapping_matches_after_near_misses() -> Result<(), String> {
    for (query, text) in [
        ("ababac", "ababababacababaCABABAC"),
        ("aabaa", "aabaaabaaabaa"),
        ("aaa", "aaXaaaaa"),
        ("x", "XxxZx"),
    ] {
        assert_eq!(ranges(query, text)?, regex_ranges(query, text)?);
    }
    Ok(())
}

#[test]
fn multiline_controls_keep_exact_ranges() -> Result<(), String> {
    let text = "a\r\n\t\u{1b}\0 A\r\n\t\u{1b}\0";
    let query = "A\r\n\t\u{1b}\0";
    assert_eq!(ranges(query, text)?, regex_ranges(query, text)?);
    Ok(())
}

#[test]
fn deterministic_mixed_unicode_queries_agree_with_regex_reference() -> Result<(), String> {
    let alphabet: Vec<char> = "aAbBkK\u{212a}ΣσςßẞİıiIſsSéÉ猫\n\t.[ ]".chars().collect();
    let mut seed = 912_u64;
    let mut choose = |bound: usize| {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        usize::try_from(seed >> 32)
            .map(|value| value % bound)
            .map_err(|e| e.to_string())
    };
    for case in 0..600 {
        let mut characters = Vec::new();
        for _ in 0..60 {
            characters.push(alphabet[choose(alphabet.len())?]);
        }
        let start = choose(50)?;
        let length = 1 + choose(10)?;
        let query: String = characters[start..start + length].iter().collect();
        // Include adjacent copies so each case also exercises overlap restart.
        let text = format!(
            "{}{}{}",
            characters.iter().collect::<String>(),
            query,
            query
        );
        assert_eq!(
            ranges(&query, &text)?,
            regex_ranges(&query, &text)?,
            "case {case}: {query:?} / {text:?}"
        );
    }
    Ok(())
}

#[test]
fn long_repeated_query_streams_with_query_bounded_offset_storage() -> Result<(), String> {
    let query = "abcdefghij\n".repeat(2_000);
    let text = format!("x{query}{query}");
    let matcher = FoldedLiteral::new(&query)?;
    let found = matcher.matches(&text);
    assert_eq!(found.starts.len(), query.chars().count());
    assert_eq!(found.count(), 2_001);
    Ok(())
}

#[test]
fn whole_word_matching_keeps_unicode_neighbours_and_document_coordinates() -> Result<(), String> {
    let document = Document::new("\u{212a}é k\nK猫\n\u{212a} K");
    let mut search = SearchState::new();
    search.find_all("k", &SearchOptions::whole_word(), &document)?;
    assert_eq!(search.matches.len(), 3);
    let slices: Vec<_> = search
        .matches
        .iter()
        .map(|range| document.slice(*range))
        .collect();
    assert_eq!(slices, ["k", "\u{212a}", "K"]);
    Ok(())
}

#[test]
fn variable_width_literal_replacements_undo_exactly() -> Result<(), String> {
    let original = "\u{212a}elvin kelvin KELVIN\n";
    let mut editor = Editor::with_defaults();
    editor.set_content(original);
    editor.find("kelvin", &SearchOptions::default())?;
    assert_eq!(editor.search_match_count(), 3);
    assert_eq!(editor.replace_all_matches("$1\n"), 3);
    assert_eq!(editor.content(), "$1\n $1\n $1\n\n");
    editor.undo();
    assert_eq!(editor.content(), original);
    Ok(())
}
