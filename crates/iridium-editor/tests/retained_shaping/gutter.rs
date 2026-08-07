//! The miss side: folds, custom gutter lines, and the digit rollover.

use iridium_editor::{Editor, KeyCode, Language, Position};

use crate::harness::{
    ActiveLanguageNoSpans, cold_pixels, compose, compositor, editor_over, gpu, pixels, press,
    target,
};
use crate::support::gpu::{HEIGHT, WIDTH};
use crate::support::pixels::assert_same_frame;

/// The sharpest trap in the input set: folding hides lines without moving
/// the document revision. The fold generation must carry the miss, and the
/// folded frame must match a cold compose of the folded state.
#[test]
fn a_fold_toggle_misses_despite_an_unchanged_document_revision() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content(
        "fn folded() {\n    one();\n    two();\n    three();\n}\nfn after() {\n    tail();\n}\n",
    );
    editor.set_language(Language::Rust);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);

    let revision_before = editor.state().document.revision();
    assert!(editor.fold_at(0), "the function must be foldable");
    assert_eq!(
        editor.state().document.revision(),
        revision_before,
        "folding must not touch the document revision — that is the trap"
    );

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the fold is a miss");
    assert_eq!(
        warm.lines_reshaped(),
        0,
        "a fold change is a full rebuild — the line↔buffer correspondence moved"
    );
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the fold must be visible");

    let mut cold_editor = Editor::with_defaults();
    cold_editor.set_content(
        "fn folded() {\n    one();\n    two();\n    three();\n}\nfn after() {\n    tail();\n}\n",
    );
    cold_editor.set_language(Language::Rust);
    assert!(cold_editor.fold_at(0));
    let cold = cold_pixels(
        &gpu,
        &tgt,
        &cold_editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |_| {},
    );
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the folded frame must be byte-identical",
    );
}

/// Custom gutter text changes the gutter's text and (through its measured
/// width) the content column.
#[test]
fn custom_gutter_lines_miss_and_recompose_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;
    let custom: Vec<String> = (0..201).map(|line| format!("+{line}")).collect();

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_custom_gutter_lines(Some(custom.clone()));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "custom gutter text is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the custom gutter must be visible");

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_custom_gutter_lines(Some(custom.clone()));
        },
    );
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the custom-gutter frame must be byte-identical",
    );
}

/// Toggling the gutter changes both buffers' geometry.
#[test]
fn a_gutter_toggle_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_gutter_enabled(false);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a gutter toggle is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the missing gutter must be visible");

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_gutter_enabled(false);
        },
    );
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the gutterless frame must be byte-identical",
    );
}

/// The 999→1000 digit rollover widens the gutter through an ordinary edit —
/// the whole chain (revision, gutter width, content width) must land on the
/// same pixels a cold compose produces.
#[test]
fn the_digit_rollover_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let mut editor = editor_over(998); // 998 lines + the trailing line = 999
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);

    // Append a line: 999 → 1000 total lines, three digits become four.
    let last_line = editor.state().document.line_count() - 1;
    editor.set_cursor(Position::new(last_line, 0));
    let _ = editor.handle_key(&press(KeyCode::Enter));
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "the rollover edit is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the widened gutter must be visible");

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the rolled-over frame must be byte-identical",
    );
}
