//! The proof that reserving space beside the document moves *everything*.
//!
//! A face that draws chrome down the left of its window — a sidebar, a file
//! tree — pushes the document across by calling
//! [`FrameCompositor::set_left_inset`]. Six separate things then have to
//! agree about where the left edge is: the gutter's background, the line
//! numbers, the change bars, the content column, and both directions of
//! hit-testing.
//!
//! **The failure this harness exists for** is the one the vertical axis
//! already suffered. The line numbers were written from the horizontal
//! padding while the code was written from the vertical inset; the two were
//! the same number on every frame this editor had ever drawn, so nothing
//! caught it until a tab strip moved one and not the other. Every X-axis
//! measure in the compositor started life at "the window's left edge" and
//! agrees with "the reserved edge" for exactly as long as nothing is
//! reserved.
//!
//! So the load-bearing test here is not a query — it is a pixel. The gutter
//! and the change bars have no placement query at all; they are quads and a
//! text area, and a defect that lives only in an origin is invisible to
//! every assertion that does not look at the frame.
//!
//! Headless by construction, like `top_inset.rs`: no surface, an offscreen
//! texture, and a missing adapter fails the run loudly rather than passing
//! over work that never happened.

mod support;

use iridium_editor::Editor;
use iridium_editor::render::FrameCompositor;
use iridium_editor::render::units::{index_to_f32, pixel_to_index};
use iridium_editor::theme::Color;

use support::frame::{Target, read_pixels};
use support::gpu::{Gpu, HEIGHT_USIZE, WIDTH_USIZE};
use support::pixels::{first_inked_column, inked, page};
use support::scene::{NoHighlights, editor, loud_gutter_theme};

/// The name this harness reports failures and labels GPU objects under.
const HARNESS: &str = "left_inset";

/// The width a sidebar would claim down the left of the window.
///
/// Wide enough that a missed inset is a whole character cell out rather than
/// a rounding argument, and narrow enough to leave the document a usable
/// column in a 512-pixel frame.
const SIDEBAR_WIDTH: f32 = 96.0;

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
fn compose(compositor: &mut FrameCompositor, editor: &Editor, gpu: &Gpu) {
    let _ = frame(compositor, editor, gpu);
}

/// The same compose, keeping the target so its pixels can be read back.
///
/// Nothing here scrolls: every X coordinate is computed once, from the inset,
/// and either uses it or does not, which is the whole reason this axis is
/// simpler than the vertical one.
fn frame(compositor: &mut FrameCompositor, editor: &Editor, gpu: &Gpu) -> Target {
    let target = support::frame::target(gpu);
    support::frame::compose(compositor, editor, 0.0, &mut NoHighlights, gpu, &target);
    target
}

/// **The round trip.** Ask the compositor where a column is drawn, click
/// there, and get the same column back.
///
/// This catches a forgotten inset on either side of the hit test, because
/// the two directions read it independently. Checked at several columns,
/// since an error of less than half a character hides at column zero.
#[test]
fn a_click_where_a_column_is_drawn_resolves_to_that_column_under_an_inset() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_left_inset(SIDEBAR_WIDTH);
    let editor = editor();
    compose(&mut compositor, &editor, &gpu);

    for column in [0_usize, 1, 7, 14, 20] {
        let Some((x, y)) =
            compositor.position_to_pixel(&editor, editor.fold_state(), 0.0, 3, column)
        else {
            die("line 3 was not placed under an inset");
        };
        let middle = y + compositor.line_height() / 2.0;
        let (line, resolved) =
            compositor.pixel_to_position(&editor, editor.fold_state(), 0.0, x, middle);

        assert_eq!(line, 3, "a click at x={x} resolved to line {line}, not 3");
        assert_eq!(
            resolved, column,
            "a click at x={x} resolved to column {resolved}, not {column}"
        );
    }
}

/// The inset actually moves the content column, rather than being stored and
/// ignored.
///
/// Without this the round trip above would pass on a compositor that applied
/// the inset in neither direction — two wrongs agreeing is the shape of
/// failure this file is about.
#[test]
fn reserving_space_moves_the_content_column_across_by_that_much() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    compose(&mut plain, &editor, &gpu);
    let Some((without, _)) = plain.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0)
    else {
        die("the first column was not placed without an inset");
    };

    let mut inset = compositor(&gpu);
    inset.set_left_inset(SIDEBAR_WIDTH);
    compose(&mut inset, &editor, &gpu);
    let Some((with, _)) = inset.position_to_pixel(&editor, editor.fold_state(), 0.0, 0, 0) else {
        die("the first column was not placed under an inset");
    };

    assert!(
        (with - without - SIDEBAR_WIDTH).abs() < 0.5,
        "the content column moved by {} pixels, not {SIDEBAR_WIDTH}",
        with - without
    );
}

