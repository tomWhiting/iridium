//! The miss side: the language answer, the spans, and their generation.

use iridium_editor::theme::Color;

use crate::harness::{
    ActiveLanguageNoSpans, TintHighlights, ToggleLanguage, cold_pixels, compose, compositor,
    editor_over, gpu, pixels, target,
};
use crate::support::gpu::{HEIGHT, WIDTH};

/// Toggling syntax highlighting switches the whole fill path.
#[test]
fn a_syntax_toggle_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_syntax_enabled(false);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a syntax toggle is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "losing the keyword colors must be visible");

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_syntax_enabled(false);
        },
    );
    assert!(after == cold, "the plain frame must be byte-identical");
}

/// The no-language ruling at the compositor's own seam: a source that
/// reports no language and no spans composes the plain frame — byte-identical
/// to the syntax-disabled fill path, and visibly not the keyword fallback.
#[test]
fn a_language_less_source_composes_the_plain_frame() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut void = ToggleLanguage { active: false };

    compose(&mut compositor, &editor, 0.0, &mut void, &gpu, &tgt);
    let composed = pixels(&gpu, &tgt);

    let plain = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_syntax_enabled(false);
        },
    );
    assert!(
        composed == plain,
        "no language must render the plain frame, never the keyword fallback"
    );

    let bridged = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert!(
        composed != bridged,
        "and the comparison discriminates: the bridge frame is coloured"
    );
}

/// The bridge half of the ruling, pinned: a language set with spans absent
/// this frame still gets the built-in keyword fallback's colors.
#[test]
fn a_set_language_without_spans_keeps_the_keyword_fallback() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut bridge = ToggleLanguage { active: true };

    compose(&mut compositor, &editor, 0.0, &mut bridge, &gpu, &tgt);
    let bridged = pixels(&gpu, &tgt);

    let fallback = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert!(
        bridged == fallback,
        "a language-active source with no spans is exactly the fallback frame"
    );

    let plain = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_syntax_enabled(false);
        },
    );
    assert!(
        bridged != plain,
        "and the fallback really coloured: the bridge frame is not the plain one"
    );
}

/// The staleness-matrix row for the language answer: a language set or
/// unset at runtime — with every other input, the generation included,
/// unchanged — must miss, and both directions must recompose exactly what a
/// cold compositor produces for the same state.
#[test]
fn a_language_set_or_unset_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut source = ToggleLanguage { active: true };

    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 1, "the first frame is cold");
    let bridged = pixels(&gpu, &tgt);

    source.active = false;
    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "unsetting the language is a miss");
    let unlanguaged = pixels(&gpu, &tgt);
    assert!(
        unlanguaged != bridged,
        "losing the language must lose the keyword colors"
    );
    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ToggleLanguage { active: false },
        |_| {},
    );
    assert!(
        unlanguaged == cold,
        "the language-less frame must be byte-identical to a cold compose"
    );

    source.active = true;
    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 3, "setting it back is a miss too");
    let rebridged = pixels(&gpu, &tgt);
    assert!(
        rebridged == bridged,
        "the returned language must reproduce the fallback frame exactly"
    );
}

/// The face's highlight generation is a key member: bumping it (with a
/// genuinely different answer) recolors, and the recolored frame matches a
/// cold compose against the same source.
#[test]
fn a_highlight_generation_bump_misses_and_recolors_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let tint = Color::new(0.1, 0.8, 0.3, 1.0);
    let mut source = TintHighlights {
        tint: None,
        generation: 0,
    };

    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);

    source.tint = Some(tint);
    source.generation = 1;
    compose(&mut warm, &editor, 0.0, &mut source, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a generation bump is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the new spans must be visible");

    let mut cold_source = TintHighlights {
        tint: Some(tint),
        generation: 7,
    };
    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut cold_source, |_| {});
    assert!(after == cold, "the recolored frame must be byte-identical");
}

/// The raw `syntax_theme_mut` accessor bumps its generation on every
/// mutable borrow — the borrow is the change signal.
#[test]
fn a_syntax_theme_borrow_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let _ = warm.syntax_theme_mut();
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        warm.shape_rebuilds(),
        2,
        "a mutable borrow of the theme map is a miss by contract"
    );
    let after = pixels(&gpu, &tgt);

    let cold = cold_pixels(&gpu, &tgt, &editor, 0.0, &mut ActiveLanguageNoSpans, |_| {});
    assert!(
        after == cold,
        "an unchanged map still recomposes identically"
    );
}
