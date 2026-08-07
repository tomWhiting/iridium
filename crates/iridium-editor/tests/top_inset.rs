//! The proof that reserving space above the document moves *everything*.
//!
//! A face that draws chrome at the top of its window — the desktop face's
//! tab strip — pushes the document down by calling
//! [`FrameCompositor::set_top_inset`]. Four separate things then have to
//! agree about where the document starts: the painter, the hit test, the
//! scroll-to-caret anchor, and the scroll clamp.
//!
//! **The failure this harness exists for**: an offset applied in the painter
//! but not in the hit test agrees with the truth exactly at the top of the
//! document, where the error is under half a row, and is a full row out
//! everywhere else. Someone clicks a word and the caret lands on the line
//! above it — reliably, but only once the strip is up, so it looks like a
//! tab bug rather than a layout one.
//!
//! Headless by construction, like `retained_shaping.rs`: no surface, an
//! offscreen texture, and a missing adapter fails the run loudly rather than
//! passing over work that never happened.

mod support;

use iridium_editor::Editor;
use iridium_editor::render::FrameCompositor;
use iridium_editor::render::units::{index_to_f32, pixel_to_index};
use iridium_editor::theme::Color;

use support::frame::{Target, read_pixels};
use support::gpu::{Gpu, HEIGHT_F32, HEIGHT_USIZE, WIDTH_USIZE};
use support::pixels::{first_inked_row, inked, page};
use support::scene::{NoHighlights, editor, loud_gutter_theme};

/// The name this harness reports failures and labels GPU objects under.
const HARNESS: &str = "top_inset";

/// The height a tab strip would claim, on top of the document's own ten
/// pixels of breathing room.
const STRIP_HEIGHT: f32 = 34.0;

/// Reports a failure this harness cannot proceed past, naming itself.
fn die(message: &str) -> ! {
    support::gpu::die(HARNESS, message)
}

/// The headless device this harness composes on.
fn gpu() -> Gpu {
    support::gpu::gpu(HARNESS)
}

/// A compositor at the default frame size, font loaded.
fn compositor(gpu: &Gpu) -> FrameCompositor {
    support::gpu::compositor(gpu)
}

/// One compose onto a fresh offscreen target, with the GPU work awaited so
/// the between-frames caches these tests read are populated.
fn compose(compositor: &mut FrameCompositor, editor: &Editor, scroll_y: f32, gpu: &Gpu) {
    let _ = frame(compositor, editor, scroll_y, gpu);
}

/// The same compose, keeping the target so its pixels can be read back.
fn frame(compositor: &mut FrameCompositor, editor: &Editor, scroll_y: f32, gpu: &Gpu) -> Target {
    let target = support::frame::target(gpu);
    support::frame::compose(
        compositor,
        editor,
        scroll_y,
        &mut NoHighlights,
        gpu,
        &target,
    );
    target
}

/// **The round trip.** Ask the compositor where a line is drawn, click
/// there, and get the same line back.
///
/// This is the assertion that catches a forgotten inset on either side,
/// because the two directions read it independently. It is checked at
/// several lines and at a non-zero scroll, since a constant error of less
/// than one row hides at the top of an unscrolled document.
#[test]
fn a_click_where_a_line_is_drawn_resolves_to_that_line_under_an_inset() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_top_inset(10.0 + STRIP_HEIGHT);
    let editor = editor();

    for scroll_y in [0.0, 47.0, 123.0] {
        compose(&mut compositor, &editor, scroll_y, &gpu);

        for line in [0_usize, 3, 9, 14] {
            let Some((x, y)) =
                compositor.position_to_pixel(&editor, editor.fold_state(), scroll_y, line, 2)
            else {
                continue;
            };
            if y < compositor.top_inset() {
                // Scrolled above the reserved band: there is nothing to
                // click, which is the point of reserving it.
                continue;
            }
            // Half a row down from the top edge of the row, which is where
            // a click on that line's text actually lands.
            let middle = y + compositor.line_height() / 2.0;
            let (resolved, _) =
                compositor.pixel_to_position(&editor, editor.fold_state(), scroll_y, x, middle);

            assert_eq!(
                resolved, line,
                "a click at y={middle} (scroll {scroll_y}) resolved to line {resolved}, not {line}"
            );
        }
    }
}

