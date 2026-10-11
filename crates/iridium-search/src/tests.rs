//! Real filesystem and cancellation tests; results are observed through the queue.
use super::*;
use std::{
    fs, thread,
    time::{Duration, Instant},
};
fn collect(root: &std::path::Path, query: &Query, buffers: Vec<Buffer>) -> Vec<Event> {
    let mut job = Job::start(root.to_owned(), query, buffers).expect("start");
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut events = Vec::new();
    while job.is_running() {
        events.extend(job.poll().expect("worker"));
        assert!(Instant::now() < deadline, "worker timeout");
        thread::sleep(Duration::from_millis(1));
    }
    assert!(matches!(events.last(), Some(Event::Finished)));
    events
}
fn hits(events: &[Event]) -> Vec<&Match> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Match(hit) => Some(hit),
            _ => None,
        })
        .collect()
}
#[test]
fn open_buffers_replace_disk_and_new_buffers_are_searchable() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("root");
    let file = root.join("a.rs");
    fs::write(&file, "disk needle").expect("write");
    fs::write(root.join("b.rs"), "needle").expect("write");
    let buffers = vec![
        Buffer {
            id: 1,
            revision: 9,
            path: file,
            text: Arc::from("edited away"),
        },
        Buffer {
            id: 2,
            revision: 10,
            path: root.join("new.rs"),
            text: Arc::from("needle"),
        },
    ];
    let events = collect(
        &root,
        &Query {
            text: "needle".into(),
            ..Query::default()
        },
        buffers,
    );
    let hits = hits(&events);
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|hit| hit.origin
        == Origin::Buffer {
            id: 2,
            revision: 10
        }));
    assert!(
        hits.iter()
            .all(|hit| hit.path.file_name().expect("name") != "a.rs")
    );
}
#[test]
fn ignores_and_globs_apply_without_following_links() {
    let dir = tempfile::tempdir().expect("fixture");
    let root = dir.path().canonicalize().expect("root");
    fs::write(root.join(".gitignore"), "ignored.rs\n").expect("ignore");
    for name in ["yes.rs", "no.txt", "ignored.rs", ".hidden.rs"] {
        fs::write(root.join(name), "needle").expect("file");
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("yes.rs"), root.join("alias.rs")).expect("link");
    let events = collect(
        &root,
        &Query {
            text: "needle".into(),
            globs: vec!["*.rs".into(), "!ignored.rs".into(), "!.hidden.rs".into()],
            ..Query::default()
        },
        vec![],
    );
    assert_eq!(hits(&events).len(), 1);
    assert!(hits(&events)[0].path.ends_with("yes.rs"));
    let events = collect(
        &root,
        &Query {
            text: "needle".into(),
            ..Query::default()
        },
        vec![],
    );
    assert_eq!(hits(&events).len(), 2);
}
#[test]
fn unicode_regex_word_boundaries_bom_and_all_line_endings() {
    let dir = tempfile::tempdir().expect("fixture");
    fs::write(
        dir.path().join("mixed"),
        "\u{feff}é猫\r\nCAT catapult\rcat\n",
    )
    .expect("file");
    let events = collect(
        dir.path(),
        &Query {
            text: "cat|é猫".into(),
            regex: true,
            ignore_case: true,
            whole_word: true,
            ..Query::default()
        },
        vec![],
    );
    let matches = hits(&events);
    assert_eq!(matches.len(), 3);
    assert_eq!((matches[0].line, matches[0].range.clone()), (0, 0..5));
    assert_eq!(matches[1].line, 1);
    assert_eq!(matches[2].line, 2);
    let events = collect(
        dir.path(),
        &Query {
            text: "^$".into(),
            regex: true,
            ..Query::default()
        },
        vec![],
    );
    assert_eq!(hits(&events).len(), 1);
    assert_eq!(hits(&events)[0].line, 3);
}
#[test]
fn malformed_binary_and_long_lines_invalidate_prior_file_results() {
    let dir = tempfile::tempdir().expect("fixture");
    fs::write(dir.path().join("binary"), b"needle\n\0").expect("file");
    fs::write(dir.path().join("utf8"), b"needle\n\xff").expect("file");
    fs::write(
        dir.path().join("long"),
        format!("needle\n{}", "x".repeat(LINE_LIMIT + 1)),
    )
    .expect("file");
    let events = collect(
        dir.path(),
        &Query {
            text: "needle".into(),
            ..Query::default()
        },
        vec![],
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Invalidated { .. }))
            .count(),
        3
    );
}
#[test]
fn cancelling_a_full_queue_unblocks_the_worker_without_draining_results() {
    let dir = tempfile::tempdir().expect("fixture");
    fs::write(dir.path().join("many"), "match\n".repeat(100_000)).expect("file");
    let mut job = Job::start(
        dir.path().to_owned(),
        &Query {
            text: "match".into(),
            ..Query::default()
        },
        vec![],
    )
    .expect("start");
    thread::sleep(Duration::from_millis(30));
    let start = Instant::now();
    job.cancel();
    while job.is_running() {
        assert!(job.poll().expect("cancel").is_empty());
        assert!(start.elapsed() < Duration::from_secs(2));
        thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn invalid_queries_are_refused_and_result_limit_is_explicit() {
    let dir = tempfile::tempdir().expect("fixture");
    assert!(
        Job::start(
            dir.path().into(),
            &Query {
                text: "[".into(),
                regex: true,
                ..Query::default()
            },
            vec![]
        )
        .is_err()
    );
    fs::write(dir.path().join("many"), "x\n".repeat(MATCH_LIMIT + 10)).expect("file");
    let events = collect(
        dir.path(),
        &Query {
            text: "x".into(),
            ..Query::default()
        },
        vec![],
    );
    assert_eq!(hits(&events).len(), MATCH_LIMIT);
    assert!(events.iter().any(
        |e| matches!(e, Event::Warning(text) if text.contains("limit") || text.contains("10,000"))
    ));
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::Invalidated { .. }))
    );
}

