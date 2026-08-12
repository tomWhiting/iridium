//! What the pointer does *inside* a panel: press a row, spin the wheel over a
//! list, rest on a row.
//!
//! Tom, 12 Aug 2026, on the shipped sidebar: *"I can't select anything or
//! navigate really any of the menus — so the sidebar, or any of those things —
//! with the mouse. Either clicking or scrolling."*
//!
//! # The shape of what was missing
//!
//! ⚠️ **`true` from `dismiss_modal_panel` meant "the document must not see
//! this", and was read as "nothing else needs to see it either".** Those are
//! two sentences. The first was built, correctly, when a click falling through
//! an open palette was moving the caret under a panel the user was reading;
//! the second was never built at all, so a press on a panel stopped dead. One
//! value standing for two sentences, and complete-looking because nothing had
//! asked it the second question yet.
//!
//! The context menu is the exception that proves the mechanism: it has been
//! mouse-driven since it shipped, through the same
//! [`PanelGeometry::row_at`](crate::overlay::PanelGeometry::row_at) every panel
//! here now uses. Nothing needed inventing — only wiring.
//!
//! # Every verb here is the panel's own
//!
//! A press on a row reaches whatever `Enter` reaches, through the panel's own
//! `click_row`. Nothing in this module decides what a row *means*: a click that
//! resolved its own file to open, or its own command to run, would be a second
//! set of those decisions to keep in step with the first, and the day they
//! drifted the mouse and the keyboard would disagree about the same row.

use winit::event::MouseScrollDelta;

use super::state::{DesktopApp, ExplorerFocus, Flow};
use crate::mouse;
use crate::overlay::{PanelHit, PanelKind};

/// How many rows one wheel gesture moves a panel's list.
///
/// A panel's list is scrolled in whole rows, because a panel's list *is* rows:
/// there is no half-row to show. The count comes from the gesture's own pixel
/// distance divided by the row pitch the panel was **painted** at, so a
/// trackpad's fine-grained delta moves a proportional number of rows and a
/// wheel notch moves the same distance it would in the document.
///
/// At least one row for any gesture that was not a rounding error: a scroll
/// that reported movement and produced none reads as a dead panel.
fn rows_for(delta_y: f32, line_height: f32) -> isize {
    if line_height <= 0.0 || !delta_y.is_finite() {
        return 0;
    }
    let rows = delta_y / line_height;
    let magnitude = rows.abs();
    if magnitude < 0.05 {
        return 0;
    }
    // Saturating at the cast rather than wrapping: a delta no input device can
    // report should stop at the end of the list, not at the other end of it.
    let steps = crate::units::pixel_to_index(magnitude.max(1.0));
    let steps = isize::try_from(steps).unwrap_or(isize::MAX);
    if rows < 0.0 { -steps } else { steps }
}

impl DesktopApp {
    /// Spends a press on the panel under the pointer, reporting whether it was
    /// spent.
    ///
    /// `None` when the pointer is not on any panel the last frame painted — the
    /// press then carries on down [`pointer_pressed`](Self::pointer_pressed)'s
    /// ladder to the tab strip and the document.
    ///
    /// A press on a panel's *padding* is spent and does nothing: it is on the
    /// panel, so it must not reach the document, and it is on no row, so there
    /// is nothing for it to run.
    pub(super) fn panel_press(&mut self) -> Option<Flow> {
        let (x, y) = self.pointer.position();
        let hit = self.painted.hit(x, y)?;
        // A message describes the input before this one.
        self.message = None;
        let Some(row) = hit.row else {
            return Some(Flow::Running);
        };
        Some(self.press_panel_row(&hit, row))
    }

