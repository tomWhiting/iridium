//! Tabs: closing them, keeping one open, and the strip that shows them.
//!
//! The strip's content and the press that lands on it live beside the close
//! that a dirty tab has to ask about, because they are one rule seen from two
//! ends: the dot the strip draws is
//! [`document_is_dirty`](DesktopApp::document_is_dirty), and so is the
//! question [`close_active_tab`](DesktopApp::close_active_tab) asks. Marking
//! a tab by one test and guarding it by another would show a dot on a tab
//! that closes without a word, or no dot on one that stops to ask.

use iridium_editor::workspace::NodeId;

use super::state::{DesktopApp, DesktopDocument, Flow, UNTITLED};
use crate::highlight::HighlightCache;
use crate::prompt::{Deed, Message, Prompt};
use crate::tab_strip::{TabHit, TabItem, TabStripContent};

impl DesktopApp {
    /// Closes the active tab, asking first when it holds unsaved changes.
    ///
    /// The kernel's `close` is about *structure* — it drops a node and, if
    /// that was the last tab onto a buffer, the buffer with it. It knows
    /// nothing about files, so left to itself it would discard an unsaved
    /// edit without a word. The question is asked here, on the same prompt
    /// strip and in the same shape as the one that guards quitting: a second
    /// confirmation shape for the same stake would be a second thing to keep
    /// right.
    ///
    /// `forced` is the answer coming back from that prompt.
    pub(super) fn close_active_tab(&mut self, forced: bool) -> Flow {
        if !forced && self.is_dirty() {
            self.prompt = Some(Prompt::confirm(
                "Unsaved changes. Close this tab without saving? (y/n)",
                Deed::CloseTab,
            ));
            self.request_redraw();
            return Flow::Running;
        }
        let Some(tab) = self.workspace.active() else {
            return Flow::Running;
        };
        if !self.workspace.close(tab) {
            return Flow::Running;
        }
        self.ensure_a_tab_is_open();
        self.after_tab_change();
        Flow::Running
    }

    /// Restores the invariant every other method here relies on: **the
    /// session always has a tab.**
    ///
    /// Closing the last one would otherwise leave a window with no document
    /// — every accessor answering `None`, every handler returning early, and
    /// a frame that paints nothing. A fresh untitled buffer is what a window
    /// with no file has always shown, so this is the state the session
    /// started in rather than a new one invented for the occasion.
    fn ensure_a_tab_is_open(&mut self) {
        if self.workspace.tab_count() > 0 {
            return;
        }
        if self
            .workspace
            .open_with(
                "",
                UNTITLED,
                None,
                DesktopDocument {
                    scroll_y: 0.0,
                    file: None,
                    syntax: HighlightCache::new(),
                },
            )
            .is_none()
        {
            // Unreachable: `open_with` refuses only a parent that is not a
            // group, and none is passed. A window with no tab cannot be
            // typed into, so it is said out loud rather than left blank.
            self.message = Some(Message::error(
                "the last tab closed and none could be opened",
            ));
        }
    }

    /// Everything that must follow the active tab changing.
    ///
    /// Three separate facts go stale at once, which is why they are named in
    /// one place: the window title is the old file's, the caret may be off
    /// screen in a document that was last scrolled somewhere else, and the
    /// frame on screen is of the tab that just left.
    pub(super) fn after_tab_change(&mut self) {
        self.refresh_title();
        self.ensure_caret_visible();
        self.request_redraw();
    }

    /// The tabs the strip should show this frame, in the workspace's own
    /// display order.
    ///
    /// `None` with nothing open — a state the session should never be in, but
    /// a band with no tabs in it is chrome that says nothing, so it is not
    /// drawn and nothing is reserved for it.
    pub(super) fn tab_strip_content(&self) -> Option<TabStripContent> {
        let active = self.workspace.active();
        let tabs: Vec<TabItem> = self
            .workspace
            .tabs()
            .into_iter()
            .filter_map(|id| {
                let node = self.workspace.node(id)?;
                Some(TabItem {
                    label: node.label().to_owned(),
                    is_active: Some(id) == active,
                    is_dirty: node
                        .document()
                        .is_some_and(|document| self.document_is_dirty(document)),
                })
            })
            .collect();
        (!tabs.is_empty()).then_some(TabStripContent { tabs })
    }

    /// Spends a press on the tab strip, reporting whether it was spent.
    ///
    /// The whole band is spent, not only the tabs on it: a press on the empty
    /// stretch to the right of the last tab is a press on chrome, and letting
    /// it through would move the caret to line zero of the document
    /// underneath — which is exactly the click-through defect the modal
    /// panels already had fixed.
    ///
    /// The placement is the one the last frame *painted*, so a press is
    /// resolved against the strip on screen rather than against one
    /// recomputed from a workspace that may have changed since.
    pub(super) fn tab_strip_press(&mut self) -> bool {
        let (x, y) = self.pointer.position();
        let Some(shell) = &self.shell else {
            return false;
        };
        let Some(layout) = shell.overlay.painted_tab_strip() else {
            return false;
        };
        if !layout.contains(x, y) {
            return false;
        }
        let hit = layout.hit(x, y);
        self.message = None;
        // The index is into the strip's content, which was built from
        // `tabs()` in the same order, so asking for it again names the same
        // node. `None` for a press on the band between two tabs.
        let Some(node) = self.tab_at(hit) else {
            return true;
        };
        // Activated before any question is asked, so the unsaved-work prompt
        // is about the tab the user can see — and answering "no" leaves them
        // looking at the file they nearly discarded.
        if self.workspace.activate(node) {
            self.after_tab_change();
        }
        if matches!(hit, Some(TabHit::Close(_))) {
            self.close_active_tab(false);
        }
        true
    }

    /// The node a press on the strip named, or `None` for a press that named
    /// no tab.
    fn tab_at(&self, hit: Option<TabHit>) -> Option<NodeId> {
        let index = match hit? {
            TabHit::Activate(index) | TabHit::Close(index) => index,
        };
        self.workspace.tabs().get(index).copied()
    }
}