/// The inset actually moves the document, rather than being stored and
/// ignored.
///
/// Without this the round trip above would pass on a compositor that
/// applied the inset in neither direction — two wrongs agreeing is exactly
/// the shape of failure this file is about.
#[test]
fn reserving_space_moves_the_first_line_down_by_that_much() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    compose(&mut plain, &editor, 0.0, &gpu);
    let Some((_, without)) = plain.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0)
    else {
        die("the first line was not placed without an inset");
    };

    let mut inset = compositor(&gpu);
    inset.set_top_inset(10.0 + STRIP_HEIGHT);
    compose(&mut inset, &editor, 0.0, &gpu);
    let Some((_, with)) = inset.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0) else {
        die("the first line was not placed with an inset");
    };

    assert!(
        (with - without - STRIP_HEIGHT).abs() < 0.5,
        "the first line moved by {} pixels, not {STRIP_HEIGHT}",
        with - without
    );
}

/// **Everything the document draws moves down together — the gutter too.**
///
/// The line numbers are a *second* text area with its own Y origin, and it was
/// written from the horizontal padding rather than from the inset. The two
/// agreed exactly while both were ten, which is every frame this editor had
/// ever drawn; the moment a face reserved a tab strip, the numbers stayed put
/// while the code they number moved down by the strip's height, and every
/// line was labelled with the number of the line above it.
///
/// This is read off the composed pixels rather than off a placement query,
/// because the gutter has no placement query — it is prepared straight into a
/// text area — and a defect that only exists in a text area's origin is
/// invisible to every other test in this file.
#[test]
fn the_gutter_moves_down_with_the_text_it_numbers() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    let plain_frame = frame(&mut plain, &editor, 0.0, &gpu);
    let plain_pixels = read_pixels(&gpu, &plain_frame);

    let mut inset = compositor(&gpu);
    inset.set_top_inset(10.0 + STRIP_HEIGHT);
    let inset_frame = frame(&mut inset, &editor, 0.0, &gpu);
    let inset_pixels = read_pixels(&gpu, &inset_frame);

    let gutter = plain.gutter_width(60);
    assert!(gutter > 0.0, "the gutter is off; this test proves nothing");

    // The two columns that must move as one: the numbers, and the code.
    for (name, left, right) in [
        ("the gutter", 0.0, gutter),
        ("the content", gutter + 12.0, 512.0),
    ] {
        let Some(before) = first_inked_row(&plain_pixels, left, right) else {
            die(&format!("{name} column drew nothing without an inset"));
        };
        let Some(after) = first_inked_row(&inset_pixels, left, right) else {
            die(&format!("{name} column drew nothing under an inset"));
        };
        let moved = index_to_f32(after) - index_to_f32(before);
        assert!(
            (moved - STRIP_HEIGHT).abs() <= 2.0,
            "{name} moved {moved} pixels, not {STRIP_HEIGHT}"
        );
    }
}

/// **The inline blame ghost text is clipped at the band too.**
///
/// The third text area, and the one that kept the window's edge as its top
/// bound after the content and the gutter were moved onto the inset. It was
/// missed because blame is populated only from the web face, which draws no
/// chrome above the document — so the two disagreed on every frame anyone
/// had composed, and would have started disagreeing visibly the day a
/// TypeScript tab strip landed.
///
/// Blame sits on the caret's line, and the caret can be scrolled up behind
/// the reserved band; with the window edge as the bound, its ghost text is
/// then drawn *inside* a face's chrome.
///
/// The claim is made against a second frame rather than against an empty
/// band, because the band is not empty: the caret and the line-background
/// quads are geometry, not text, and a text area's bounds do not clip them
/// at all. Composing the same scroll with and without blame data isolates
/// exactly the ink this test is about.
#[test]
fn the_blame_ghost_text_stays_out_of_the_reserved_band() {
    /// A scroll that leaves the caret's line inside the band rather than
    /// above the window: `top_inset` minus this is where its text lands.
    const INTO_THE_BAND: f32 = 14.0;

    let gpu = gpu();
    let editor = editor();
    let inset = 10.0 + STRIP_HEIGHT;

    let mut bare = compositor(&gpu);
    bare.set_top_inset(inset);
    let bare_frame = frame(&mut bare, &editor, INTO_THE_BAND, &gpu);
    let bare_pixels = read_pixels(&gpu, &bare_frame);

    let mut blamed = compositor(&gpu);
    blamed.set_top_inset(inset);
    blamed
        .blame_data_mut()
        .insert(0, "committed by someone, a while ago".to_owned());
    let blamed_frame = frame(&mut blamed, &editor, INTO_THE_BAND, &gpu);
    let blamed_pixels = read_pixels(&gpu, &blamed_frame);

    let band = pixel_to_index(inset).min(HEIGHT_USIZE);
    assert!(band > 0, "there is no band; this test proves nothing");

    for row in 0..band {
        for column in 0..WIDTH_USIZE {
            let at = (row * WIDTH_USIZE + column) * 4;
            assert_eq!(
                bare_pixels[at..at + 3],
                blamed_pixels[at..at + 3],
                "the blame ghost text drew at ({column}, {row}), inside the {inset}-pixel \
                 band the face reserved"
            );
        }
    }
}