    /// Runs the panel's own verb for row `row`.
    fn press_panel_row(&mut self, hit: &PanelHit, row: usize) -> Flow {
        match hit.kind {
            PanelKind::Explorer => {
                // ⭐ Clicking a sidebar is how you start driving it. Without
                // this the row opens and the next arrow key still walks the
                // document, which is the state that reads as "the panel took
                // my click and then ignored me".
                self.explorer_focus = ExplorerFocus::Panel;
                let Some(explorer) = self.explorer.as_mut() else {
                    return Flow::Running;
                };
                let outcome = explorer.click_row(row);
                self.request_redraw();
                self.apply_explorer_outcome(outcome)
            },
            PanelKind::Palette => {
                let Some(editor) = self.workspace.active_editor() else {
                    return Flow::Running;
                };
                let outcome = self.palette.click_row(row, editor, &self.mru);
                self.request_redraw();
                self.apply_palette_outcome(outcome)
            },
            PanelKind::History => {
                let Some(editor) = self.workspace.active_editor() else {
                    return Flow::Running;
                };
                let outcome = self.history.click_row(row, editor);
                self.request_redraw();
                self.apply_history_outcome(outcome)
            },
            PanelKind::Search => {
                self.search.click_row(row, hit.rows);
                self.request_redraw();
                Flow::Running
            },
            // The menu is spent before this ladder is reached — it is modal,
            // and a press anywhere while it is up belongs to it, including the
            // press that dismisses it by landing somewhere else entirely.
            // See [`DesktopApp::menu_click`].
            PanelKind::Menu => Flow::Running,
        }
    }

    /// Spends a wheel gesture on the panel under the pointer, reporting
    /// whether it was spent.
    ///
    /// ⭐ **The panel's own row pitch, not the document's.** The gesture is
    /// converted against the pitch the panel was *painted* at, so a list whose
    /// rows are a different height from the document's text scrolls by the
    /// distance the hand moved rather than by a count borrowed from something
    /// else on screen.
    pub(super) fn panel_wheel(&mut self, delta: &MouseScrollDelta) -> bool {
        let (x, y) = self.pointer.position();
        let Some(hit) = self.painted.hit(x, y) else {
            return false;
        };
        let (_, delta_y) = mouse::wheel_delta(delta, hit.geometry.line_height);
        let rows = rows_for(delta_y, hit.geometry.line_height);
        if rows == 0 {
            // Still spent: the pointer is over the panel, and a gesture too
            // small to move a row must not fall through and move the document
            // behind it instead.
            return true;
        }
        match hit.kind {
            PanelKind::Explorer => {
                if let Some(explorer) = self.explorer.as_mut() {
                    explorer.scroll_rows(rows);
                }
            },
            PanelKind::Palette => self.palette.scroll_rows(rows),
            PanelKind::History => self.history.scroll_rows(rows),
            // Neither has a list to move: the search panel is two fields, and
            // the menu is short by construction. The gesture is still spent,
            // because the document under them must not move either.
            PanelKind::Search | PanelKind::Menu => {},
        }
        self.request_redraw();
        true
    }

    /// Records the panel row the pointer is resting on, repainting only when
    /// the highlight actually moved.
    ///
    /// The context menu is not included: it steers with the pointer through
    /// [`hover_menu`](Self::hover_menu), where hovering *is* selecting, and
    /// a second faint band under its own highlight would say nothing.
    pub(super) fn panel_hover(&mut self) {
        let (x, y) = self.pointer.position();
        let hovered = self
            .painted
            .hit(x, y)
            .filter(|hit| hit.kind != PanelKind::Menu)
            .and_then(|hit| hit.row.map(|row| (hit.kind, row)));
        if hovered == self.hover {
            return;
        }
        self.hover = hovered;
        self.request_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::rows_for;

    #[test]
    fn a_gesture_of_one_row_moves_one_row() {
        assert_eq!(rows_for(20.0, 20.0), 1);
        assert_eq!(rows_for(-20.0, 20.0), -1);
    }

    #[test]
    fn a_gesture_that_reported_movement_always_moves_a_row() {
        // A trackpad's first few pixels: less than a row, but the hand moved
        // and a list that did not would read as dead.
        assert_eq!(rows_for(3.0, 20.0), 1);
        assert_eq!(rows_for(-3.0, 20.0), -1);
    }

    #[test]
    fn a_rounding_error_moves_nothing() {
        assert_eq!(rows_for(0.0, 20.0), 0);
        assert_eq!(rows_for(0.4, 20.0), 0);
    }

    #[test]
    fn an_unmeasured_panel_is_not_divided_by() {
        assert_eq!(rows_for(60.0, 0.0), 0);
        assert_eq!(rows_for(f32::NAN, 20.0), 0);
        assert_eq!(rows_for(f32::INFINITY, 20.0), 0);
    }
}
