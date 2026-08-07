//! What a pattern finds in a path, and where it says it found it.

use super::Pattern;

/// The hit for `text` against `path`, or `None`.
fn find(text: &str, path: &str) -> Option<super::Hit> {
    Pattern::parse(text).find(path)
}

/// The characters a hit underlines, as a string, so a failing assertion says
/// what was highlighted rather than which indices were.
fn underlined(text: &str, path: &str) -> Option<String> {
    let hit = find(text, path)?;
    let name = path.rsplit('/').next().unwrap_or(path);
    let characters: Vec<char> = name.chars().collect();
    Some(
        hit.positions
            .iter()
            .filter_map(|&position| characters.get(position as usize))
            .collect(),
    )
}

#[test]
fn a_regular_expression_matches_the_name() {
    assert!(find("/^mod", "engine/mod.rs").is_some());
}

#[test]
fn an_anchor_binds_to_the_name_before_it_binds_to_the_path() {
    // The reason the name is tried first. `^mod` against the path
    // `engine/mod.rs` fails — the path starts with `engine` — and a matcher
    // that only tried the path would make the most obvious pattern anyone
    // types do nothing.
    assert!(find("/^mod", "engine/mod.rs").is_some());
    assert!(find("/^engine", "engine/mod.rs").is_some());
}

#[test]
fn a_pattern_with_a_separator_reaches_the_whole_path() {
    // And the other half of the same rule: `widgets/.*[.]rs` is meaningless
    // against a basename and exact against a path.
    assert!(find("/widgets/.*[.]rs", "widgets/button.rs").is_some());
    assert!(find("/widgets/.*[.]rs", "engine/button.rs").is_none());
}

#[test]
fn a_name_hit_outranks_a_hit_only_in_the_folders_above_it() {
    // So the selection lands on the file that is named what you typed, not
    // on the first file inside a folder that is.
    let named = find("/engine", "engine/engine.rs").expect("the name matches");
    let inside = find("/engine", "engine/render.rs").expect("the path matches");
    assert!(
        named.score > inside.score,
        "name {} should beat path {}",
        named.score,
        inside.score
    );
}

#[test]
fn nothing_matching_is_nothing() {
    assert!(find("/^zzz", "engine/mod.rs").is_none());
}

#[test]
fn a_pattern_that_does_not_compile_matches_nothing() {
    assert!(find("/[", "engine/mod.rs").is_none());
}

#[test]
fn an_absent_pattern_matches_nothing_rather_than_everything() {
    // `Unfiltered` means "the caller shows its own list", not "everything
    // matches". A matcher that returned a hit here would put every row
    // through the filter's own drawing path for an empty query.
    assert!(find("", "engine/mod.rs").is_none());
    assert!(find("/", "engine/mod.rs").is_none());
}

#[test]
fn regular_expressions_ignore_case_exactly_as_the_fuzzy_half_does() {
    // A query that found a file and then lost it because the user reached
    // for `/` is indefensible.
    assert!(find("/readme", "README.md").is_some());
    assert!(find("readme", "README.md").is_some());
}

#[test]
fn case_sensitivity_is_available_to_anyone_who_asks_for_it() {
    assert!(find("/(?-i)readme", "README.md").is_none());
    assert!(find("/(?-i)README", "README.md").is_some());
}

#[test]
fn the_underlined_characters_are_the_ones_the_pattern_matched() {
    assert_eq!(underlined("/mod", "engine/mod.rs").as_deref(), Some("mod"));
    assert_eq!(
        underlined("/mod[.]rs", "engine/mod.rs").as_deref(),
        Some("mod.rs")
    );
}

#[test]
fn a_match_that_landed_above_the_name_underlines_nothing_in_it() {
    // `engine/` matched entirely in the folder chain. Underlining anything
    // in `render.rs` would be pointing at a character the pattern never
    // touched — which is why positions below the basename are dropped
    // rather than clamped onto its first glyph.
    assert_eq!(
        underlined("/engine/", "engine/render.rs").as_deref(),
        Some("")
    );
}

#[test]
fn a_match_straddling_the_separator_underlines_only_its_tail() {
    // `engine/r` covers the last folder character, the separator and the
    // name's first character. Exactly one of those is in the name.
    assert_eq!(
        underlined("/engine/r", "engine/render.rs").as_deref(),
        Some("r")
    );
}

#[test]
fn a_pattern_matching_the_empty_string_matches_and_underlines_nothing() {
    // `a*` matches the empty string at position zero of anything. It is a
    // match — the row belongs in the results — and there is nothing to point
    // at, which the row must not invent.
    let hit = find("/a*", "engine/mod.rs").expect("an empty match is still a match");
    assert!(hit.positions.is_empty());
}

#[test]
fn positions_are_ascending_and_index_inside_the_name() {
    // The property the drawing code relies on and no single example proves.
    let paths = [
        "engine/mod.rs",
        "widgets/button.rs",
        "README.md",
        "a/b/c/deep.rs",
        "spaced name/with spaces.rs",
        "ünïcödé/nämé.rs",
    ];
    let queries = [
        "/.*", "/[a-z]+", "/e", "/^.", "/.$", "/rs", "/n/", "/o.e", "mod", "button", "deep",
    ];
    for path in paths {
        let name_chars = path.rsplit('/').next().unwrap_or(path).chars().count();
        for query in queries {
            let Some(hit) = find(query, path) else {
                continue;
            };
            for pair in hit.positions.windows(2) {
                assert!(
                    pair[0] < pair[1],
                    "{query} on {path}: {:?} is not ascending",
                    hit.positions
                );
            }
            for &position in &hit.positions {
                assert!(
                    (position as usize) < name_chars,
                    "{query} on {path}: {position} is outside a {name_chars}-character name"
                );
            }
        }
    }
}

#[test]
fn a_multi_byte_name_is_indexed_by_character_and_not_by_byte() {
    // The failure this guards is silent: byte offsets through `é` underline
    // half a glyph, or land past the end of a short name entirely.
    assert_eq!(
        underlined("/nämé", "ünïcödé/nämé.rs").as_deref(),
        Some("nämé")
    );
}
