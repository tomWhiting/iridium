//! One compositor, several documents — the tab-switch case.
//!
//! Every other file in this suite composes **one** document per compositor, so
//! none of them could see this: the cache identified a document by its
//! revision, which counts that document's own changes and starts at zero for
//! every document, so two files edited the same number of times were
//! indistinguishable. A face holds one `FrameCompositor` and composes whichever
//! tab is active (`apps/iridium-desktop/src/app/paint.rs:80`), so the switch
//! was a cache hit and the previous file's buffers were re-presented.
//!
//! See `docs/IN-FLIGHT-87-document-identity.md`.

use iridium_editor::Editor;

use crate::harness::{ActiveLanguageNoSpans, compose, compositor, gpu, pixels, target};
use crate::support::gpu::{HEIGHT, WIDTH};
use crate::support::pixels::assert_same_frame;

/// An editor over `text`, loaded the way a face loads a file.
fn editor_over_text(text: &str) -> Editor {
    let mut editor = Editor::with_defaults();
    editor.set_content(text);
    editor
}

/// Enough lines to overflow the viewport, so `viewport_end` is the window's
/// height rather than the document's length.
///
/// ⚠️ This is what made the defect hide. `viewport_end` is `min(start +
/// visible, line_count)`, so two *short* documents of different lengths are
/// told apart by it accidentally — and a short fixture is exactly what a test
/// reaches for. Two real files both longer than the window are not.
fn filled(glyph: char) -> String {
    let mut text = String::new();
    for _ in 0..200 {
        for _ in 0..12 {
            text.push(glyph);
            text.push(' ');
        }
        text.push('\n');
    }
    text
}

/// ⭐ The one that would put the wrong file on screen.
#[test]
fn a_second_document_through_the_same_compositor_is_not_a_cache_hit() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut shared = compositor(&gpu);

    let first = editor_over_text(&filled('A'));
    let second = editor_over_text(&filled('Z'));

    // The premise, asserted rather than assumed: these two are exactly as
    // alike as two freshly-opened files are, which is what makes the case
    // ordinary rather than a corner.
    assert_eq!(
        first.state().document.revision(),
        second.state().document.revision(),
        "both documents must sit at the same revision, or this proves nothing"
    );
    assert_ne!(
        first.state().document.id(),
        second.state().document.id(),
        "two documents must never share an identity"
    );

    compose(
        &mut shared,
        &first,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let first_frame = pixels(&gpu, &tgt);

    compose(
        &mut shared,
        &second,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let second_frame = pixels(&gpu, &tgt);

    assert_eq!(
        shared.shape_rebuilds(),
        2,
        "a different document is a miss — the retained buffers hold the other file's text"
    );

    // The oracle: what a fresh compositor makes of the second document.
    let mut fresh = compositor(&gpu);
    compose(
        &mut fresh,
        &second,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let cold = pixels(&gpu, &tgt);

    // Without this the assertion below would pass against a compositor that
    // rendered nothing at all for either document.
    assert!(
        first_frame != cold,
        "the fixture is wrong: the two documents must render differently"
    );
    assert_same_frame(
        &second_frame,
        &cold,
        WIDTH,
        "the shared compositor must draw the second document, not the first",
    );
}

/// The same switch, then back again. A compositor that keyed the identity but
/// retained per-document buffers could pass the test above and still be wrong
/// here; this pins that a return is a miss too.
#[test]
fn switching_back_to_the_first_document_is_a_miss_as_well() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut shared = compositor(&gpu);

    let first = editor_over_text(&filled('A'));
    let second = editor_over_text(&filled('Z'));

    compose(
        &mut shared,
        &first,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let first_frame = pixels(&gpu, &tgt);

    compose(
        &mut shared,
        &second,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );

    compose(
        &mut shared,
        &first,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    let returned = pixels(&gpu, &tgt);

    assert_eq!(shared.shape_rebuilds(), 3, "every switch is a miss");
    assert_same_frame(
        &returned,
        &first_frame,
        WIDTH,
        "coming back to a document must show that document",
    );
}

/// The same document composed twice is still a hit. Without this, the fix
/// could be "never cache anything", which would pass every assertion above and
/// throw away the whole point of the retained-shaping cache.
#[test]
fn the_same_document_twice_is_still_a_hit() {
    let gpu = gpu();
    let tgt = target(&gpu, WIDTH, HEIGHT);
    let mut shared = compositor(&gpu);

    let editor = editor_over_text(&filled('A'));

    compose(
        &mut shared,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );
    compose(
        &mut shared,
        &editor,
        0.0,
        &mut ActiveLanguageNoSpans,
        &gpu,
        &tgt,
    );

    assert_eq!(
        shared.shape_rebuilds(),
        1,
        "an unchanged document must still hit — the identity is a key member, not a kill switch"
    );
}

/// A document's identity is its own, and a whole-content replacement takes a
/// fresh one.
#[test]
fn identities_are_distinct_and_a_replacement_takes_a_new_one() {
    use iridium_editor::Document;

    let first = Document::new("one");
    let second = Document::new("one");
    assert_ne!(
        first.id(),
        second.id(),
        "identical text is still two documents — the id is not a hash"
    );

    let replaced = Document::continuing_from("two", &first);
    assert_ne!(
        replaced.id(),
        first.id(),
        "a whole-content replacement is worth a fresh identity: everything \
         built for the old text is worthless"
    );
    assert!(
        replaced.revision() > first.revision(),
        "and the revision still moves, as it always did"
    );
}
