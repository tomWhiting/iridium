//! The text seam on its own: no files, no editor, just lines in and entries
//! out.

use super::{LineState, Outline, OutlineEntry, ProjectionError};

/// One entry keyed by its own label.
fn entry(depth: usize, label: &str, branch: bool) -> OutlineEntry<String> {
    OutlineEntry {
        depth,
        label: label.to_owned(),
        branch,
        key: label.to_owned(),
    }
}

/// `src/` holding `main.rs` and `lib.rs`, then `README.md` at the top.
fn fixture() -> Outline<String> {
    Outline::project(vec![
        entry(0, "src", true),
        entry(1, "main.rs", false),
        entry(1, "lib.rs", false),
        entry(0, "README.md", false),
    ])
    .expect("every label fits on a line")
}

/// Text as the editor's lines.
fn lines(text: &str) -> Vec<String> {
    text.split('\n').map(str::to_owned).collect()
}

/// Each read line as `label <- loaded key`, with its parent's label.
fn summary(outline: &Outline<String>) -> Vec<String> {
    let reading = outline.read();
    reading
        .lines
        .iter()
        .map(|line| {
            let parent = line
                .parent
                .map_or("-", |parent| reading.lines[parent].label.as_str());
            let origin = line
                .origin
                .map_or("new", |entry| outline.entries()[entry].key.as_str());
            format!("{} in {parent} <- {origin}", line.label)
        })
        .collect()
}

/// The keys of every entry no line holds.
fn removed(outline: &Outline<String>) -> Vec<String> {
    outline
        .read()
        .removed
        .iter()
        .map(|&entry| outline.entries()[entry].key.clone())
        .collect()
}

#[test]
fn entries_are_written_one_indent_per_level_with_a_mark_on_branches() {
    assert_eq!(
        fixture().text(),
        "src/\n  main.rs\n  lib.rs\nREADME.md",
        "the projection is what the person edits, so it is pinned exactly"
    );
}

#[test]
fn untouched_text_reads_back_as_exactly_what_was_loaded() {
    let mut outline = fixture();
    assert!(!outline.track(lines(&outline.text())), "nothing changed");
    assert_eq!(
        summary(&outline),
        [
            "src in - <- src",
            "main.rs in src <- main.rs",
            "lib.rs in src <- lib.rs",
            "README.md in - <- README.md",
        ]
    );
    assert!(removed(&outline).is_empty());
    assert!(!outline.is_edited());
}

#[test]
fn editing_a_line_in_place_keeps_the_entry_it_was() {
    let mut outline = fixture();
    assert!(outline.track(lines("src/\n  main.rs\n  library.rs\nREADME.md")));
    assert_eq!(summary(&outline)[2], "library.rs in src <- lib.rs");
    assert!(removed(&outline).is_empty(), "a rename is not a delete");
    assert!(outline.is_edited());
}

#[test]
fn a_new_line_nests_under_whatever_it_is_indented_beneath() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\n  lib.rs\n  util.rs\nREADME.md"));
    assert_eq!(summary(&outline)[3], "util.rs in src <- new");

    outline.track(lines("src/\n  main.rs\n  lib.rs\nutil.rs\nREADME.md"));
    assert_eq!(
        summary(&outline)[3],
        "util.rs in - <- new",
        "dedenting it took it out of the folder"
    );
}

#[test]
fn a_deleted_line_is_an_entry_removed() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\nREADME.md"));
    assert_eq!(removed(&outline), ["lib.rs"]);
}

#[test]
fn a_line_cut_and_pasted_back_is_the_same_entry_and_not_a_new_one() {
    // Two edits: the cut, then the paste. Between them the entry has no line,
    // and the paste must not read as deleting the file and typing a new one.
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\nREADME.md"));
    outline.track(lines("src/\n  lib.rs\n  main.rs\nREADME.md"));
    assert_eq!(summary(&outline)[1], "lib.rs in src <- lib.rs");
    assert!(removed(&outline).is_empty());
    assert!(
        !outline.is_edited(),
        "the same entries under the same parents say nothing new"
    );
}

#[test]
fn a_line_moved_in_one_edit_keeps_its_entry() {
    let mut outline = fixture();
    outline.track(lines("src/\n  lib.rs\n  main.rs\nREADME.md"));
    assert_eq!(
        summary(&outline)[1..3],
        ["lib.rs in src <- lib.rs", "main.rs in src <- main.rs"]
    );
}

#[test]
fn a_line_emptied_and_retyped_is_a_rename_rather_than_a_delete() {
    let mut outline = fixture();
    for step in ["  lib.r", "  lib", "  ", "  c", "  core.rs"] {
        outline.track(lines(&format!("src/\n  main.rs\n{step}\nREADME.md")));
    }
    assert_eq!(summary(&outline)[2], "core.rs in src <- lib.rs");
    assert!(removed(&outline).is_empty());
}