/// The scroll clamp grows with the inset, so the last line can still be
/// reached.
///
/// A clamp that ignored it would stop scrolling `STRIP_HEIGHT` pixels early
/// and leave the final line permanently behind the chrome — visible only to
/// someone who scrolled all the way down, which is the last thing anyone
/// tests by hand.
#[test]
fn the_scroll_clamp_still_reaches_the_last_line() {
    let gpu = gpu();
    let editor = editor();
    let height = HEIGHT_F32;

    let mut plain = compositor(&gpu);
    compose(&mut plain, &editor, 0.0, &gpu);
    let without = plain.max_scroll_y(&editor, editor.fold_state(), height);

    let mut inset = compositor(&gpu);
    inset.set_top_inset(10.0 + STRIP_HEIGHT);
    compose(&mut inset, &editor, 0.0, &gpu);
    let with = inset.max_scroll_y(&editor, editor.fold_state(), height);

    assert!(
        (with - without - STRIP_HEIGHT).abs() < 0.5,
        "the clamp moved by {} pixels, not {STRIP_HEIGHT}",
        with - without
    );
}

/// The default is the document's own breathing room, unchanged.
///
/// Every face that draws no chrome above the text — the web face, the
/// screenshot harness, the benches — must be pixel-identical to before this
/// existed.
#[test]
fn the_default_inset_is_the_documents_own_padding() {
    let gpu = gpu();
    let compositor = compositor(&gpu);

    assert!((compositor.top_inset() - 10.0).abs() < f32::EPSILON);
}

/// A nonsense inset is refused rather than stored.
///
/// Negative would draw the document above the top of the window, and NaN
/// would poison every comparison downstream of it. Both are rejected at the
/// setter so no consumer has to guard.
#[test]
fn a_nonsense_inset_leaves_the_previous_one_in_place() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_top_inset(44.0);

    compositor.set_top_inset(-1.0);
    assert!((compositor.top_inset() - 44.0).abs() < f32::EPSILON);

    compositor.set_top_inset(f32::NAN);
    assert!((compositor.top_inset() - 44.0).abs() < f32::EPSILON);

    compositor.set_top_inset(f32::INFINITY);
    assert!((compositor.top_inset() - 44.0).abs() < f32::EPSILON);
}

