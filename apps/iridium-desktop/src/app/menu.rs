//! The right-click context menu: opening it, steering it, and spending a
//! click on it.
//!
//! Modal while it is up — it holds key focus, and the click that leaves it is
//! spent leaving it. Its verbs reach the document through
//! [`run_chosen_command`](DesktopApp::run_chosen_command), the same
//! kernel-first, command-sourced, undoable path a chord and a palette entry
//! take; a second dispatch would be a second set of semantics to keep in step.

use iridium_editor::KeyEvent;

use super::state::{DesktopApp, Flow};
use crate::context_menu::{ContextMenu, MenuOutcome};
use crate::overlay::{PanelGeometry, PanelKind};

impl DesktopApp {
    /// Handles the secondary button going down: the context menu.
    ///
    /// Only the secondary button opens it (D-3); Ctrl+click keeps the
    /// add-cursor meaning it shipped with. A press while a prompt or a modal
    /// panel is up is spent on that panel and opens nothing — modal means
    /// modal — and a press on the tab strip opens nothing either: the menu's
    /// verbs act on the document's selection, and the strip is not the
    /// document. (A menu of tab verbs is a separate thing to design, not a
    /// document menu hung from a place it does not belong.)
    pub(super) fn secondary_pressed(&mut self) {
        if self.prompt.is_some() || self.palette_open || self.history_open {
            return;
        }
        if self.pointer_is_on_the_tab_strip() {
            return;
        }
        // A message describes the input before this one.
        self.message = None;
        self.place_caret_under_pointer();
        let (x, y) = self.pointer.position();
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };
        self.menu = Some(ContextMenu::open(editor, x, y));
        if let Some(shell) = &mut self.shell {
            shell.compositor.reset_blink();
            shell.window.request_redraw();
        }
    }

    /// Hands a key to the open context menu and acts on the outcome.
    pub(super) fn drive_menu(&mut self, event: &KeyEvent) -> Flow {
        let Some(menu) = self.menu.as_mut() else {
            return Flow::Running;
        };
        match menu.handle_key(event) {
            MenuOutcome::Handled => Flow::Running,
            MenuOutcome::Closed => {
                self.menu = None;
                Flow::Running
            },
            MenuOutcome::Run(command) => {
                self.menu = None;
                self.run_chosen_command(&command)
            },
        }
    }

    /// Spends a press on the open context menu: a row runs, the padding does
    /// nothing, and anything outside dismisses.
    pub(super) fn menu_click(&mut self) -> Flow {
        let (x, y) = self.pointer.position();
        let row = self
            .painted_menu()
            .filter(|(_, geometry)| geometry.contains(x, y))
            .map(|(menu, geometry)| geometry.row_at(x, y, menu.rows()));
        match row {
            None => {
                self.menu = None;
                self.request_redraw();
                Flow::Running
            },
            // On the menu, but on its padding: nothing runs, nothing changes.
            Some(None) => Flow::Running,
            Some(Some(row)) => match self.menu.as_mut().and_then(|menu| menu.activate(row)) {
                Some(command) => {
                    self.menu = None;
                    let flow = self.run_chosen_command(&command);
                    self.request_redraw();
                    flow
                },
                // A separator or a greyed verb: the menu stays up rather than
                // vanishing on a click that meant nothing.
                None => Flow::Running,
            },
        }
    }

    /// Highlights the menu row the pointer is over, repainting only when the
    /// highlight actually moved.
    pub(super) fn hover_menu(&mut self) {
        let (x, y) = self.pointer.position();
        let Some(row) = self
            .painted_menu()
            .filter(|(_, geometry)| geometry.contains(x, y))
            .and_then(|(menu, geometry)| geometry.row_at(x, y, menu.rows()))
        else {
            return;
        };
        if self.menu.as_mut().is_some_and(|menu| menu.hover(row)) {
            self.request_redraw();
        }
    }

    /// The open menu as the last frame painted it.
    ///
    /// ⚠️ **Asked for by name, not taken as the last entry.** It used to be
    /// read off the end of the record on the reasoning that the menu composes
    /// last — true, but true only until the day something composes after it,
    /// and the failure that day would be a menu handing its clicks to whatever
    /// had taken its place.
    ///
    /// `None` when it is closed, when the window could not hold it, or before
    /// the first frame that showed it — a press that cannot be resolved
    /// against painted chrome is honestly not on the menu.
    fn painted_menu(&self) -> Option<(&ContextMenu, PanelGeometry)> {
        let menu = self.menu.as_ref()?;
        let geometry = self.painted.geometry_of(PanelKind::Menu)?;
        Some((menu, geometry))
    }
}
