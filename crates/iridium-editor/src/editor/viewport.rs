//! Host viewport changes are independent of keyboard editing intentions.
use super::Editor;
use crate::Position;
use crate::render::Viewport;

impl Editor {
    /// Replaces only the viewport's geometry and scroll position.
    ///
    /// Resizing and scrolling do not change document/cursor identity. Unlike
    /// [`Self::state_mut`], this preserves sticky vertical columns, cursor
    /// addition order and pending key sequences. The host supplies units
    /// appropriate to its renderer (terminal cells or desktop/browser pixels).
    pub const fn set_viewport(&mut self, viewport: Viewport) {
        self.state.viewport = viewport;
    }

    /// Scrolls just enough to show `position`, accounting for folded lines.
    /// The cursor, selection history and keyboard state are unchanged.
    pub fn reveal_in_viewport(&mut self, position: Position) {
        self.state
            .viewport
            .scroll_to_position_with_folds(position, &self.state.fold_state);
    }
}