/// Nothing the document draws reaches into the reserved band — **at a
/// scroll**, which is the case the X axis has no analogue for.
///
/// `left_inset.rs` already asserts the same sentence for the left band, and
/// it holds there because nothing scrolls sideways: every X coordinate is
/// computed once, from the inset, and either uses it or does not. The Y axis
/// is different. A row's Y is `top_inset + row - scroll`, so a row that
/// started below the band *moves into it* as the document scrolls, and the
/// question stops being "was the inset applied?" and becomes "is the band
/// clipped?".
///
/// Those are different questions with different answers. The text is clipped
/// — `TextBounds` carries `top: top_inset` on the content, the gutter and the
/// blame ghost. **`TextBounds` clips text, not geometry**, and every quad the
/// compositor emits is geometry: the gutter's background, its change bars,
/// the diff line backgrounds, the selection highlights and the caret. Each
/// one tests its own visibility against `0.0` — the window's top edge — which
/// is the same number as the band's bottom edge for exactly as long as no
/// face reserves anything.
///
/// The frame below is arranged so that all five would show: a vivid gutter
/// colour, change bars, a line background, a multi-line selection, and a
/// caret, all on lines the scroll has carried up into the band.
///
/// On the desktop face this is masked today, because the overlay paints the
/// tab strip opaquely over the band in a second pass. It is masked only for
/// as long as that chrome stays opaque and full-width — and rounded corners
/// alone would expose it.
#[test]
fn nothing_the_document_draws_reaches_into_the_reserved_band_at_a_scroll() {
    let gpu = gpu();
    let inset = 10.0 + STRIP_HEIGHT;

    let mut compositor = compositor(&gpu);
    compositor.set_theme(loud_gutter_theme());
    compositor.set_top_inset(inset);

    // A scroll of six rows puts row five one row above the band's bottom
    // edge — inside it, not above the window, which is the only placement
    // that distinguishes a clip from the cull the builders already do.
    let line_height = compositor.line_height();
    assert!(
        line_height > 0.0 && line_height < inset,
        "the band must be more than one row deep or a row cannot sit inside it"
    );
    let scroll = 6.0 * line_height;

    let mut editor = editor();
    // Anchor above, head inside the band: the selection spans the band's
    // rows, and the caret is drawn on the head's row.
    editor.set_selection(
        iridium_editor::Position::new(3, 0),
        iridium_editor::Position::new(5, 8),
    );

    for line in 0..12 {
        compositor
            .gutter_changes_mut()
            .insert(line, Color::new(0.20, 0.85, 0.45, 1.0));
        compositor
            .line_backgrounds_mut()
            .insert(line, Color::new(0.95, 0.75, 0.10, 1.0));
    }

    let frame = frame(&mut compositor, &editor, scroll, &gpu);
    let pixels = read_pixels(&gpu, &frame);
    let page = page(&pixels);

    let band = pixel_to_index(inset).min(HEIGHT_USIZE);
    assert!(band > 0, "there is no band; this test proves nothing");

    for row in 0..band {
        for column in 0..WIDTH_USIZE {
            assert!(
                !inked(&pixels, page, column, row),
                "the document drew at ({column}, {row}), inside the {inset}-pixel band the \
                 face reserved"
            );
        }
    }
}

/// The same claim with the gutter turned off, so the document's *own* quads
/// have to answer for themselves.
///
/// The test above fails on the first offending pixel, and the gutter's
/// background quad — which spans the full frame height from zero — is at
/// column zero, row zero. It would therefore report a leak while the
/// selection, the line backgrounds and the caret were all clipped correctly,
/// and it would keep reporting one after they were fixed. This one removes
/// the gutter entirely: anything left in the band is content.
#[test]
fn the_documents_own_quads_stay_out_of_the_reserved_band_at_a_scroll() {
    let gpu = gpu();
    let inset = 10.0 + STRIP_HEIGHT;

    let mut compositor = compositor(&gpu);
    compositor.set_top_inset(inset);
    compositor.set_gutter_enabled(false);

    let line_height = compositor.line_height();
    assert!(
        line_height > 0.0 && line_height < inset,
        "the band must be more than one row deep or a row cannot sit inside it"
    );
    let scroll = 6.0 * line_height;

    let mut editor = editor();
    editor.set_selection(
        iridium_editor::Position::new(3, 0),
        iridium_editor::Position::new(5, 8),
    );
    for line in 0..12 {
        compositor
            .line_backgrounds_mut()
            .insert(line, Color::new(0.95, 0.75, 0.10, 1.0));
    }

    let frame = frame(&mut compositor, &editor, scroll, &gpu);
    let pixels = read_pixels(&gpu, &frame);
    let page = page(&pixels);

    let band = pixel_to_index(inset).min(HEIGHT_USIZE);
    for row in 0..band {
        for column in 0..WIDTH_USIZE {
            assert!(
                !inked(&pixels, page, column, row),
                "the document drew at ({column}, {row}), inside the {inset}-pixel band the \
                 face reserved"
            );
        }
    }
}