#[test]
fn a_copy_of_a_line_that_still_has_its_own_is_a_new_line() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\n  main.rs\n  lib.rs\nREADME.md"));
    let summary = summary(&outline);
    assert_eq!(
        summary
            .iter()
            .filter(|line| line.ends_with("<- main.rs"))
            .count(),
        1,
        "one entry is one line: {summary:?}"
    );
    assert_eq!(
        summary
            .iter()
            .filter(|line| line.starts_with("main.rs") && line.ends_with("<- new"))
            .count(),
        1,
        "{summary:?}"
    );
}

#[test]
fn a_line_split_in_two_keeps_the_entry_on_one_half() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\n  li\n  b.rs\nREADME.md"));
    let summary = summary(&outline);
    assert_eq!(
        summary
            .iter()
            .filter(|line| line.ends_with("<- lib.rs"))
            .count(),
        1,
        "{summary:?}"
    );
    assert_eq!(
        summary
            .iter()
            .filter(|line| line.ends_with("<- new"))
            .count(),
        1,
        "{summary:?}"
    );
}

#[test]
fn several_lines_edited_at_once_each_keep_their_own_entry() {
    // What typing with a caret on every line does.
    let mut outline = fixture();
    outline.track(lines("src_/\n  main.rs_\n  lib.rs_\nREADME.md_"));
    assert_eq!(
        summary(&outline),
        [
            "src_ in - <- src",
            "main.rs_ in src_ <- main.rs",
            "lib.rs_ in src_ <- lib.rs",
            "README.md_ in - <- README.md",
        ]
    );
}

#[test]
fn undoing_back_to_the_loaded_text_is_clean_again() {
    let mut outline = fixture();
    let loaded = outline.text();
    outline.track(lines("src/\n  main.rs\nREADME.md"));
    assert!(outline.is_edited());
    outline.track(lines(&loaded));
    assert!(!outline.is_edited(), "{:?}", summary(&outline));
    assert!(removed(&outline).is_empty());
}

#[test]
fn blank_lines_say_nothing() {
    let mut outline = fixture();
    outline.track(lines("src/\n\n  main.rs\n  lib.rs\n   \nREADME.md\n"));
    assert!(!outline.is_edited());
    assert_eq!(summary(&outline)[1], "main.rs in src <- main.rs");
}

#[test]
fn a_tab_is_one_level_of_indentation() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\n  lib.rs\n\tmod.rs\nREADME.md"));
    assert_eq!(summary(&outline)[3], "mod.rs in src <- new");
}

#[test]
fn a_trailing_mark_makes_a_typed_line_a_branch() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\n  lib.rs\n  tests/\nREADME.md"));
    let reading = outline.read();
    assert!(reading.lines[3].branch);
    assert_eq!(reading.lines[3].label, "tests");
}

#[test]
fn a_label_that_cannot_be_one_line_is_refused_before_anything_is_written() {
    let refused = Outline::project(vec![entry(0, "two\nlines", false)]);
    assert_eq!(
        refused.err(),
        Some(ProjectionError::Unrepresentable {
            label: "two\nlines".to_owned()
        })
    );
    assert!(Outline::project(vec![entry(0, "", false)]).is_err());
}

#[test]
fn a_label_that_starts_with_a_space_survives_being_left_alone() {
    // Read from its text alone it would lose the space, which for a file is a
    // rename. An untouched line means what it was loaded as.
    let mut outline = Outline::project(vec![entry(0, "dir", true), entry(1, " padded", false)])
        .expect("a space is printable");
    outline.track(lines("dir/\n   padded\nnew"));
    assert_eq!(summary(&outline)[1], " padded in dir <-  padded");
    assert_eq!(outline.line_states()[1], LineState::Loaded);
}

#[test]
fn line_states_say_what_changed_on_each_line() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\n  core.rs\n\nnew.txt\nREADME.md"));
    assert_eq!(
        outline.line_states(),
        [
            LineState::Loaded,
            LineState::Loaded,
            LineState::Changed,
            LineState::Blank,
            LineState::Typed,
            LineState::Loaded,
        ]
    );
}

#[test]
fn an_entry_moved_under_another_parent_is_changed() {
    let mut outline = fixture();
    outline.track(lines("src/\n  main.rs\n  lib.rs\n  README.md"));
    assert_eq!(summary(&outline)[3], "README.md in src <- README.md");
    assert_eq!(outline.line_states()[3], LineState::Changed);
    assert!(outline.is_edited());
}

