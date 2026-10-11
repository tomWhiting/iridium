//! Search state and navigation regression tests, extracted without behavioral changes.
use super::*;
mod unicode;

fn create_test_doc() -> Document {
    Document::new("foo bar foo baz foo\nfoo in line 2")
}

#[test]
fn find_all_basic() {
    let doc = create_test_doc();
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 4);
    assert_eq!(state.current_match, Some(0));
}

#[test]
fn find_all_case_insensitive() {
    let doc = Document::new("FOO foo Foo fOO");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();

    // Default is case-insensitive
    assert_eq!(state.match_count(), 4);
}

#[test]
fn find_all_case_sensitive() {
    let doc = Document::new("FOO foo Foo fOO");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::case_sensitive(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 1);
}

#[test]
fn find_all_whole_word() {
    let doc = Document::new("foo foobar barfoo foo");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::whole_word(), &doc)
        .unwrap();

    // Should only match standalone "foo", not "foobar" or "barfoo"
    assert_eq!(state.match_count(), 2);
}

#[test]
fn find_all_regex() {
    let doc = Document::new("foo123 bar456 baz789");
    let mut state = SearchState::new();
    state
        .find_all(r"\w+\d+", &SearchOptions::regex_mode(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 3);
}

#[test]
fn find_all_regex_invalid() {
    let doc = Document::new("test");
    let mut state = SearchState::new();
    let result = state.find_all("[invalid", &SearchOptions::regex_mode(), &doc);

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid regex"));
}

#[test]
fn next_match_wraps() {
    let doc = Document::new("foo foo");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();

    assert_eq!(state.current_match, Some(0));
    state.next_match();
    assert_eq!(state.current_match, Some(1));
    state.next_match();
    assert_eq!(state.current_match, Some(0)); // Wrapped
}

#[test]
fn previous_match_wraps() {
    let doc = Document::new("foo foo");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();

    assert_eq!(state.current_match, Some(0));
    state.previous_match();
    assert_eq!(state.current_match, Some(1)); // Wrapped to last
}

#[test]
fn current_range() {
    let doc = Document::new("foo bar");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();

    let range = state.current_range().unwrap();
    assert_eq!(range.start, Position::new(0, 0));
    assert_eq!(range.end, Position::new(0, 3));
}

#[test]
fn goto_nearest_match() {
    let doc = Document::new("foo bar foo baz foo");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();

    // Start at position after second foo
    state.goto_nearest_match(Position::new(0, 10));

    // Should go to third foo at position 16
    assert_eq!(state.current_match, Some(2));
}

#[test]
fn clear_resets_state() {
    let doc = Document::new("foo bar");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();
    assert!(state.has_matches());

    state.clear();
    assert!(!state.has_matches());
    assert!(state.query.is_empty());
    assert!(!state.is_active);
}

#[test]
fn empty_query_clears_matches() {
    let doc = Document::new("foo bar");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();
    assert!(state.has_matches());

    state.find_all("", &SearchOptions::default(), &doc).unwrap();
    assert!(!state.has_matches());
    assert!(!state.is_active);
}

#[test]
fn multiline_search() {
    let doc = Document::new("foo bar\nfoo baz");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 2);

    // First match on line 0
    let first = &state.matches[0];
    assert_eq!(first.start.line, 0);
    assert_eq!(first.start.column, 0);

    // Second match on line 1
    let second = &state.matches[1];
    assert_eq!(second.start.line, 1);
    assert_eq!(second.start.column, 0);
}

#[test]
fn update_query_detects_changes() {
    let doc = Document::new("foo bar baz");
    let mut state = SearchState::new();
    state
        .find_all("foo", &SearchOptions::default(), &doc)
        .unwrap();
    assert_eq!(state.match_count(), 1);

    // Query that finds different number of matches
    let changed = state.update_query("a", &doc).unwrap();
    assert!(changed);
    assert_eq!(state.match_count(), 2); // "a" in bar and baz
}

#[test]
fn escape_regex_special_chars() {
    assert_eq!(SearchState::escape_regex("foo.bar"), r"foo\.bar");
    assert_eq!(SearchState::escape_regex("[test]"), r"\[test\]");
}

/// Converts every match back to byte offsets and asserts each range is
/// valid and char-boundary-aligned in the original text, returning the
/// matched slices.
fn assert_matches_slice_cleanly<'a>(
    state: &SearchState,
    doc: &Document,
    text: &'a str,
) -> Vec<&'a str> {
    state
        .all_matches()
        .iter()
        .map(|range| {
            let start = doc
                .position_to_offset(range.start)
                .expect("match start must map to a valid offset");
            let end = doc
                .position_to_offset(range.end)
                .expect("match end must map to a valid offset");
            text.get(start..end)
                .expect("match range must be char-boundary-aligned in the original text")
        })
        .collect()
}