#[test]
#[ignore = "manual 64 MiB project-search measurement"]
fn measured_large_file_streaming() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().expect("fixture");
    let mut file = fs::File::create(dir.path().join("large.log")).expect("create");
    let mut block = vec![b'x'; 64 * 1024];
    for index in (127..block.len()).step_by(128) {
        block[index] = b'\n';
    }
    block[..6].copy_from_slice(b"needle");
    file.write_all(&block).expect("first");
    block[..6].copy_from_slice(b"xxxxxx");
    for _ in 1..1023 {
        file.write_all(&block).expect("block");
    }
    block[..6].copy_from_slice(b"needle");
    file.write_all(&block).expect("last");
    drop(file);
    let start = Instant::now();
    let mut job = Job::start(
        dir.path().into(),
        &Query {
            text: "needle".into(),
            ..Query::default()
        },
        vec![],
    )
    .expect("job");
    let mut first = None;
    let mut count = 0;
    while job.is_running() {
        for event in job.poll().expect("poll") {
            match event {
                Event::Match(_) => {
                    first.get_or_insert_with(|| start.elapsed());
                    count += 1;
                },
                Event::Finished => {},
                event => panic!("unexpected result: {event:?}"),
            }
        }
        assert!(start.elapsed() < Duration::from_secs(30));
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(count, 2);
    writeln!(
        std::io::stderr().lock(),
        "PROJECT_SCAN_METRICS bytes=67108864 matches={count} first_us={} total_us={}",
        first.expect("first").as_micros(),
        start.elapsed().as_micros()
    )
    .expect("write benchmark result");
}

#[cfg(unix)]
#[test]
fn disk_stamp_detects_replacement_even_when_size_and_mtime_match() {
    let dir = tempfile::tempdir().expect("fixture");
    let path = dir.path().join("file");
    let replacement = dir.path().join("other");
    fs::write(&path, "cat").expect("file");
    let metadata = fs::metadata(&path).expect("metadata");
    let stamp = DiskStamp::from_metadata(&metadata).expect("stamp");
    fs::write(&replacement, "dog").expect("replacement");
    fs::File::options()
        .write(true)
        .open(&replacement)
        .expect("file")
        .set_times(std::fs::FileTimes::new().set_modified(metadata.modified().expect("mtime")))
        .expect("restore mtime");
    fs::rename(&replacement, &path).expect("replace");
    assert!(!stamp.agrees(&path).expect("compare"));
}

#[test]
fn result_columns_are_unicode_scalars_and_increment_across_matches() {
    let dir = tempfile::tempdir().expect("fixture");
    fs::write(dir.path().join("file"), "é cat 猫 cat").expect("file");
    let events = collect(
        dir.path(),
        &Query {
            text: "cat".into(),
            ..Query::default()
        },
        vec![],
    );
    assert_eq!(
        hits(&events)
            .iter()
            .map(|hit| hit.column)
            .collect::<Vec<_>>(),
        vec![2, 8]
    );
}
