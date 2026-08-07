//! Reading meaning out of a frame's bytes.
//!
//! Everything here works at the default frame size from [`super::gpu`], which
//! is the size both inset harnesses compose at.

use iridium_editor::render::units::pixel_to_index;

use super::gpu::{HEIGHT_USIZE, WIDTH_USIZE};

/// How far a channel must move from the page colour to count as ink.
///
/// Well above antialiasing noise and well below any real colour difference.
/// Every comparison here is between two frames of the same document under the
/// same threshold, which makes the *difference* the claim — so the exact value
/// buys nothing and costs nothing.
pub const INK: i32 = 24;

/// The page colour, sampled from the frame's bottom-right corner.
///
/// **Not** the top-left. That corner is the one the inset tests are about:
/// under a top or left inset it is the first pixel of the reserved band, so a
/// compositor that wrongly painted the gutter there would make the band's own
/// colour the reference — and hide the very defect being tested. Every line in
/// [`super::scene::numbered_document`] is far shorter than the frame is wide
/// and far above its last row, so the bottom-right pixel is page on every frame
/// these harnesses compose.
///
/// This is the *only* definition of "page" here. `top_inset.rs` used to carry
/// this reasoning in a doc comment and then sample the top-left anyway, two
/// functions further down. Both corners are page on a correct frame, so it was
/// latent rather than live — but it was the same one-decision-written-twice
/// shape, and having one function is what stops it coming back.
#[must_use]
pub fn page(pixels: &[u8]) -> [u8; 3] {
    let offset = ((HEIGHT_USIZE - 1) * WIDTH_USIZE + (WIDTH_USIZE - 1)) * 4;
    [pixels[offset], pixels[offset + 1], pixels[offset + 2]]
}

/// Whether the pixel at `(column, row)` differs from the page colour.
#[must_use]
pub fn inked(pixels: &[u8], page: [u8; 3], column: usize, row: usize) -> bool {
    let offset = (row * WIDTH_USIZE + column) * 4;
    (0..3)
        .any(|channel| (i32::from(pixels[offset + channel]) - i32::from(page[channel])).abs() > INK)
}

/// The first row carrying ink within the pixel range `[left, right)`, or
/// `None` when that column range is empty top to bottom.
///
/// The bounds are pixel coordinates rather than column indices because callers
/// have them in that form — a compositor query answers in pixels.
#[must_use]
pub fn first_inked_row(pixels: &[u8], left: f32, right: f32) -> Option<usize> {
    let page = page(pixels);
    let first = pixel_to_index(left.max(0.0)).min(WIDTH_USIZE);
    let last = pixel_to_index(right.max(0.0)).min(WIDTH_USIZE);
    (0..HEIGHT_USIZE).find(|&row| (first..last).any(|column| inked(pixels, page, column, row)))
}

/// The first column carrying ink anywhere down its height, or `None` for a
/// frame that drew nothing at all.
#[must_use]
pub fn first_inked_column(pixels: &[u8]) -> Option<usize> {
    let page = page(pixels);
    (0..WIDTH_USIZE).find(|&column| (0..HEIGHT_USIZE).any(|row| inked(pixels, page, column, row)))
}

/// How two frames differ, in the terms a failure has to be diagnosed from.
///
/// A pixel comparison that fails with nothing but `left == right` tells
/// whoever reads the log nothing at all: a one-pixel difference along a glyph
/// edge and a frame composed from the wrong document produce the same
/// message. This says how many pixels moved, where the first one is, what
/// area they cover and by how much — which is the difference between "the
/// glyph atlas packed differently" and "this is a regression".
///
/// Written because [`retained_shaping`'s custom-gutter row is flaky under box
/// load](../../../../docs/SESSION-STATE.md) and, when it fired, left nothing
/// behind to work from.
#[must_use]
pub fn describe_difference(left: &[u8], right: &[u8], width: u32) -> String {
    // The callers hold widths as `u32`, which is what the render target
    // carries; the arithmetic below indexes, so it converts once here.
    let width = usize::try_from(width).unwrap_or(usize::MAX);
    if left.len() != right.len() {
        return format!(
            "the frames are not even the same size: {} bytes against {} bytes",
            left.len(),
            right.len()
        );
    }

    let mut differing = 0_usize;
    let mut first = None;
    let (mut min_column, mut max_column) = (usize::MAX, 0_usize);
    let (mut min_row, mut max_row) = (usize::MAX, 0_usize);
    let mut worst = 0_u8;

    for (index, (here, there)) in left.chunks_exact(4).zip(right.chunks_exact(4)).enumerate() {
        if here == there {
            continue;
        }
        differing += 1;
        let (column, row) = (index % width, index / width);
        first.get_or_insert((column, row));
        min_column = min_column.min(column);
        max_column = max_column.max(column);
        min_row = min_row.min(row);
        max_row = max_row.max(row);
        for (a, b) in here.iter().zip(there) {
            worst = worst.max(a.abs_diff(*b));
        }
    }

    if differing == 0 {
        return "identical".to_owned();
    }
    let (column, row) = first.unwrap_or((0, 0));
    format!(
        "{differing} of {} pixels differ; the first at column {column}, row {row}; \
         they span columns {min_column}..={max_column} and rows {min_row}..={max_row}; \
         the largest single channel difference is {worst}",
        left.len() / 4
    )
}

/// Asserts two frames are byte-identical, and says *how* they differ if they
/// are not.
///
/// The message is only formatted when the assertion fails, so the scan costs
/// nothing on the passing path — which is every run but the one that matters.
pub fn assert_same_frame(left: &[u8], right: &[u8], width: u32, claim: &str) {
    assert!(
        left == right,
        "{claim}\n  {}",
        describe_difference(left, right, width)
    );
}

/// A frame of `count` pixels, every channel `value`.
fn flat(count: usize, value: u8) -> Vec<u8> {
    vec![value; count * 4]
}

#[test]
fn identical_frames_are_reported_as_identical() {
    let frame = flat(64, 7);
    assert_eq!(describe_difference(&frame, &frame, 8), "identical");
}

#[test]
fn one_changed_pixel_is_located_and_measured() {
    // The whole point of the helper: a failure has to say *where* and *by how
    // much*, because a glyph edge off by one and a frame composed from the
    // wrong document are the same assertion otherwise.
    let left = flat(64, 7);
    let mut right = left.clone();
    // Pixel index 19 on an 8-wide frame is column 3, row 2.
    right[19 * 4 + 1] = 30;

    let described = describe_difference(&left, &right, 8);
    assert!(described.contains("1 of 64 pixels differ"), "{described}");
    assert!(described.contains("column 3, row 2"), "{described}");
    assert!(
        described.contains("columns 3..=3") && described.contains("rows 2..=2"),
        "{described}"
    );
    assert!(
        described.contains("channel difference is 23"),
        "the magnitude is what separates a rounding difference from a \
         regression: {described}"
    );
}

#[test]
fn a_scattered_difference_reports_the_area_it_covers() {
    let left = flat(64, 0);
    let mut right = left.clone();
    for pixel in [9_usize, 54] {
        right[pixel * 4] = 255;
    }

    let described = describe_difference(&left, &right, 8);
    assert!(described.contains("2 of 64 pixels differ"), "{described}");
    assert!(described.contains("columns 1..=6"), "{described}");
    assert!(described.contains("rows 1..=6"), "{described}");
}

#[test]
fn frames_of_different_sizes_say_so_rather_than_comparing() {
    let described = describe_difference(&flat(64, 0), &flat(32, 0), 8);
    assert!(described.contains("not even the same size"), "{described}");
}
