//! The hit side of the matrix: the inputs deliberately outside the shape key.

use iridium_editor::Editor;
use iridium_editor::render::FrameTarget;
use iridium_editor::theme::Color;

use crate::harness::{
    ActiveLanguageNoSpans, compose, compositor, editor_over, gpu, pixels, target,
};
use crate::support::gpu::{HEIGHT, WIDTH};

/// An identical frame, a sub-line scroll, a blink-phase frame and every
/// per-frame presentation input are hits — and the hit frame's pixels are
/// byte-identical to the cold frame before it (§5.3a).
#[test]
fn steady_frames_hit_and_reproduce_the_cold_frame_exactly() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(compositor.shape_rebuilds(), 1, "the first frame is cold");
    let cold = pixels(&gpu, &tgt);

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        compositor.shape_rebuilds(),
        1,
        "an identical frame is a hit"
    );
    let hot = pixels(&gpu, &tgt);
    assert!(
        hot == cold,
        "the hit frame must be byte-identical to the cold one"
    );

    // A sub-line scroll: the viewport line range is unchanged, only the
    // remainder applied at the text area's top edge moves.
    let line_height = compositor.line_height();
    compose(
        &mut compositor,
        &editor,
        0.4 * line_height,
        &mut highlights,
        &gpu,
        &tgt,
    );
    assert_eq!(compositor.shape_rebuilds(), 1, "a sub-line scroll is a hit");

    // Presentation inputs feed per-frame quads and blame, never the shaped
    // buffers — they must not invalidate.
    compositor
        .line_backgrounds_mut()
        .insert(2, Color::new(0.2, 0.4, 0.2, 1.0));
    compositor
        .gutter_changes_mut()
        .insert(3, Color::new(0.8, 0.6, 0.1, 1.0));
    compositor
        .blame_data_mut()
        .insert(0, "author, yesterday".to_string());
    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(
        compositor.shape_rebuilds(),
        1,
        "presentation inputs must not invalidate the shapes"
    );

    // A frame later in the blink cycle: blink is compose-internal state,
    // not a shaping input.
    std::thread::sleep(std::time::Duration::from_millis(600));
    compositor
        .compose(
            &editor,
            editor.fold_state(),
            0.0,
            &mut highlights,
            FrameTarget {
                view: &tgt.view,
                device: &gpu.device,
                queue: &gpu.queue,
                width: tgt.width,
                height: tgt.height,
            },
        )
        .expect("the blink frame must compose");
    assert_eq!(compositor.shape_rebuilds(), 1, "a blink frame is a hit");
    assert_eq!(
        compositor.lines_reshaped(),
        0,
        "no frame here went through the diff path"
    );
}

/// The composed frame stands on the theme's own background: the dark
/// preset must present opaque `#1a1a1a` — the pixels the web demo shows
/// behind its canvas — on a surface with no compositing behind it.
#[test]
fn the_dark_frame_background_is_opaque_1a1a1a() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let editor = editor_over(200);
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let frame = pixels(&gpu, &tgt);
    // The bottom-right corner pixel: right of the gutter column and every
    // glyph, under no quad — the clear color alone.
    let corner = (((HEIGHT - 1) * WIDTH + (WIDTH - 1)) * 4) as usize;
    assert_eq!(
        &frame[corner..corner + 4],
        &[26, 26, 26, 255],
        "the dark background must present as opaque #1a1a1a (Bgra8Unorm bytes)"
    );
}

/// The empty document is the degenerate content path (one empty buffer
/// line): it must compose, hit, and reproduce itself exactly.
#[test]
fn an_empty_document_composes_and_hits() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut compositor = compositor(&gpu);
    let mut editor = Editor::with_defaults();
    editor.set_content("");
    let mut highlights = ActiveLanguageNoSpans;

    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    let cold = pixels(&gpu, &tgt);
    compose(&mut compositor, &editor, 0.0, &mut highlights, &gpu, &tgt);
    assert_eq!(compositor.shape_rebuilds(), 1, "the empty frame hits too");
    let hot = pixels(&gpu, &tgt);
    assert!(hot == cold, "the empty hit frame must be byte-identical");
}