#[test]
fn an_empty_hierarchy_is_one_empty_line() {
    let mut outline: Outline<String> = Outline::project(Vec::new()).expect("nothing to refuse");
    assert_eq!(outline.text(), "");
    assert!(!outline.track(lines("")));
    assert!(outline.track(lines("a.txt")));
    assert_eq!(summary(&outline), ["a.txt in - <- new"]);
}

#[test]
fn a_rewrite_of_thousands_of_lines_still_pairs_each_with_its_own() {
    let entries: Vec<OutlineEntry<String>> = (0..3000)
        .map(|index| entry(0, &format!("file{index}.txt"), false))
        .collect();
    let mut outline = Outline::project(entries).expect("plain labels");
    let renamed: Vec<String> = (0..3000).map(|index| format!("file{index}.md")).collect();
    outline.track(renamed);
    let reading = outline.read();
    assert!(reading.removed.is_empty());
    assert!(
        reading
            .lines
            .iter()
            .enumerate()
            .all(|(index, line)| line.origin == Some(index)),
        "a rename of every line keeps every line its own entry"
    );
}

/// How each line compares with what was loaded, worked out from a whole
/// reading rather than kept up to date: the definition the kept states must
/// agree with.
fn states_from_reading(outline: &Outline<String>) -> Vec<LineState> {
    let reading = outline.read();
    let mut states = vec![LineState::Blank; outline.line_count()];
    for line in &reading.lines {
        states[line.line] = line.origin.map_or(LineState::Typed, |entry| {
            let parent = line.parent.map(|parent| reading.lines[parent].origin);
            let same_parent = match (parent, outline.loaded_parent(entry)) {
                (None, None) => true,
                (Some(above), Some(loaded)) => above == Some(loaded),
                _ => false,
            };
            if same_parent && outline.entries()[entry].label == line.label {
                LineState::Loaded
            } else {
                LineState::Changed
            }
        });
    }
    states
}

/// A tree deep enough that a re-indent can put a line under another folder,
/// with names repeated across folders when `repeated`.
fn wide_fixture(repeated: bool) -> Outline<String> {
    let name = |stem: &str, folder: usize| {
        if repeated {
            stem.to_owned()
        } else {
            format!("{stem}{folder}")
        }
    };
    let mut entries = Vec::new();
    for folder in 0..6 {
        entries.push(entry(0, &format!("dir{folder}"), true));
        entries.push(entry(1, &name("inner", folder), true));
        for file in 0..4 {
            entries.push(entry(2, &name(&format!("f{file}.rs"), folder), false));
        }
        for file in 0..3 {
            entries.push(entry(1, &name(&format!("g{file}.rs"), folder), false));
        }
    }
    entries.push(entry(0, "top.md", false));
    Outline::project(entries).expect("plain labels")
}

/// One random edit to `text`: the lines it replaced and what replaced them.
fn random_edit(
    text: &[String],
    clipboard: &mut Vec<String>,
    next: &mut impl FnMut(usize) -> usize,
) -> (usize, usize, Vec<String>) {
    let at = next(text.len());
    match next(9) {
        // Type a character at the end of a line.
        0 => (at, at + 1, vec![format!("{}x", text[at])]),
        // Take a character off the end of a line.
        1 => {
            let mut line = text[at].clone();
            line.pop();
            (at, at + 1, vec![line])
        },
        // Cut a run of lines; the text always keeps one line.
        2 => {
            let end = (at + 1 + next(3)).min(text.len());
            *clipboard = text[at..end].to_vec();
            let left = if end - at == text.len() {
                vec![String::new()]
            } else {
                Vec::new()
            };
            (at, end, left)
        },
        // Paste the cut lines above a line.
        3 => {
            let mut pasted = clipboard.clone();
            pasted.push(text[at].clone());
            (at, at + 1, pasted)
        },
        // Indent or dedent a line.
        4 => {
            let line = &text[at];
            let changed = if next(2) == 0 {
                format!("  {line}")
            } else {
                line.strip_prefix("  ").unwrap_or(line).to_owned()
            };
            (at, at + 1, vec![changed])
        },
        // Empty a line.
        5 => (at, at + 1, vec![String::new()]),
        // Split a line in two.
        6 => {
            let line = &text[at];
            let half = line.char_indices().nth(line.chars().count() / 2);
            let cut = half.map_or(line.len(), |(offset, _)| offset);
            let parts = vec![line[..cut].to_owned(), format!("  {}", &line[cut..])];
            (at, at + 1, parts)
        },
        // Join a line with the next.
        7 if at + 1 < text.len() => {
            let joined = format!("{}{}", text[at], text[at + 1].trim_start());
            (at, at + 2, vec![joined])
        },
        // Type an existing name again somewhere else, maybe deeper.
        _ => {
            let indent = if next(2) == 0 { "  " } else { "" };
            (at, at, vec![format!("{indent}{}", text[next(text.len())])])
        },
    }
}