#[test]
fn find_case_insensitive_dotted_capital_i_is_boundary_safe() {
    // 'İ' (U+0130) lowercases to "i\u{307}" (2 bytes -> 3 bytes). The old
    // lowercase-both-sides approach desynced byte offsets from the original
    // text here. Unicode simple case folding (used by the regex engine)
    // does not fold 'İ' to 'i', so "istanbul" does not match "İstanbul" —
    // and crucially, no misaligned range is ever produced.
    let text = "İstanbul";
    let doc = Document::new(text);
    let mut state = SearchState::new();
    state
        .find_all("istanbul", &SearchOptions::default(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 0);
    assert!(assert_matches_slice_cleanly(&state, &doc, text).is_empty());
}

#[test]
fn find_case_insensitive_ascii_mixed_case_ranges() {
    let text = "Hello HELLO hello hElLo";
    let doc = Document::new(text);
    let mut state = SearchState::new();
    state
        .find_all("hello", &SearchOptions::default(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 4);
    let slices = assert_matches_slice_cleanly(&state, &doc, text);
    assert_eq!(slices, vec!["Hello", "HELLO", "hello", "hElLo"]);
}

#[test]
fn find_case_insensitive_sharp_s_no_misaligned_ranges() {
    // 'ẞ' (U+1E9E, 3 bytes) lowercases to 'ß' (U+00DF, 2 bytes), so the
    // lowercased haystack is shorter than the original. The old approach
    // produced ranges that were out of sync with (and not boundary-aligned
    // in) the original text. Property: every reported match must slice
    // cleanly out of the original haystack.
    let text = "GROẞE straße";
    let doc = Document::new(text);
    let mut state = SearchState::new();
    state
        .find_all("ße", &SearchOptions::default(), &doc)
        .unwrap();

    // Matches "ẞE" in "GROẞE" (via case folding) and "ße" in "straße".
    assert_eq!(state.match_count(), 2);
    let slices = assert_matches_slice_cleanly(&state, &doc, text);
    assert_eq!(slices, vec!["ẞE", "ße"]);
    for slice in slices {
        assert_eq!(slice.to_lowercase(), "ße");
    }
}

#[test]
fn find_case_insensitive_multibyte_whole_word() {
    // Whole-word filtering must keep working on the case-insensitive path.
    let text = "straße straßenbahn STRAẞE";
    let doc = Document::new(text);
    let mut state = SearchState::new();
    state
        .find_all("straße", &SearchOptions::whole_word(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 2);
    let slices = assert_matches_slice_cleanly(&state, &doc, text);
    assert_eq!(slices, vec!["straße", "STRAẞE"]);
}

#[test]
fn find_case_insensitive_overlapping_matches_preserved() {
    // The literal search historically reports overlapping matches (resume
    // one position past each match start); the case-insensitive path must
    // preserve that.
    let text = "aAa";
    let doc = Document::new(text);
    let mut state = SearchState::new();
    state
        .find_all("aa", &SearchOptions::default(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 2);
    let slices = assert_matches_slice_cleanly(&state, &doc, text);
    assert_eq!(slices, vec!["aA", "Aa"]);
}

#[test]
fn find_case_sensitive_multibyte_query_resumes_on_a_character_boundary() {
    // The case-sensitive literal path resumes one *byte* past each match
    // start. When the match starts on a multi-byte character that byte is
    // inside it, and the next `text[start..]` panics on a non-boundary
    // index — a hard crash in the search path for any accented query.
    let text = "ééé";
    let doc = Document::new(text);
    let mut state = SearchState::new();
    state
        .find_all("é", &SearchOptions::case_sensitive(), &doc)
        .unwrap();

    assert_eq!(state.match_count(), 3);
    let slices = assert_matches_slice_cleanly(&state, &doc, text);
    assert_eq!(slices, vec!["é", "é", "é"]);
}

#[test]
fn whole_word_reads_a_non_ascii_neighbour_as_part_of_the_word() {
    // The boundary test inspects one *byte* on each side, so every
    // non-ASCII neighbour reads as a non-word character and the match is
    // reported as a whole word. `na` is not a word in `naïve`.
    let doc = Document::new("naïve");
    let mut state = SearchState::new();
    state
        .find_all("na", &SearchOptions::whole_word(), &doc)
        .unwrap();
    assert_eq!(state.match_count(), 0, "`na` is not a whole word here");

    // The same divergence on the leading side: the character before `ve`
    // is `ï`, which is as much a word character as `i` is.
    state
        .find_all("ve", &SearchOptions::whole_word(), &doc)
        .unwrap();
    assert_eq!(state.match_count(), 0, "`ve` is not a whole word here");
}

#[test]
fn validate_regex_valid() {
    assert!(SearchState::validate_regex(r"\w+").is_ok());
}

#[test]
fn validate_regex_invalid() {
    assert!(SearchState::validate_regex("[invalid").is_err());
}
