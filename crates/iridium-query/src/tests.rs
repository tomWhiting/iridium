use super::*;
fn query(filter: &str) -> Query {
    Query {
        filter: filter.into(),
        format: Format::Auto,
        target: Target::Values,
    }
}
fn selected<'a>(source: &'a str, filter: &str) -> Result<Vec<&'a str>, String> {
    select(source, &query(filter))?
        .iter()
        .map(|range| {
            source
                .get(range.start..range.end)
                .ok_or_else(|| "invalid range".into())
        })
        .collect()
}

#[test]
fn filtered_paths_select_original_tokens_not_equal_text_elsewhere() {
    let source = r#"{"items":[{"status":"pending","email":"same"},{"status":"done","email":"same"}],"unrelated":"same"}"#;
    let ranges = select(
        source,
        &query(r#".items[] | select(.status == "pending") | .email"#),
    )
    .expect("query");
    assert_eq!(ranges.len(), 1);
    assert_eq!(
        ranges[0].start,
        source.find(r#""same""#).expect("first email")
    );
    assert_eq!(&source[ranges[0].start..ranges[0].end], r#""same""#);
}

#[test]
fn jsonl_records_preserve_offsets_unicode_crlf_and_numeric_spelling() {
    let source = "{\"name\":\"漢字\",\"v\":1e2}\r\n{\"name\":\"other\",\"v\":2.00}\r\n";
    assert_eq!(selected(source, ".v").expect("values"), vec!["1e2", "2.00"]);
    assert_eq!(
        selected(source, r#"select(.name == "漢字") | .name"#).expect("unicode"),
        vec!["\"漢字\""]
    );
}

#[test]
fn key_targets_resolve_escaped_keys_without_reserializing() {
    let source = r#"{"na\u006de":"yes","count":1}"#;
    let mut filter = query(r#".[] | select(type == "string")"#);
    filter.target = Target::Keys;
    let ranges = select(source, &filter).expect("keys");
    assert_eq!(&source[ranges[0].start..ranges[0].end], r#""na\u006de""#);
}

#[test]
fn errors_never_return_partial_selectable_results() {
    for source in [
        "{\"v\":1}\nnot-json\n",
        "{\"v\":1,\"v\":2}",
        "{\"v\":1,\"\\u0076\":2}",
    ] {
        assert!(select(source, &query(".v")).is_err());
    }
    assert!(select("[1,2]", &query("map(.+1)")).is_err());
    assert!(select("{}", &query(".missing")).is_err());
    assert!(select("{}", &query("[")).is_err());
    assert!(select("{}", &query("env")).is_err());
}

#[test]
fn zero_matches_are_valid_and_repeated_paths_are_deduplicated() {
    assert!(
        select("[1,2]", &query(".[] | select(. > 5)"))
            .expect("none")
            .is_empty()
    );
    assert_eq!(
        selected("[11,22]", ".[1], .[1]").expect("dedup"),
        vec!["22"]
    );
}
