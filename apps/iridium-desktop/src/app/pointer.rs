//! The pointer's modal ladder, and the hit test underneath it.
//!
//! [`DesktopApp::pointer_pressed`] is the ladder, in the same order as the
//! keyboard's: an open prompt swallows the click, an open context menu owns
//! it, an open modal panel spends it being dismissed, the tab strip owns
//! anything landing on chrome, and only then does the document see it. The
//! wheel stays live throughout — reading the document can inform the answer.
//!
//! Every question about *where* the pointer is is answered against what the
//! last frame actually painted, never against geometry recomputed from state
//! that may have moved since.

use iridium_editor::{Editor, MouseResult, Position};
use winit::event::MouseScrollDelta;

use super::state::{DesktopApp, Flow};
use crate::keys;
use crate::mouse::{self, Grid};
use crate::units::{index_to_f32, u32_to_f32};

impl DesktopApp {
    /// Records where the cursor is and extends a drag if one is under way.
    pub(super) fn pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer.set_position(x, y);
        if self.menu.is_some() {
            // The one overlay this face steers with the pointer; the document
            // underneath is not being selected while a menu is up.
            self.hover_menu();
            return;
        }
        if !self.pointer.is_dragging() || self.prompt.is_some() {
            return;
        }
        let Some((grid, line, column)) = self.hit_test() else {
            return;
        };
        let modifiers = keys::kernel_modifiers(self.modifiers);
        let result = {
            let Some(state) = self.workspace.active_editor().map(Editor::state) else {
                return;
            };
            self.pointer.drag(
                line,
                column,
                modifiers,
                grid,
                &state.document,
                &state.cursor,
            )
        };
        self.apply_mouse(result);
    }

    /// Handles the primary button going down.
    ///
    /// Modal like the keyboard, and in the same order: while a prompt is open
    /// a click answers nothing and edits nothing, so it is swallowed; an open
    /// context menu owns the click ([`Self::menu_click`]); an open modal panel
    /// spends it being dismissed ([`Self::dismiss_modal_panel`]); the tab
    /// strip, which is chrome the document does not extend under, owns any
    /// click that lands on it ([`Self::tab_strip_press`]); only then does the
    /// document see it. The wheel stays live throughout — reading the document
    /// can inform the answer.
    pub(super) fn pointer_pressed(&mut self) -> Flow {
        if self.prompt.is_some() {
            return Flow::Running;
        }
        if self.menu.is_some() {
            return self.menu_click();
        }
        if self.dismiss_modal_panel() {
            return Flow::Running;
        }
        if self.tab_strip_press() {
            return Flow::Running;
        }
        self.message = None;
        let Some((grid, line, column)) = self.hit_test() else {
            return Flow::Running;
        };
        let modifiers = keys::kernel_modifiers(self.modifiers);
        let result = {
            let Some(state) = self.workspace.active_editor().map(Editor::state) else {
                return Flow::Running;
            };
            self.pointer.press(
                line,
                column,
                modifiers,
                grid,
                &state.document,
                &state.cursor,
            )
        };
        self.apply_mouse(result);
        Flow::Running
    }

    /// Spends a press on the modal panel that is open, reporting whether it
    /// was spent.
    ///
    /// The click-through fix (D-4): before this, a click while the palette or
    /// the undo tree was up fell straight through to the document and moved
    /// the caret under a panel the user was still reading. A press outside the
    /// panel now dismisses it, macOS-style, and a press *on* it is swallowed
    /// and leaves it up — those panels are keyboard-driven, so there is
    /// nothing inside one for a click to do, but dismissing on a click that
    /// landed on the panel itself would be a trap. Either way the document
    /// never sees the click.
    ///
    /// The search panel is deliberately not included: it is not modal — keys
    /// it does not bind stay the host's — so clicking into the document while
    /// it is up is the shipped, wanted behaviour.
    fn dismiss_modal_panel(&mut self) -> bool {
        if !(self.palette_open || self.history_open || self.explorer.is_some()) {
            return false;
        }
        if !self.pointer_is_on_a_panel() {
            self.palette_open = false;
            self.history_open = false;
            // ⚠️ The explorer is the one of the three that can be holding work.
            // Its rows are editable as text and applied to the filesystem, and
            // a click on the document is not a decision to throw a buffer full
            // of renames away — so a dirty panel stays up and keeps its
            // dismissal for the keys that mean it.
            if !self
                .explorer
                .as_ref()
                .is_some_and(crate::file_tree::FileExplorer::has_unapplied_edits)
            {
                self.explorer = None;
            }
            // Only a dismissal changed the frame; a press on the panel itself
            // leaves the screen exactly as it was.
            self.request_redraw();
        }
        true
    }

    /// Whether the pointer is on the tab strip the last frame actually
    /// painted.
    pub(super) fn pointer_is_on_the_tab_strip(&self) -> bool {
        let (x, y) = self.pointer.position();
        self.shell.as_ref().is_some_and(|shell| {
            shell
                .overlay
                .painted_tab_strip()
                .is_some_and(|layout| layout.contains(x, y))
        })
    }

    /// Whether the pointer is on any panel the last frame actually painted.
    fn pointer_is_on_a_panel(&self) -> bool {
        let Some(shell) = &self.shell else {
            return false;
        };
        let (x, y) = self.pointer.position();
        shell
            .overlay
            .painted_panels()
            .iter()
            .flatten()
            .any(|geometry| geometry.contains(x, y))
    }

    /// Moves the caret to the cell the pointer is over, unless the pointer is
    /// inside a selection.
    ///
    /// D-6, the macOS rule: right-pressing inside a selection leaves it alone
    /// — it is what the verbs are about to act on — and right-pressing outside
    /// one moves the caret to the clicked cell, so Cut and Copy act where the
    /// user pointed rather than where the caret happened to be.
    pub(super) fn place_caret_under_pointer(&mut self) {
        let Some((_, line, column)) = self.hit_test() else {
            return;
        };
        let position = Position::new(line, column);
        let Some(state) = self.workspace.active_editor().map(Editor::state) else {
            return;
        };
        if mouse::selection_covers(&state.cursor, position) {
            return;
        }
        let result = mouse::caret_at(position, &state.document, &state.cursor);
        self.apply_mouse(result);
    }

    /// Handles the window losing focus.
    pub(super) fn blurred(&mut self) {
        // A chord left pending across a blur would eat the first keystroke
        // after the user came back; the kernel names this path explicitly.
        if let Some(editor) = self.workspace.active_editor_mut() {
            editor.abort_pending_key_sequence();
        }
        // A menu is aimed at a click that is no longer being made.
        self.menu = None;
    }

    /// Handles the primary button going up, ending any drag.
    pub(super) fn pointer_released(&mut self) {
        let Some((grid, ..)) = self.hit_test() else {
            return;
        };
        let result = {
            let Some(state) = self.workspace.active_editor().map(Editor::state) else {
                return;
            };
            self.pointer.release(grid, &state.document, &state.cursor)
        };
        self.apply_mouse(result);
    }

    /// Resolves the pointer's position to a document cell through the
    /// compositor — the honest, wrap- and fold-aware mapping.
    pub(super) fn hit_test(&self) -> Option<(Grid, usize, usize)> {
        let shell = self.shell.as_ref()?;
        let editor = self.workspace.active_editor()?;
        let scroll_y = self.workspace.active_payload()?.scroll_y;
        let (x, y) = self.pointer.position();
        let (line, column) =
            shell
                .compositor
                .pixel_to_position(editor, editor.fold_state(), scroll_y, x, y);
        let grid = Grid {
            line_height: shell.compositor.line_height(),
            width: u32_to_f32(shell.surface.width()),
            height: u32_to_f32(shell.surface.height()),
        };
        Some((grid, line, column))
    }

    /// Acts on what the kernel's mouse machinery decided, and repaints only
    /// when something changed.
    pub(super) fn apply_mouse(&mut self, result: MouseResult) {
        match result {
            MouseResult::Command(command) => {
                if let Some(editor) = self.workspace.active_editor_mut() {
                    editor.apply_command(command);
                }
                if let Some(shell) = &mut self.shell {
                    shell.compositor.reset_blink();
                    shell.window.request_redraw();
                }
            },
            MouseResult::Scroll { delta_y, .. } => self.scroll_by(delta_y),
            // Unreachable through the synthesized grid (see `crate::mouse`),
            // handled honestly in case the seam widens: the kernel named an
            // effect and the face must not drop it.
            MouseResult::ToggleFold { line } => {
                if let Some(editor) = self.workspace.active_editor_mut() {
                    editor.toggle_fold_at(line);
                }
                if let Some(shell) = &self.shell {
                    shell.window.request_redraw();
                }
            },
            MouseResult::ScrollToLine { target_line } => {
                let line_height = self
                    .shell
                    .as_ref()
                    .map_or(0.0, |shell| shell.compositor.line_height());
                if let Some(document) = self.workspace.active_payload_mut() {
                    document.scroll_y = index_to_f32(target_line) * line_height;
                }
                self.clamp_scroll();
                if let Some(shell) = &self.shell {
                    shell.window.request_redraw();
                }
            },
            MouseResult::Handled | MouseResult::Ignored => {},
        }
    }

    /// Handles a wheel or trackpad scroll, in the kernel's sign convention.
    pub(super) fn wheel(&mut self, delta: &MouseScrollDelta) {
        let Some(shell) = &self.shell else {
            return;
        };
        let (_, delta_y) = mouse::wheel_delta(delta, shell.compositor.line_height());
        self.scroll_by(delta_y);
    }
}
