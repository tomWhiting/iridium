//! The pointer's modal ladder, and the hit test underneath it.
//!
//! [`DesktopApp::pointer_pressed`] is the ladder, in the same order as the
//! keyboard's: an open prompt swallows the click, an open context menu owns
//! it, a click *on* a panel belongs to that panel, a click that missed one
//! spends itself dismissing the modal panel, the tab strip owns anything
//! landing on chrome, and only then does the document see it. The wheel walks
//! the same order — the panel under the pointer scrolls, and the document only
//! when there is none.
//!
//! What each panel then *does* with a press or a gesture lives in
//! [`super::panel_mouse`]; this file is the order the questions are asked in.
//!
//! Every question about *where* the pointer is is answered against what the
//! last frame actually painted, never against geometry recomputed from state
//! that may have moved since.

use iridium_editor::{Editor, MouseResult, Position};
use winit::event::MouseScrollDelta;

use super::state::{DesktopApp, ExplorerFocus, ExplorerPlacement, Flow};
use crate::keys;
use crate::mouse::{self, Grid};
use crate::units::{index_to_f32, u32_to_f32};

impl DesktopApp {
    /// Records where the cursor is and extends a drag if one is under way.
    pub(super) fn pointer_moved(&mut self, x: f32, y: f32) {
        self.pointer.set_position(x, y);
        if self.menu.is_some() {
            // The one overlay whose hover *is* its selection; the document
            // underneath is not being selected while a menu is up.
            self.hover_menu();
            return;
        }
        // Every other panel highlights the row under the pointer without
        // moving its selection — see [`PanelContent::hovered`].
        self.panel_hover();
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
    /// context menu owns the click ([`Self::menu_click`]); a click *on* a
    /// panel belongs to that panel ([`Self::panel_press`]); a click that
    /// misses one while a modal panel is up is spent dismissing it
    /// ([`Self::dismiss_modal_panel`]); the tab strip, which is chrome the
    /// document does not extend under, owns any click that lands on it
    /// ([`Self::tab_strip_press`]); only then does the document see it. The
    /// wheel stays live throughout — reading the document can inform the
    /// answer.
    ///
    /// ⚠️ **`panel_press` runs before `dismiss_modal_panel`, and the order is
    /// the fix.** The dismissal step answers "is this press on a panel?" and
    /// swallows it either way, which is right for a press that missed and was
    /// the whole of what a press that *hit* used to get. Asking the panel
    /// first is what turns the second half of that answer into something.
    pub(super) fn pointer_pressed(&mut self) -> Flow {
        if self.prompt.is_some() {
            return Flow::Running;
        }
        if self.menu.is_some() {
            return self.menu_click();
        }
        if let Some(flow) = self.panel_press() {
            return flow;
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

    /// Spends a press that **missed** every painted panel on dismissing the
    /// modal one, reporting whether it was spent.
    ///
    /// ⚠️ **Only reached for a press that missed.** A press landing on a panel
    /// is taken by [`panel_press`](Self::panel_press) one rung earlier, so
    /// every path through here is answering "the user pointed somewhere else"
    /// — which is what makes the dismissal unconditional now rather than
    /// guarded. It used to hold both halves, and the half that meant *hit*
    /// returned `true` and did nothing, which is the whole of the defect Tom
    /// found: the press was correctly kept from the document and then handed
    /// to nobody.
    ///
    /// The click-through fix (D-4) is the other half and still stands: before
    /// it, a click while the palette or the undo tree was up fell straight
    /// through and moved the caret under a panel the user was still reading.
    ///
    /// The search panel is deliberately not included: it is not modal — keys
    /// it does not bind stay the host's — so clicking into the document while
    /// it is up is the shipped, wanted behaviour.
    fn dismiss_modal_panel(&mut self) -> bool {
        // ⚠️ **A sidebar is not one of the modal three, and treating it as one
        // would make the document unclickable while it is on screen.** The
        // whole placement exists to be kept open beside the code: a click in
        // the text has to reach the text, place the caret, and take the keys
        // back — which is what every editor with a file tree does, and what
        // the routing in `keyboard` already spells for `Escape`.
        let modal_explorer =
            self.explorer.is_some() && self.explorer_placement == ExplorerPlacement::Popover;
        if self.explorer.is_some() && !modal_explorer && self.explorer_focus == ExplorerFocus::Panel
        {
            self.explorer_focus = ExplorerFocus::Document;
            self.request_redraw();
        }
        if !(self.palette_open || self.history_open || modal_explorer) {
            return false;
        }
        self.palette_open = false;
        self.history_open = false;
        // ⚠️ The explorer is the one of the three that can be holding work.
        // Its rows are editable as text and applied to the filesystem, and a
        // click on the document is not a decision to throw a buffer full of
        // renames away — so a dirty panel stays up and keeps its dismissal for
        // the keys that mean it.
        if modal_explorer
            && !self
                .explorer
                .as_ref()
                .is_some_and(crate::file_tree::FileExplorer::has_unapplied_edits)
        {
            self.explorer = None;
            self.sync_left_inset();
        }
        self.request_redraw();
        true
    }

    /// Whether the pointer is on the tab strip the last frame actually
    /// painted.
    pub(super) fn pointer_is_on_the_tab_strip(&self) -> bool {
        let (x, y) = self.pointer.position();
        self.painted.is_on_the_tab_strip(x, y)
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
    ///
    /// ⚠️ **The panel under the pointer is asked first, and takes the gesture
    /// whether or not it had anywhere to go.** This used to scroll the
    /// document unconditionally, so spinning the wheel over a full-height
    /// sidebar scrolled the text *behind* it — the one thing on screen the
    /// user was demonstrably not pointing at.
    ///
    /// The document's own conversion needs the compositor's line height, and a
    /// session with no window has none. A pixel delta — what a trackpad
    /// reports — is already physical and is honoured regardless; a line delta
    /// resolves to nothing, which is the honest answer for "how many rows is
    /// that" asked before a font has measured.
    pub(super) fn wheel(&mut self, delta: &MouseScrollDelta) {
        if self.panel_wheel(delta) {
            return;
        }
        let line_height = self
            .shell
            .as_ref()
            .map_or(0.0, |shell| shell.compositor.line_height());
        let (_, delta_y) = mouse::wheel_delta(delta, line_height);
        self.scroll_by(delta_y);
    }
}