/// **Nothing the document draws reaches into the reserved band.**
///
/// The load-bearing test. Six X-axis measures in the compositor were written
/// from the window's left edge — the gutter's background quad, the line
/// numbers' origin and both of their clip bounds, the change bars, and the
/// content text area's left bound. Every one of them agrees with the
/// reserved edge while nothing is reserved, which is every frame drawn
/// before this existed.
///
/// So the frame is arranged so that each of them would *show*: a vivid
/// gutter colour, because the stock dark theme paints the gutter the same
/// colour as the page and an unmoved quad would be invisible; and a change
/// bar, because the bars are three pixels wide at a hardcoded offset and
/// nothing else would reveal them. Then the claim is one sentence — the
/// reserved band is empty — and it holds against all six at once.
#[test]
fn nothing_the_document_draws_reaches_into_the_reserved_band() {
    let gpu = gpu();
    let editor = editor();

    let mut inset = compositor(&gpu);
    inset.set_theme(loud_gutter_theme());
    inset.set_left_inset(SIDEBAR_WIDTH);
    for line in 0..12 {
        inset
            .gutter_changes_mut()
            .insert(line, Color::new(0.20, 0.85, 0.45, 1.0));
    }
    let frame = frame(&mut inset, &editor, &gpu);
    let pixels = read_pixels(&gpu, &frame);
    let page = page(&pixels);

    let band = pixel_to_index(SIDEBAR_WIDTH).min(WIDTH_USIZE);
    for column in 0..band {
        for row in 0..HEIGHT_USIZE {
            assert!(
                !inked(&pixels, page, column, row),
                "the document drew at ({column}, {row}), inside the {SIDEBAR_WIDTH}-pixel band \
                 the face reserved"
            );
        }
    }
}

/// The band is empty because the document moved, not because it stopped
/// drawing.
///
/// The test above is satisfied by a compositor that draws nothing at all.
/// This one pins the other side: the leftmost ink in the frame is exactly a
/// sidebar's width further right than it was without one.
#[test]
fn the_gutter_moves_across_with_the_content_it_sits_beside() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    plain.set_theme(loud_gutter_theme());
    let plain_frame = frame(&mut plain, &editor, &gpu);
    let plain_pixels = read_pixels(&gpu, &plain_frame);

    let mut inset = compositor(&gpu);
    inset.set_theme(loud_gutter_theme());
    inset.set_left_inset(SIDEBAR_WIDTH);
    let inset_frame = frame(&mut inset, &editor, &gpu);
    let inset_pixels = read_pixels(&gpu, &inset_frame);

    let Some(before) = first_inked_column(&plain_pixels) else {
        die("the frame drew nothing without an inset");
    };
    let Some(after) = first_inked_column(&inset_pixels) else {
        die("the frame drew nothing under an inset");
    };

    let moved = index_to_f32(after) - index_to_f32(before);
    assert!(
        (moved - SIDEBAR_WIDTH).abs() <= 2.0,
        "the leftmost thing the document draws moved {moved} pixels, not {SIDEBAR_WIDTH}"
    );
}

/// **Everything right of the band is the same pixels it was, moved across.**
///
/// A pure translation is a fair claim here and only here: the lines in
/// [`editor`] are far shorter than the content column at either inset, so
/// nothing wraps differently, and the inset is a whole number of pixels, so
/// nothing rasterizes differently either. Comparing the frames outright is
/// then stricter than any eye, and covers every glyph rather than the ones
/// someone thought to look at.
///
/// **What it deliberately does not claim** is that the content's left clip
/// is in the right place. Both frames clip relative to their own content
/// column, so a bound set uniformly too far right shaves both by the same
/// amount and the translation survives intact — two wrongs agreeing, the
/// same shape of failure this whole file is about. Verified: a twelve-pixel
/// over-tight bound passes this test untouched. The absolute claim is
/// [`the_content_starts_where_the_compositor_says_it_does`], which is why
/// that test exists separately.
#[test]
fn the_content_column_is_the_same_pixels_a_sidebars_width_across() {
    let gpu = gpu();
    let editor = editor();

    let mut plain = compositor(&gpu);
    plain.set_theme(loud_gutter_theme());
    let plain_frame = frame(&mut plain, &editor, &gpu);
    let plain_pixels = read_pixels(&gpu, &plain_frame);

    let mut inset = compositor(&gpu);
    inset.set_theme(loud_gutter_theme());
    inset.set_left_inset(SIDEBAR_WIDTH);
    let inset_frame = frame(&mut inset, &editor, &gpu);
    let inset_pixels = read_pixels(&gpu, &inset_frame);

    let shift = pixel_to_index(SIDEBAR_WIDTH);
    let start = pixel_to_index(plain.content_left_edge(60));
    if start + shift >= WIDTH_USIZE {
        die("the frame is too narrow for this comparison to mean anything");
    }

    let page = page(&plain_pixels);
    let mut compared = 0_usize;
    for row in 0..HEIGHT_USIZE {
        for column in start..(WIDTH_USIZE - shift) {
            let here = (row * WIDTH_USIZE + column) * 4;
            let there = (row * WIDTH_USIZE + column + shift) * 4;
            assert_eq!(
                plain_pixels[here..here + 3],
                inset_pixels[there..there + 3],
                "the content pixel at ({column}, {row}) is not the one that \
                 landed {SIDEBAR_WIDTH} pixels further right under an inset"
            );
            if inked(&plain_pixels, page, column, row) {
                compared += 1;
            }
        }
    }

    assert!(
        compared > 1_000,
        "only {compared} inked pixels were compared; this test proves nothing"
    );
}

