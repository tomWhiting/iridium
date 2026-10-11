use super::*;

#[test]
fn whole_word_never_matches_a_partial_accented_word_or_emoji() {
    let document = Document::new("cafe\u{301} cafe 👩🏽‍💻 👩");
    for regex in [false, true] {
        for case_sensitive in [false, true] {
            let options = SearchOptions {
                whole_word: true,
                regex,
                case_sensitive,
            };
            let mut search = SearchState::new();
            search.find_all("cafe", &options, &document).unwrap();
            assert_eq!(
                search.matches,
                vec![Range::new(Position::new(0, 6), Position::new(0, 10))]
            );
            search.find_all("👩", &options, &document).unwrap();
            assert_eq!(
                search.matches,
                vec![Range::new(Position::new(0, 16), Position::new(0, 17))]
            );
            search.find_all("cafe\u{301}", &options, &document).unwrap();
            assert_eq!(search.match_count(), 1);
        }
    }
}

#[test]
fn combining_neighbour_keeps_the_following_letters_in_the_same_word() {
    let document = Document::new("e\u{301}x x");
    let mut search = SearchState::new();
    search
        .find_all("x", &SearchOptions::whole_word(), &document)
        .unwrap();
    assert_eq!(
        search.matches,
        vec![Range::new(Position::new(0, 4), Position::new(0, 5))]
    );
}

#[test]
fn whole_word_replacement_uses_the_same_cluster_boundaries_and_undo() {
    let mut document = Document::new("cafe\u{301} cafe");
    let mut cursor = crate::CursorState::at(Position::zero());
    let mut search = SearchState::new();
    let options = SearchOptions {
        whole_word: true,
        regex: true,
        case_sensitive: true,
    };
    search.find_all("(cafe)", &options, &document).unwrap();
    let (command, result) =
        crate::search::replace_all(&search, "${1}!", &document, &cursor).unwrap();
    assert_eq!(result.count, 1);
    command.apply(&mut document, &mut cursor).unwrap();
    assert_eq!(document.text(), "cafe\u{301} cafe!");
    command.inverse().apply(&mut document, &mut cursor).unwrap();
    assert_eq!(document.text(), "cafe\u{301} cafe");
}

#[test]
fn whole_word_rejects_invalid_offsets() {
    for (start, end) in [(2, 1), (0, 8), (1, 2)] {
        assert!(!SearchState::is_word_boundary("é", start, end));
    }
}
