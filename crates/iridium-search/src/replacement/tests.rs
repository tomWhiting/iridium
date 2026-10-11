use super::*;
use crate::Origin;
use std::{path::PathBuf, sync::Arc};
fn hit(text: &str, range: std::ops::Range<usize>) -> Match {
    Match {
        path: Arc::new(PathBuf::from("/project/file")),
        origin: Origin::Buffer { id: 1, revision: 4 },
        line: 0,
        column: text
            .get(..range.start)
            .map_or(0, |prefix| prefix.chars().count()),
        range,
        line_text: Arc::from(text),
    }
}
#[test]
fn captures_match_regex_expansion_and_literals_keep_dollars() {
    let query = Query {
        text: r"(?P<name>\pL+)=(\d+)".into(),
        regex: true,
        ..Query::default()
    };
    let text = "猫=12 é=34";
    let hits = vec![hit(text, 7..12), hit(text, 0..6)];
    for template in [
        "${name}:$2",
        "$0 $$ $missing",
        "${2}_$name",
        "${broken",
        "$",
        "\n$1\r\n",
    ] {
        let result = preview_replacements(&query, &hits, template, || false).expect("preview");
        let regex = crate::compile(&query).expect("regex");
        for entry in result {
            let captures = regex
                .captures_at(text, entry.source.range.start)
                .expect("capture");
            let mut expected = String::new();
            captures.expand(template, &mut expected);
            assert_eq!(entry.text(), expected);
        }
    }
    let result = preview_replacements(
        &Query {
            text: "猫".into(),
            ..Query::default()
        },
        &[hit(text, 0..3)],
        "$1",
        || false,
    )
    .expect("literal");
    assert_eq!(result[0].text(), "$1");
}
#[test]
fn stale_forged_overlapping_and_mixed_snapshots_are_rejected() {
    let query = Query {
        text: "cat".into(),
        ..Query::default()
    };
    for hits in [
        vec![hit("cat", 1..3)],
        vec![hit("cat", 0..3), hit("cat", 0..3)],
        vec![hit("猫", 0..1)],
        vec![hit("cat cat", 0..3), hit("cat dog", 4..7)],
    ] {
        assert!(preview_replacements(&query, &hits, "dog", || false).is_err());
    }
    assert!(preview_replacements(&query, &[hit("cat", 0..3)], "dog", || true).is_err());
}
#[test]
fn zero_width_matches_follow_the_search_iterator() {
    let query = Query {
        text: "a*".into(),
        regex: true,
        ..Query::default()
    };
    let valid = vec![hit("a", 0..1)];
    assert_eq!(
        preview_replacements(&query, &valid, "x", || false).expect("preview")[0].original(),
        "a"
    );
    assert!(preview_replacements(&query, &[hit("a", 1..1)], "x", || false).is_err());
    let query = Query {
        text: "^|$".into(),
        regex: true,
        ..Query::default()
    };
    assert_eq!(
        preview_replacements(&query, &[hit("猫", 0..0), hit("猫", 3..3)], "x", || false)
            .expect("preview")
            .len(),
        2
    );
}
#[test]
fn amplified_captures_are_sized_before_allocating_output() {
    let query = Query {
        text: "(x+)".into(),
        regex: true,
        ..Query::default()
    };
    let text = "x".repeat(256 * 1024);
    assert!(
        preview_replacements(&query, &[hit(&text, 0..text.len())], "$1$1$1$1$1", || false).is_err()
    );
    let allowed = preview_replacements(&query, &[hit(&text, 0..text.len())], "$1$1$1$1", || false)
        .expect("exact limit");
    assert_eq!(allowed[0].text.len(), LINE_LIMIT);
}