/// **The content's first glyph is where the compositor says the column
/// begins**, under an inset and without one.
///
/// The absolute claim the translation test cannot make. The content text
/// area's left bound moved from the window's edge to the gutter's, and a
/// bound set too far right slices the first column of glyphs — invisible to
/// every relative assertion, because the ink that survives has still moved
/// by exactly the right amount.
///
/// The tolerance is one character cell, and it is a real limit rather than
/// a hedge. A glyph's ink starts a small bearing inside its cell, and that
/// bearing is a fact about the font, not about this compositor; a bound
/// wrong by less than it removes nothing at all, so there is nothing there
/// to catch. A cell is enough to catch the mistake anyone would actually
/// make — clipping at the padding, or at the content column plus it.
///
/// **It reads one line's rows, not the whole frame**, and that is the whole
/// difficulty. The caret is a *quad*, and quads are not subject to a text
/// area's bounds at all; it is drawn at the content column's left edge, so
/// a frame whose text had been clipped away entirely still had ink exactly
/// where this test looks for it. Scanning the full height measured the
/// caret and called it the text — a proxy agreeing with its target on
/// everything examined. Line three has glyphs and no caret.
#[test]
fn the_content_starts_where_the_compositor_says_it_does() {
    /// A line far enough down to be clear of the caret's row, and well
    /// inside the composed viewport.
    const PROBE_LINE: usize = 3;

    let gpu = gpu();
    let editor = editor();

    for inset in [0.0, SIDEBAR_WIDTH] {
        let mut compositor = compositor(&gpu);
        compositor.set_theme(loud_gutter_theme());
        compositor.set_left_inset(inset);
        let frame = frame(&mut compositor, &editor, &gpu);
        let pixels = read_pixels(&gpu, &frame);
        let page = page(&pixels);

        let Some((_, top)) =
            compositor.position_to_pixel(&editor, editor.fold_state(), 0.0, PROBE_LINE, 0)
        else {
            die("the probe line was not placed");
        };
        let rows =
            pixel_to_index(top)..pixel_to_index(top + compositor.line_height()).min(HEIGHT_USIZE);
        if rows.is_empty() {
            die("the probe line's rows fall outside the frame");
        }

        let edge = compositor.content_left_edge(60);
        let first = pixel_to_index(edge).min(WIDTH_USIZE);
        let cell = compositor.char_width();

        // Not `die` here: a probe line that draws nothing is the defect this
        // test is for — a clip that swallowed the whole line — and it has to
        // be reported as a failing assertion rather than as the harness
        // giving up, or the run reports an aborted process instead of a
        // named test.
        let ink = (first..WIDTH_USIZE)
            .find(|&column| rows.clone().any(|row| inked(&pixels, page, column, row)));
        assert!(
            ink.is_some(),
            "under an inset of {inset} the content's line {PROBE_LINE} drew nothing at all \
             right of {edge}"
        );
        let Some(ink) = ink else { return };

        let gap = index_to_f32(ink) - edge;
        assert!(
            gap < cell,
            "under an inset of {inset} the content's first ink is at {ink}, {gap} pixels \
             right of the column edge the compositor reports at {edge} — more than the \
             {cell}-pixel cell it should start inside"
        );
    }
}

/// The default reserves nothing.
///
/// Every face that draws no chrome beside the text — the web face, the
/// desktop face today, the screenshot harness, the benches — must be
/// pixel-identical to before this existed.
#[test]
fn the_default_left_inset_reserves_nothing() {
    let gpu = gpu();
    let compositor = compositor(&gpu);

    assert!(compositor.left_inset().abs() < f32::EPSILON);
}

/// A nonsense inset is refused rather than stored.
///
/// Negative would draw the gutter off the left of the window, and NaN would
/// poison every comparison downstream of it. Both are rejected at the setter
/// so no consumer has to guard.
#[test]
fn a_nonsense_left_inset_leaves_the_previous_one_in_place() {
    let gpu = gpu();
    let mut compositor = compositor(&gpu);
    compositor.set_left_inset(SIDEBAR_WIDTH);

    compositor.set_left_inset(-1.0);
    assert!((compositor.left_inset() - SIDEBAR_WIDTH).abs() < f32::EPSILON);

    compositor.set_left_inset(f32::NAN);
    assert!((compositor.left_inset() - SIDEBAR_WIDTH).abs() < f32::EPSILON);

    compositor.set_left_inset(f32::INFINITY);
    assert!((compositor.left_inset() - SIDEBAR_WIDTH).abs() < f32::EPSILON);
}
