//! The search panel aimed at the file explorer's listing while it is edited
//! as text.
//!
//! It is a panel of its own, not the document's: the document keeps its query
//! for when the person goes back to it, and the two are never open together.
//! The frame reserves the same rows for either, so the document gives up the
//! same space; this module repaints those rows with the listing's matches,
//! which the explorer marks in its own rows.

use iridium_editor::{KeyEvent, Keymap};
use iridium_tui::cell::CellBuffer;
use iridium_tui::frame::{
    CellPosition, FileExplorerPanel, FrameLayout, Palette, SearchCut, SearchOutcome, SearchOverlay,
};

use super::clipboard::CutTarget;
use super::{App, Flow, Message};

/// The panel and whether it is on screen, kept together because neither means
/// anything without the other.
pub(super) struct ListingSearch {
    /// The panel. Kept while closed, so reopening it finds the last query.
    pub(super) overlay: SearchOverlay,
    /// Whether the panel is on screen.
    pub(super) open: bool,
}

impl ListingSearch {
    /// A closed panel with the user's search keys.
    pub(super) fn new(keys: &Keymap) -> Self {
        Self {
            overlay: SearchOverlay::new(keys),
            open: false,
        }
    }

    /// The panel the frame reserves rows for, when it is on screen.
    pub(super) fn panel(&self) -> Option<&SearchOverlay> {
        self.open.then_some(&self.overlay)
    }
}

impl App {
    /// Opens the panel on the explorer's listing, taking the panel from the
    /// document if it was searching there. Does nothing unless the listing is
    /// being edited as text: there is nothing else in it to search.
    pub(super) fn open_listing_search(&mut self) {
        if self.search_open {
            if let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() {
                session.search.close(editor);
            }
            self.search_open = false;
        }
        let search = &mut self.listing_search.overlay;
        self.listing_search.open = self
            .explorer
            .as_mut()
            .and_then(|explorer| explorer.with_buffer_editor(|editor| search.open(editor)))
            .is_some();
        // The panel takes rows from the document, so the kernel's viewport
        // shrinks with it.
        self.sync_viewport();
    }

    /// Closes the panel on the explorer's listing, if it is open.
    pub(super) fn close_listing_search(&mut self) {
        if !self.listing_search.open {
            return;
        }
        let search = &mut self.listing_search.overlay;
        if let Some(explorer) = self.explorer.as_mut() {
            explorer.with_buffer_editor(|editor| search.close(editor));
        }
        self.listing_search.open = false;
        self.sync_viewport();
    }

    /// Closes a panel whose listing is no longer being edited: the explorer
    /// left its text, or closed.
    pub(super) fn drop_orphaned_listing_search(&mut self) {
        let orphaned = self.listing_search.open
            && self
                .explorer
                .as_ref()
                .is_none_or(|explorer| explorer.buffer_editor().is_none());
        if orphaned {
            self.listing_search.open = false;
            self.sync_viewport();
        }
    }

    /// Hands a key to the panel on the explorer's listing, or `None` when
    /// there is none or it leaves the key to the explorer.
    pub(super) fn listing_search_key(&mut self, event: &KeyEvent) -> Option<Flow> {
        if !self.listing_search.open {
            return None;
        }
        let search = &mut self.listing_search.overlay;
        let outcome = self.explorer.as_mut().and_then(|explorer| {
            explorer.with_buffer_editor(|editor| search.handle_key(event, editor))
        });
        let Some(outcome) = outcome else {
            self.drop_orphaned_listing_search();
            return None;
        };
        match outcome {
            SearchOutcome::Ignored => return None,
            SearchOutcome::Handled | SearchOutcome::Replaced(_) => {},
            SearchOutcome::CutRequested(cut) => {
                self.store_clipboard(
                    cut.text().to_owned(),
                    Some(CutTarget::ListingSearch(Box::new(cut))),
                );
            },
            SearchOutcome::Copied(text) => self.store_clipboard(text, None),
            SearchOutcome::PasteRequested => {
                self.clipboard(iridium_editor::ClipboardOperation::Paste);
            },
            SearchOutcome::Selected(count) => {
                self.listing_search.open = false;
                self.sync_viewport();
                self.message = Some(Message::notice(format!(
                    "{count} selections in the list — Ctrl+S applies the edit"
                )));
            },
            SearchOutcome::Closed => {
                self.listing_search.open = false;
                self.sync_viewport();
            },
        }
        Some(Flow::Running)
    }

    /// Pastes into the panel's field, answering `false` when the panel is not
    /// open on a listing being edited and the text belongs elsewhere.
    pub(super) fn paste_into_listing_search(&mut self, text: &str) -> bool {
        if !self.listing_search.open {
            return false;
        }
        let search = &mut self.listing_search.overlay;
        self.explorer
            .as_mut()
            .and_then(|explorer| explorer.with_buffer_editor(|editor| search.paste(text, editor)))
            .is_some()
    }

    /// Finishes a cut from the panel's field once the clipboard has the text.
    pub(super) fn finish_listing_search_cut(&mut self, cut: &SearchCut) -> Result<(), String> {
        if !self.listing_search.open {
            return Err("Text copied; the list's search closed, so cut was cancelled".into());
        }
        let search = &mut self.listing_search.overlay;
        self.explorer
            .as_mut()
            .and_then(|explorer| {
                explorer.with_buffer_editor(|editor| search.finish_cut(cut, editor))
            })
            .ok_or_else(|| {
                String::from("Text copied; the list is no longer edited, so cut was cancelled")
            })?
    }
}

/// Repaints the rows the frame reserved for a search panel with the panel
/// aimed at the listing, counting the listing's matches rather than the
/// document's. Returns where the panel's caret goes, or `None` when the panel
/// is not open on a listing being edited.
///
/// The frame drew the panel against the document first; this paints over
/// every one of those rows. The count of matches beyond the list's visible
/// rows is not shown: the count itself still names every match.
///
/// Takes the fields rather than the [`App`], because the frame is drawn while
/// the document's editor is borrowed from the workspace.
pub(super) fn paint_listing_search(
    search: &SearchOverlay,
    explorer: Option<&FileExplorerPanel>,
    buffer: &mut CellBuffer,
    layout: &FrameLayout,
    palette: &Palette,
) -> Option<CellPosition> {
    let (first_row, row_count) = layout.search_rows?;
    let listing = explorer?.buffer_editor()?;
    search.paint(buffer, first_row, row_count, listing, palette, 0)
}
