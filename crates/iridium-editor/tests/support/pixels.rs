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
