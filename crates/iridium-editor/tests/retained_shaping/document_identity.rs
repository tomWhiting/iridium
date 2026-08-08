//! TEMPORARY — #69 diagnostic. Not to be committed.

use iridium_editor::Editor;

use crate::harness::{ActiveLanguageNoSpans, compose, compositor, gpu, pixels, target};
use crate::support::gpu::{HEIGHT, WIDTH};

/// Two different documents, composed into one compositor.
///
/// This is what a face does on every tab switch: `DesktopApp` holds a single
/// `FrameCompositor` and composes whichever tab's `Editor` is active.
#[test]
fn two_documents_through_one_compositor_render_as_themselves() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut shared = compositor(&gpu);

    let mut first = Editor::with_defaults();
    first.set_content("AAAA AAAA AAAA\nAAAA AAAA AAAA\nAAAA AAAA AAAA\n");
    let mut second = Editor::with_defaults();
    second.set_content("ZZZZ ZZZZ ZZZZ\nZZZZ ZZZZ ZZZZ\nZZZZ ZZZZ ZZZZ\n");

    println!(
        "revisions: first={} second={}",
        first.state().document.revision(),
        second.state().document.revision()
    );

    compose(
        &mut shared,
        &first,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let frame_a = pixels(&gpu, &tgt);

    compose(
        &mut shared,
        &second,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let frame_b = pixels(&gpu, &tgt);

    println!("shape_rebuilds after both = {}", shared.shape_rebuilds());

    // A cold compositor's answer for the second document, as the oracle.
    let mut fresh = compositor(&gpu);
    compose(
        &mut fresh,
        &second,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let cold_b = pixels(&gpu, &tgt);

    assert!(
        frame_a != cold_b,
        "the fixture is wrong: the two documents must render differently"
    );
    assert!(
        frame_b == cold_b,
        "the shared compositor drew the FIRST document's text for the second"
    );
}
