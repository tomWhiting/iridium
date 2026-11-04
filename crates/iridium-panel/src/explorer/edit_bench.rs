//! Timing harness for one keystroke in a listing being edited, run by hand.
//!
//! `#[ignore]`d because it prints measurements rather than asserting them: a
//! timing threshold in CI is a flake. Run it in release:
//! `cargo nextest run -p iridium-panel --release --run-ignored only --no-capture edit_bench`.
//!
//! The bar is that the cost per key is flat across listing sizes, because a
//! keystroke must cost what the edit touched, not what the listing holds.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use iridium_editor::theme::Theme;
use iridium_editor::{KeyCode, KeyEvent};
use iridium_file::test_support::TempDir;

use super::buffer::SourceRow;
use super::session::{EditLayers, Editing};
use super::tests::support::{FIT, opened, press};
use crate::explorer::FileExplorer;

/// Files under each synthetic folder.
const PER_FOLDER: usize = 49;

/// A listing of `entries` rows under the root: folders of [`PER_FOLDER`]
/// files, the shape a large expanded tree has.
fn source(entries: usize) -> Vec<SourceRow> {
    let root = PathBuf::from("/bench");
    let mut rows = vec![SourceRow {
        depth: 0,
        name: "bench".to_owned(),
        path: root.clone(),
        directory: true,
    }];
    let mut folder = root.clone();
    for index in 0..entries {
        if index % (PER_FOLDER + 1) == 0 {
            let name = format!("folder_{index:06}");
            folder = root.join(&name);
            rows.push(SourceRow {
                depth: 1,
                name,
                path: folder.clone(),
                directory: true,
            });
        } else {
            let name = format!("file_{index:06}.rs");
            rows.push(SourceRow {
                depth: 2,
                path: folder.join(&name),
                name,
                directory: false,
            });
        }
    }
    rows
}

/// A panel editing a listing of `entries` rows, caret mid-listing, drawn once.
fn session(directory: &TempDir, entries: usize) -> FileExplorer {
    let mut explorer = opened(directory);
    let (editing, problems) = Editing::new(&source(entries), entries / 2, &EditLayers::default())
        .expect("the synthetic rows project");
    assert!(problems.is_empty(), "{problems:?}");
    assert!(explorer.mode.begin_edit(editing));
    explorer.body(&Theme::dark(), FIT);
    explorer
}

/// Presses each of `keys` and draws a frame after it, returning the mean.
fn drive(explorer: &mut FileExplorer, keys: &[KeyEvent]) -> Duration {
    let theme = Theme::dark();
    let mut total = Duration::ZERO;
    for key in keys {
        let started = Instant::now();
        explorer.handle_key(key);
        let body = explorer.body(&theme, FIT);
        total += started.elapsed();
        assert!(!body.rows.is_empty());
    }
    total / count(keys)
}

/// The same keys again, each split into its three parts: the kernel editor's
/// handling, the listing following it, and the frame.
fn phases(explorer: &mut FileExplorer, keys: &[KeyEvent]) -> [Duration; 3] {
    let theme = Theme::dark();
    let mut spent = [Duration::ZERO; 3];
    for key in keys {
        let Some(editing) = explorer.mode.editing_mut() else {
            break;
        };
        let started = Instant::now();
        editing.editor.handle_key(key);
        let handled = Instant::now();
        editing.sync();
        let followed = Instant::now();
        explorer.body(&theme, FIT);
        let drawn = Instant::now();
        spent[0] += handled - started;
        spent[1] += followed - handled;
        spent[2] += drawn - followed;
    }
    spent.map(|part| part / count(keys))
}

/// `keys.len()` as a divisor.
fn count(keys: &[KeyEvent]) -> u32 {
    u32::try_from(keys.len()).unwrap_or(u32::MAX).max(1)
}

/// Microseconds, for printing.
fn us(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e6
}

#[test]
#[ignore = "a measurement, run by hand in release"]
fn edit_bench_keystroke_to_frame() {
    for entries in [100, 10_000, 100_000] {
        let directory = TempDir::new("edit-bench");
        let mut explorer = session(&directory, entries);

        let typed = vec![press(KeyCode::Char('x')); 500];
        let typing = drive(&mut explorer, &typed);
        let [kernel, follow, frame] = phases(&mut explorer, &typed);

        // Each pair splits a line and joins it again: the line count changes.
        let split: Vec<KeyEvent> = [press(KeyCode::Enter), press(KeyCode::Backspace)]
            .into_iter()
            .cycle()
            .take(200)
            .collect();
        let splitting = drive(&mut explorer, &split);

        println!(
            "{entries:>7} entries: typed char {:>6.1} us/frame (kernel editor {:>5.1}, \
             listing {:>4.1}, frame {:>4.1}); enter/backspace {:>6.1} us/frame",
            us(typing),
            us(kernel),
            us(follow),
            us(frame),
            us(splitting),
        );
    }
}
