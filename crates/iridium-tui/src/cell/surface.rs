//! The front/back pair the face renders through.
//!
//! The back buffer is where a frame is drawn, freely and in any order. The
//! front buffer is what the terminal is currently showing. The difference
//! between them is the frame's damage, and it is worked out by comparison —
//! there is no way for a renderer to say "this part changed", because a
//! renderer that says it too rarely leaves stale cells on screen and one that
//! says it too often makes the screen flicker.
//!
//! The front buffer is private and only [`Surface::commit`] can change it, so
//! it cannot drift from what was actually written.

use super::buffer::CellBuffer;
use super::color::ColorDepth;
use super::damage::Damage;

/// A front and a back cell buffer, and the terminal's colour depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Surface {
    front: CellBuffer,
    back: CellBuffer,
    depth: ColorDepth,
    invalidated: bool,
}

impl Surface {
    /// A surface of the given size, showing nothing yet.
    ///
    /// The first frame is a full repaint: the terminal's contents at start-up
    /// are whatever the shell left behind, which is not a blank screen.
    pub fn new(width: usize, height: usize, depth: ColorDepth) -> Self {
        Self {
            front: CellBuffer::new(width, height),
            back: CellBuffer::new(width, height),
            depth,
            invalidated: true,
        }
    }

    /// The number of columns.
    pub const fn width(&self) -> usize {
        self.back.width()
    }

    /// The number of rows.
    pub const fn height(&self) -> usize {
        self.back.height()
    }

    /// The colour depth damage is computed against.
    pub const fn depth(&self) -> ColorDepth {
        self.depth
    }

    /// Changes the colour depth, invalidating the screen.
    ///
    /// What is on screen was painted in the old depth, so none of it can be
    /// compared against the new one.
    pub fn set_depth(&mut self, depth: ColorDepth) {
        if self.depth != depth {
            self.depth = depth;
            self.invalidated = true;
        }
    }

    /// The buffer being drawn into.
    pub const fn back(&self) -> &CellBuffer {
        &self.back
    }

    /// The buffer being drawn into, mutably. This is where a frame is built.
    pub const fn back_mut(&mut self) -> &mut CellBuffer {
        &mut self.back
    }

    /// What the terminal is currently showing.
    pub const fn front(&self) -> &CellBuffer {
        &self.front
    }

    /// Resizes the surface, keeping whatever content still fits.
    ///
    /// A resize invalidates the screen: the terminal reflows, scrolls or
    /// truncates its own contents when it changes size, in ways no diff can
    /// predict, so the next frame must be painted in full.
    pub fn resize(&mut self, width: usize, height: usize) {
        if width == self.back.width() && height == self.back.height() {
            return;
        }
        self.back.resize(width, height);
        self.invalidated = true;
    }

    /// Declares that nothing on screen can be trusted.
    ///
    /// This is the only way to declare damage, and it can only declare all of
    /// it. It is for the moments when the terminal's contents changed without
    /// going through this surface: a resume from suspend, another process
    /// writing to the same terminal, a mode change.
    pub const fn invalidate(&mut self) {
        self.invalidated = true;
    }

    /// Whether the next frame will be a full repaint.
    pub const fn is_invalidated(&self) -> bool {
        self.invalidated
    }

    /// What must be written to bring the terminal up to date.
    ///
    /// This does not change the surface: the front buffer is only updated by
    /// [`Surface::commit`], which the caller runs once the writes have
    /// actually reached the terminal. A write that fails therefore leaves the
    /// surface still knowing that the screen is out of date.
    pub fn damage(&self) -> Damage {
        if self.invalidated {
            Damage::full(&self.back, self.depth)
        } else {
            Damage::compute(&self.front, &self.back, self.depth)
        }
    }