/// Whether two non-blank lines of `text` are the same, or two are blank.
///
/// Then which of the two an edit kept is a matter of where the comparison
/// started: a span and the whole text can both be right and disagree.
fn has_twins(text: &[String]) -> bool {
    let mut seen = std::collections::HashSet::new();
    text.iter().any(|line| {
        !seen.insert(if line.trim().is_empty() {
            ""
        } else {
            line.as_str()
        })
    })
}

/// Edits followed one span at a time read exactly as the same edits followed
/// by rereading the whole text each time, and the states and counts kept
/// along the way are always the ones a whole reading gives.
#[test]
fn following_spans_reads_the_same_as_rereading_everything() {
    let mut seed: u64 = 0x1234_5678_9abc_def1;
    let mut next = |bound: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
    };
    let mut compared = 0;
    for round in 0..80 {
        let repeated = round % 2 == 0;
        let mut spans = wide_fixture(repeated);
        let mut whole = wide_fixture(repeated);
        let mut text = lines(&spans.text());
        let mut clipboard = Vec::new();
        for step in 0..60 {
            // Without twins every step can be compared with a whole reading.
            let (start, old_end, new) = loop {
                let (start, mut old_end, mut new) = random_edit(&text, &mut clipboard, &mut next);
                // The editor's spans are often wider than the change: one
                // more untouched line, carried through.
                if next(2) == 0 && old_end < text.len() {
                    new.push(text[old_end].clone());
                    old_end += 1;
                }
                let mut after = text.clone();
                after.splice(start..old_end, new.iter().cloned());
                if repeated || !has_twins(&after) {
                    break (start, old_end, new);
                }
            };
            text.splice(start..old_end, new.iter().cloned());
            spans.edit(start, old_end, new);
            whole.track(text.clone());

            let context = format!("round {round}, step {step}");
            let reading = spans.read();
            assert_eq!(
                spans.line_states(),
                states_from_reading(&spans),
                "{context}"
            );
            if has_twins(&text) {
                whole = spans.clone();
            } else {
                assert_eq!(reading, whole.read(), "{context}");
                assert_eq!(spans.line_states(), whole.line_states(), "{context}");
                compared += 1;
            }
            assert_eq!(spans.removed_count(), reading.removed.len(), "{context}");
            let edited = !reading.removed.is_empty()
                || spans
                    .line_states()
                    .iter()
                    .any(|state| matches!(state, LineState::Changed | LineState::Typed));
            assert_eq!(spans.is_edited(), edited, "{context}");
        }
    }
    assert!(
        compared > 2000,
        "only {compared} steps compared with a whole reading"
    );
}

/// Two thousand top-level files, for edits far apart.
fn long_fixture() -> Outline<String> {
    let entries: Vec<OutlineEntry<String>> = (0..2000)
        .map(|index| entry(0, &format!("file{index}.txt"), false))
        .collect();
    Outline::project(entries).expect("plain labels")
}

#[test]
fn a_line_cut_and_pasted_far_away_is_still_its_entry() {
    let mut outline = long_fixture();
    outline.edit(3, 4, Vec::new());
    assert_eq!(outline.removed_count(), 1);
    outline.edit(1998, 1998, vec!["file3.txt".to_owned()]);
    assert_eq!(outline.removed_count(), 0, "the paste is the entry again");
    assert!(!outline.is_edited(), "the same entry under the same parent");
}

#[test]
fn a_copy_pasted_far_away_becomes_the_entry_when_the_original_goes() {
    let mut outline = long_fixture();
    outline.edit(1990, 1990, vec!["file3.txt".to_owned()]);
    assert_eq!(
        outline.line_state(1990),
        LineState::Typed,
        "a copy, for now"
    );
    outline.edit(3, 4, Vec::new());
    assert_eq!(outline.line_state(1989), LineState::Loaded);
    assert_eq!(outline.removed_count(), 0);
    assert!(!outline.is_edited());
}

#[test]
fn a_line_emptied_then_claimed_elsewhere_types_as_new_when_refilled() {
    let mut outline = fixture();
    // Empty lib.rs's line, then type lib.rs elsewhere: that line takes the
    // entry, so text typed into the emptied line later is a new file.
    outline.edit(2, 3, vec!["  ".to_owned()]);
    outline.edit(4, 4, vec!["  lib.rs".to_owned()]);
    assert_eq!(outline.removed_count(), 0);
    outline.edit(2, 3, vec!["  other.rs".to_owned()]);
    assert_eq!(outline.line_state(2), LineState::Typed);
    assert_eq!(outline.removed_count(), 0);
}
