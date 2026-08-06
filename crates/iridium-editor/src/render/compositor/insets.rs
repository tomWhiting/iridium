//! The chrome a face reserves around the document, on both axes.
//!
//! Two values, held once and read by everything: the painter, both directions
//! of hit-testing, scroll-to-caret and the scroll clamp. That is the whole
//! reason they are fields rather than arguments — an offset applied in one of
//! the four and forgotten in another agrees with the truth at exactly one
//! point on the screen and is wrong everywhere else.
//!
//! They are deliberately **asymmetric** in what the face passes:
//! [`FrameCompositor::set_top_inset`] takes the chrome's height *plus* the
//! document's own breathing room, [`FrameCompositor::set_left_inset`] takes the
//! chrome's width alone. The horizontal breathing room lives on the far side
//! of the gutter, so it is not the face's to account for.

use super::state::FrameCompositor;

impl FrameCompositor {
    /// The document's own breathing room above its first line, in pixels —
    /// the inset a compositor starts at, and what a face drawing no chrome
    /// above the text leaves it at.
    ///
    /// A face that *does* draw chrome there adds its height to this and
    /// passes the sum to [`Self::set_top_inset`]; it is public so that sum is
    /// arithmetic on one named value rather than on a ten repeated in the
    /// face.
    pub const DOCUMENT_TOP_PADDING: f32 = 10.0;

    /// The pixels of window reserved above the document.
    #[must_use]
    pub const fn top_inset(&self) -> f32 {
        self.top_inset
    }

    /// The pixels of window reserved to the left of the gutter.
    #[must_use]
    pub const fn left_inset(&self) -> f32 {
        self.left_inset
    }

    /// Reserves `pixels` of window to the left of the gutter.
    ///
    /// The face passes the width of whatever chrome it draws there — a
    /// sidebar, a file tree — and every horizontal measure follows: the
    /// gutter's background and its numbers, the change bars, the content
    /// column, and both directions of hit-testing.
    ///
    /// Unlike [`Self::set_top_inset`] the face adds nothing to its own width.
    /// The document's horizontal breathing room lives on the *far* side of the
    /// gutter, so it is not the face's to account for; the whole sum on this
    /// side is the chrome's width.
    ///
    /// A negative or non-finite value is ignored rather than stored, for the
    /// same reason it is on the vertical axis: it would place the gutter off
    /// the left of the window, and rejecting it here keeps every consumer from
    /// having to guard.
    pub const fn set_left_inset(&mut self, pixels: f32) {
        if pixels >= 0.0 && pixels.is_finite() {
            self.left_inset = pixels;
        }
    }

    /// Reserves `pixels` of window above the document.
    ///
    /// The face passes the height of whatever chrome it draws there — a tab
    /// strip, a breadcrumb bar — *plus* the text's own breathing room, and
    /// every layout query follows: painting, hit-testing, scroll-to-caret
    /// and the scroll clamp. That is the whole reason it is one value rather
    /// than an argument to each: a face that remembered three of the four
    /// would have a window where clicks land a row out from where they look.
    ///
    /// A negative or non-finite value is ignored rather than stored — it
    /// would place the document above the top of the window, which is not a
    /// layout anyone asked for, and rejecting it here keeps every consumer
    /// from having to guard.
    pub const fn set_top_inset(&mut self, pixels: f32) {
        if pixels >= 0.0 && pixels.is_finite() {
            self.top_inset = pixels;
        }
    }
}