    /// Records that the last damage was written to the terminal.
    ///
    /// Call this only after the writes have succeeded.
    pub fn commit(&mut self) {
        self.front.clone_from(&self.back);
        self.invalidated = false;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::super::style::Style;
    use super::*;

    #[test]
    fn the_first_frame_is_a_full_repaint() {
        let surface = Surface::new(4, 2, ColorDepth::TrueColor);
        let damage = surface.damage();
        assert!(surface.is_invalidated());
        assert!(damage.requires_clear());
        assert_eq!(damage.runs().len(), 2);
    }

    #[test]
    fn a_committed_frame_leaves_nothing_to_do() {
        let mut surface = Surface::new(4, 2, ColorDepth::TrueColor);
        surface.back_mut().set_str(0, 0, "ab", Style::DEFAULT);
        surface.commit();
        assert!(!surface.is_invalidated());
        assert!(surface.damage().is_empty());
        assert_eq!(surface.front(), surface.back());
    }

    #[test]
    fn damage_does_not_change_the_surface() {
        let mut surface = Surface::new(4, 1, ColorDepth::TrueColor);
        surface.commit();
        surface.back_mut().set_str(0, 0, "x", Style::DEFAULT);
        let first = surface.damage();
        let second = surface.damage();
        assert_eq!(first, second, "asking twice must give the same answer");
        assert!(!first.is_empty());
    }

    #[test]
    fn an_uncommitted_frame_is_still_owed() {
        // A driver whose write failed must not lose the damage.
        let mut surface = Surface::new(4, 1, ColorDepth::TrueColor);
        surface.commit();
        surface.back_mut().set_str(0, 0, "x", Style::DEFAULT);
        let damage = surface.damage();
        assert!(!damage.is_empty());
        assert_eq!(surface.damage(), damage);
        surface.commit();
        assert!(surface.damage().is_empty());
    }

    #[test]
    fn only_the_change_is_repainted() {
        let mut surface = Surface::new(6, 1, ColorDepth::TrueColor);
        surface.back_mut().set_str(0, 0, "abcdef", Style::DEFAULT);
        surface.commit();
        surface.back_mut().set_str(3, 0, "X", Style::DEFAULT);
        let damage = surface.damage();
        assert!(!damage.requires_clear());
        assert_eq!(damage.runs().len(), 1);
        assert_eq!(damage.runs()[0].column(), 3);
        assert_eq!(damage.runs()[0].text(), "X");
    }

    #[test]
    fn a_resize_forces_a_full_repaint() {
        let mut surface = Surface::new(4, 1, ColorDepth::TrueColor);
        surface.back_mut().set_str(0, 0, "abcd", Style::DEFAULT);
        surface.commit();
        surface.resize(6, 2);
        assert!(surface.is_invalidated());
        assert_eq!(surface.width(), 6);
        assert_eq!(surface.height(), 2);
        let damage = surface.damage();
        assert!(damage.requires_clear());
        assert_eq!(damage.runs().len(), 2);
        assert_eq!(damage.runs()[0].text(), "abcd  ");
        surface.commit();
        assert!(surface.damage().is_empty());
    }

    #[test]
    fn a_resize_to_the_same_size_is_not_a_repaint() {
        let mut surface = Surface::new(4, 1, ColorDepth::TrueColor);
        surface.commit();
        surface.resize(4, 1);
        assert!(!surface.is_invalidated());
        assert!(surface.damage().is_empty());
    }

    #[test]
    fn invalidation_forces_a_full_repaint() {
        let mut surface = Surface::new(4, 1, ColorDepth::TrueColor);
        surface.commit();
        assert!(surface.damage().is_empty());
        surface.invalidate();
        let damage = surface.damage();
        assert!(damage.requires_clear());
        assert_eq!(damage.runs().len(), 1);
    }

    #[test]
    fn changing_the_colour_depth_forces_a_full_repaint() {
        let mut surface = Surface::new(4, 1, ColorDepth::TrueColor);
        surface.commit();
        surface.set_depth(ColorDepth::TrueColor);
        assert!(!surface.is_invalidated(), "the same depth changes nothing");
        surface.set_depth(ColorDepth::Ansi16);
        assert_eq!(surface.depth(), ColorDepth::Ansi16);
        assert!(surface.damage().requires_clear());
    }
}
