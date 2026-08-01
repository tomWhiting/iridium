//! Attribution harness for the per-keystroke cost with a language set.
//!
//! Temporary diagnostic, not a shipped artefact. It splits the measured
//! keystroke cost into the three O(document) components that
//! `Editor::apply_command_internal` pays on every content change, so the fix
//! targets whichever actually dominates rather than the first one spotted.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::hint::black_box;
use std::io::{self, Write as _};
use std::time::Instant;

use iridium_editor::document::compute_edit_span;
use iridium_editor::editor::{Editor, EditorConfig, FoldState, SyntaxState};
use iridium_editor::{Command, Language, Position};

/// One JSON record per line, which is the shape of the files this editor is
/// actually used on.
fn json_document(lines: usize) -> String {
    let mut out = String::from("[\n");
    for i in 0..lines {
        // `write!` into the buffer rather than `push_str(&format!(..))`, which
        // would allocate a throwaway `String` per line. Writing to a `String`
        // is infallible, so the result is discarded deliberately.
        let _ = writeln!(
            out,
            "  {{ \"id\": {i}, \"name\": \"record-{i}\", \"active\": true, \"score\": {}.5 }},",
            i % 97
        );
    }
    out.push_str("  null\n]\n");
    out
}

/// Median of a handful of runs, to keep one scheduling hiccup from being the
/// headline number.
fn median_micros(mut samples: Vec<f64>) -> f64 {
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    samples[samples.len() / 2]
}

fn time_it<F: FnMut()>(runs: usize, mut f: F) -> f64 {
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        f();
        samples.push(start.elapsed().as_secs_f64() * 1e6);
    }
    median_micros(samples)
}

/// Brings `syntax` and `folds` up to date with `editor`'s document.
///
/// The same three steps `EditorState::refresh_syntax` takes, spelled out here
/// because the harness needs a clock between the second and the third.
fn prime_folds(syntax: &mut SyntaxState, folds: &mut FoldState, editor: &Editor) {
    let document = &editor.state().document;
    syntax.sync(document);
    let delta = syntax.take_delta();
    if let Some(tree) = syntax.tree() {
        folds.update_regions(tree, &delta, || Cow::Owned(document.text()));
    }
}

fn main() -> io::Result<()> {
    // One locked handle for the whole run rather than `println!`, which
    // re-locks stdout on every call.
    let stdout = io::stdout();
    let mut out = stdout.lock();

    writeln!(
        out,
        "{:>8} {:>12} {:>12} {:>12} {:>12} {:>12}",
        "lines", "keystroke", "text()", "sync", "folds", "unattributed"
    )?;

    for lines in [10_000_usize, 50_000, 100_000] {
        let content = json_document(lines);

        // (1) The real thing: a keystroke through the public command surface.
        let mut editor = Editor::new(EditorConfig::default());
        editor.set_content(&content);
        editor.set_language(Language::Json);
        let mut col = 3usize;
        let keystroke = time_it(9, || {
            editor.apply_command(Command::Insert {
                position: Position::new(1, col),
                text: "x".to_string(),
            });
            col += 1;
        });

        // (2) The rope -> String materialisation, which the keystroke path pays
        //     twice: once inside `sync`, once again in `refresh_syntax`.
        let doc_editor = Editor::new(EditorConfig::default());
        let mut probe = doc_editor;
        probe.set_content(&content);
        let text_once = time_it(9, || {
            black_box(probe.state().document.text());
        });

        // (3) The parse alone, incremental, including its own `text()` call.
        let mut syntax = SyntaxState::new();
        syntax.set_language(Language::Json);
        let mut doc_editor2 = Editor::new(EditorConfig::default());
        doc_editor2.set_content(&content);
        let document = &doc_editor2.state().document;
        black_box(syntax.sync(document));

        let mut sync_editor = Editor::new(EditorConfig::default());
        sync_editor.set_content(&content);
        let mut syntax2 = SyntaxState::new();
        syntax2.set_language(Language::Json);
        black_box(syntax2.sync(&sync_editor.state().document));
        let mut col2 = 3usize;
        let sync_cost = time_it(9, || {
            let cmd = Command::Insert {
                position: Position::new(1, col2),
                text: "y".to_string(),
            };
            col2 += 1;
            let span = compute_edit_span(&sync_editor.state().document, &cmd)
                .ok()
                .flatten();
            sync_editor.apply_command(cmd);
            if let Some(span) = span {
                syntax2.note_edit(&sync_editor.state().document, &span);
            }
            black_box(syntax2.sync(&sync_editor.state().document));
        });

        // (4) Fold refreshing alone, as a keystroke actually pays for it.
        //
        // Not a full recompute over a finished tree: that is what the editor
        // used to do and the whole point of the fix is that it no longer does.
        // Each iteration applies a real edit, reports it, reparses, and times
        // *only* the fold refresh that follows — the same three calls
        // `EditorState::refresh_syntax` makes, with the clock around the last
        // one.
        let mut fold_editor = Editor::new(EditorConfig::default());
        fold_editor.set_content(&content);
        let mut syntax3 = SyntaxState::new();
        syntax3.set_language(Language::Json);
        let mut folds = FoldState::for_language(Language::Json);
        prime_folds(&mut syntax3, &mut folds, &fold_editor);

        let mut nodes_before = folds.fold_nodes_visited();
        let mut fold_samples = Vec::with_capacity(9);
        let mut fold_nodes_per_keystroke = 0;
        for step in 0..9 {
            let cmd = Command::Insert {
                position: Position::new(1, 3 + step),
                text: "z".to_string(),
            };
            let span = compute_edit_span(&fold_editor.state().document, &cmd)
                .ok()
                .flatten();
            fold_editor.apply_command(cmd);
            let document = &fold_editor.state().document;
            if let Some(span) = span {
                syntax3.note_edit(document, &span);
            }
            syntax3.sync(document);
            let delta = syntax3.take_delta();
            let Some(tree) = syntax3.tree() else {
                continue;
            };
            let start = Instant::now();
            black_box(folds.update_regions(tree, &delta, || Cow::Owned(document.text())));
            fold_samples.push(start.elapsed().as_secs_f64() * 1e6);

            let nodes_after = folds.fold_nodes_visited();
            fold_nodes_per_keystroke = nodes_after - nodes_before;
            nodes_before = nodes_after;
        }
        let fold_cost = median_micros(fold_samples);

        let attributed = text_once + sync_cost + fold_cost;
        writeln!(
            out,
            "{lines:>8} {keystroke:>11.0}µ {text_once:>11.0}µ {sync_cost:>11.0}µ \
             {fold_cost:>11.0}µ {:>11.0}µ",
            keystroke - attributed
        )?;
        writeln!(
            out,
            "         parses: full={} incremental={}   regions={}   \
             fold nodes/keystroke={fold_nodes_per_keystroke}",
            syntax2.full_parses(),
            syntax2.incremental_parses(),
            folds.regions().len()
        )?;
    }

    Ok(())
}
