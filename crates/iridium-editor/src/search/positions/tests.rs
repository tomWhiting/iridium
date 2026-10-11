//! Offset projection agrees with the document's Unicode and newline model.
use super::*;
use crate::Document;

#[test]
fn every_boundary_matches_document_positions_including_crlf() -> Result<(), String> {
    for text in [
        "",
        "abc",
        "é🙂\r\n猫\r𐀀\n\n",
        "\r\r\n\r",
        "e\u{301}\u{2028}x\t\r\n",
    ] {
        let document = Document::new(text);
        let mut stream = MatchPositions::new(text);
        for offset in text
            .char_indices()
            .map(|(offset, _)| offset)
            .chain([text.len()])
        {
            let range = stream.range(offset, offset)?;
            assert_eq!(
                Some(range.start),
                document.offset_to_position(offset),
                "{text:?} at {offset}"
            );
            assert_eq!(range.start, range.end);
        }
    }
    Ok(())
}

#[test]
fn overlapping_multiline_ranges_keep_independent_start_and_end_progress() -> Result<(), String> {
    let text = "é\r\né\r\né\r\né";
    let doc = Document::new(text);
    let mut positions = MatchPositions::new(text);
    for (start, end) in [(0, 6), (4, 10), (8, 14)] {
        let range = positions.range(start, end)?;
        assert_eq!(Some(range.start), doc.offset_to_position(start));
        assert_eq!(Some(range.end), doc.offset_to_position(end));
        assert_eq!(doc.slice(range), "é\r\né");
    }
    Ok(())
}

#[test]
fn invalid_or_unordered_offsets_are_reported() -> Result<(), String> {
    let mut positions = MatchPositions::new("éabc");
    assert!(positions.range(1, 2).is_err());
    assert!(positions.range(0, 6).is_err());
    assert!(positions.range(3, 2).is_err());
    let mut positions = MatchPositions::new("éabc");
    positions.range(2, 4)?;
    assert!(positions.range(0, 3).is_err());
    Ok(())
}

#[test]
fn dense_results_preserve_unicode_ranges_for_all_matchers() -> Result<(), String> {
    use crate::search::{SearchOptions, SearchState};
    let text = "é\u{212a}\r\néK\r".repeat(1_000);
    let doc = Document::new(&text);
    for options in [
        SearchOptions::default(),
        SearchOptions::case_sensitive(),
        SearchOptions::regex_mode(),
    ] {
        let mut search = SearchState::new();
        search.find_all("é", &options, &doc)?;
        assert_eq!(search.match_count(), 2_000);
        for (range, (offset, _)) in search.matches.iter().zip(text.match_indices('é')) {
            assert_eq!(Some(range.start), doc.offset_to_position(offset));
            assert_eq!(Some(range.end), doc.offset_to_position(offset + 2));
            assert_eq!(doc.slice(*range), "é");
        }
    }
    Ok(())
}
