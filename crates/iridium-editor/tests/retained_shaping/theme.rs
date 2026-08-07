//! The miss side: the palette the text and the fallback highlighter draw from.

use iridium_editor::theme::{Color, Theme};

use crate::harness::{
    ActiveLanguageNoSpans, cold_pixels, compose, compositor, editor_over, gpu, pixels, target,
};
use crate::support::gpu::{HEIGHT, WIDTH};
use crate::support::pixels::assert_same_frame;

/// The theme feeds text colors and the fallback highlighter's palette.
#[test]
fn a_theme_flip_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_dark_theme(false);
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a theme flip is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the theme flip must be visible");

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_dark_theme(false);
        },
    );
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the light-theme frame must be byte-identical",
    );
}

/// The staleness row for `set_theme`, the arbitrary-theme mutator beside
/// `set_dark_theme`: replacing the theme must miss — a `set_theme` that
/// skipped the generation bump would serve stale-colored retained frames —
/// and the frame must both present the new background and be byte-identical
/// to a cold compositor holding the same theme.
#[test]
fn a_set_theme_misses_and_recomposes_identically() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut warm = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;
    let mut theme = Theme::dark();
    // #336699: every channel an exact multiple of 1/255, so the readback
    // bytes are exact.
    theme.editor.background = Color::new(0.2, 0.4, 0.6, 1.0);

    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let before = pixels(&gpu, &tgt);
    warm.set_theme(theme.clone());
    compose(&mut warm, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(warm.shape_rebuilds(), 2, "a theme replacement is a miss");
    let after = pixels(&gpu, &tgt);
    assert!(after != before, "the new background must be visible");

    // The clear color follows the set theme, not a frozen preset.
    let corner = (((HEIGHT - 1) * WIDTH + (WIDTH - 1)) * 4) as usize;
    assert_eq!(
        &after[corner..corner + 4],
        &[153, 102, 51, 255],
        "the frame must stand on the set theme's background (Bgra8Unorm bytes)"
    );

    let cold = cold_pixels(
        &gpu,
        &tgt,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        |fresh| {
            fresh.set_theme(theme.clone());
        },
    );
    assert_same_frame(
        &after,
        &cold,
        WIDTH,
        "the set-theme frame must be byte-identical",
    );
}
