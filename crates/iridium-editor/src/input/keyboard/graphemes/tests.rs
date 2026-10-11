use super::*;

#[test]
fn chunked_boundaries_match_contiguous_unicode_at_every_scalar() {
    // Mixed emoji and Indic text exercises internal chunk boundaries; the long
    // trailing combining sequence requires context across multiple chunks.
    let text = format!(
        "{}{}{}\r\n{}",
        "a".repeat(1023),
        "e\u{301}".repeat(700),
        "🇦🇺👩🏽‍💻क्‍ष".repeat(180),
        "\u{301}".repeat(2500)
    );
    let rope = Rope::from_str(&text);
    assert!(rope.chunks().count() > 5);
    let boundaries: Vec<_> = text
        .grapheme_indices(true)
        .map(|(index, _)| index)
        .chain(std::iter::once(text.len()))
        .collect();
    for offset in text
        .char_indices()
        .map(|(offset, _)| offset)
        .chain(std::iter::once(text.len()))
    {
        let previous = boundaries
            .iter()
            .copied()
            .take_while(|&x| x < offset)
            .last()
            .unwrap_or(0);
        let next = boundaries
            .iter()
            .copied()
            .find(|&x| x > offset)
            .unwrap_or(offset);
        let floor = boundaries
            .iter()
            .copied()
            .take_while(|&x| x <= offset)
            .last()
            .unwrap_or(0);
        assert_eq!(
            boundary(&rope, offset, Boundary::Previous),
            Ok(previous),
            "previous at {offset}"
        );
        assert_eq!(
            boundary(&rope, offset, Boundary::Next),
            Ok(next),
            "next at {offset}"
        );
        assert_eq!(
            boundary(&rope, offset, Boundary::Floor),
            Ok(floor),
            "floor at {offset}"
        );
        let ceiling = boundaries
            .iter()
            .copied()
            .find(|&x| x >= offset)
            .unwrap_or(offset);
        assert_eq!(
            boundary(&rope, offset, Boundary::Ceiling),
            Ok(ceiling),
            "ceiling at {offset}"
        );
    }
}

#[test]
fn line_endings_are_single_steps_and_edges_are_stable() {
    let document = Document::new("a\r\nb\rc\n");
    for (from, to) in [
        (Position::new(0, 1), Position::new(1, 0)),
        (Position::new(1, 1), Position::new(2, 0)),
        (Position::new(2, 1), Position::new(3, 0)),
    ] {
        assert_eq!(move_to(&document, from, Boundary::Next), to);
        assert_eq!(move_to(&document, to, Boundary::Previous), from);
    }
    let empty = Document::new("");
    for direction in [Boundary::Previous, Boundary::Next, Boundary::Floor] {
        assert_eq!(
            move_to(&empty, Position::new(0, 0), direction),
            Position::new(0, 0)
        );
    }
}
