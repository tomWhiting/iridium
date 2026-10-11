//! What reaches the screen: one frame, where the caret goes, and the folds and
//! scrolling that decide what is on it.
//!
//! Drawing is [`iridium_tui::frame`]'s and layout is the kernel's. What is left
//! here is the two things neither can know: the [`Chrome`] the host owns — the
//! file's name, whether it has unsaved changes, whether the search panel is
//! open — and the prompt line, which is painted over the statusline after the
//! frame rather than given a row of its own. See [`prompt`](super::prompt) for
//! why it displaces the statusline instead of shrinking the document.

use iridium_editor::{EditorState, Position, Selection};
use iridium_tui::cell::Surface;
use iridium_tui::driver::CursorState;
use iridium_tui::frame::{Chrome, Frame, Palette, Status};

use super::prompt::Message;
use super::{App, Flow};
use crate::file::TextFile;

impl App {
    /// Draws one frame, returning where the terminal's own cursor belongs.
    pub fn render(&mut self, surface: &mut Surface) -> CursorState {
        let (columns, rows) = (surface.width(), surface.height());
        if columns != self.columns || rows != self.rows {
            // The surface is the authority on how big the screen is. Agreeing
            // with it here means a resize this session never saw cannot leave
            // the kernel's viewport describing a screen that is gone.
            self.resize(columns, rows);
        }

        self.poll_mouse_scroll(std::time::Instant::now());
        self.poll_query();
        self.poll_project_search();
        // Complete reads even while a narrow screen temporarily hides the band.
        if let Some(explorer) = self.explorer.as_mut() {
            explorer.poll();
        }
        let name = self.file().map(TextFile::display_name);
        let dirty = self.is_dirty();
        let band_rows = self.band_rows();
        let sidebar_columns = self.sidebar_columns();
        let tabs = self.tab_strip();
        let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() else {
            return CursorState::Hidden;
        };
        let chrome = Chrome {
            status: Status {
                name: name.as_deref(),
                dirty,
            },
            search: self.search_open.then_some(&session.search),
            sidebar_columns,
            tab_bar: true,
        };
        let layout = session.frame.render(editor, chrome, surface.back_mut());

        if let Some(menu) = &self.completion {
            menu.paint(surface.back_mut(), layout.caret(), &Palette::from_theme(editor.get_theme()));
        }

        let mut cursor = layout
            .caret()
            .map_or(CursorState::Hidden, |cell| CursorState::At {
                row: cell.row,
                column: cell.column,
            });

        if let Some(row) = layout.status_row {
            let palette = Palette::from_theme(editor.get_theme());
            if let Some(prompt) = self.prompt.as_ref() {
                // A question owns the caret while it is open: typing goes into
                // it, and a cursor left in the document would say otherwise.
                cursor = prompt.paint(surface.back_mut(), row, &palette).map_or(
                    CursorState::Hidden,
                    |column| CursorState::At { row, column },
                );
            } else if let Some(message) = self.message.as_ref() {
                message.paint(surface.back_mut(), row, &palette);
            }
        }

        if let Some(explorer) = self
            .explorer
            .as_mut()
            .filter(|_| self.focus == crate::app::sidebar::Focus::Explorer || sidebar_columns > 0)
        {
            let styles = Palette::from_theme(editor.get_theme());
            let theme = editor.get_theme().clone();
            let explorer_cursor = explorer
                .paint(surface.back_mut(), band_rows, &theme, &styles)
                .map_or(CursorState::Hidden, |cell| CursorState::At {
                    row: cell.row,
                    column: cell.column,
                });
            if self.focus == crate::app::sidebar::Focus::Explorer {
                cursor = explorer_cursor;
            }
        }

        if self.palette_open {
            // The palette floats over everything, prompt line included, and is
            // modal — so its query field owns the caret while it is open.
            let styles = Palette::from_theme(editor.get_theme());
            cursor = self
                .palette
                .paint(surface.back_mut(), editor, &self.mru, &styles)
                .map_or(CursorState::Hidden, |cell| CursorState::At {
                    row: cell.row,
                    column: cell.column,
                });
        }

        if self.history_open {
            // The undo-tree panel has no text field, so no cell owns the
            // caret while it is open: a visible cursor would claim typing
            // goes somewhere it does not.
            let styles = Palette::from_theme(editor.get_theme());
            self.history.paint(surface.back_mut(), editor, &styles);
            cursor = CursorState::Hidden;
        }

        if let Some(row) = layout.tab_row {
            tabs.paint(
                surface.back_mut(),
                row,
                &Palette::from_theme(editor.get_theme()),
            );
        }
        if let Some(panel) = self.project_search.as_ref().filter(|p| p.open) {
            return panel.paint(surface.back_mut(), &Palette::from_theme(editor.get_theme()));
        }
        cursor
    }

    /// Columns the file explorer's band takes from the document this frame.
    ///
    /// ⚠️ **Computed every frame and written even when it is zero.** The panel
    /// refuses a band on a screen too small to hold one honestly, so the same
    /// panel answers 32 and then 0 across a resize with no key pressed; and a
    /// closed panel must actively return the columns rather than leave the last
    /// width in place. This is the terminal's half of the rule
    /// `sync_left_inset` carries in the desktop.
    pub(super) fn sidebar_columns(&self) -> usize {
        let band_rows = self.band_rows();
        self.explorer.as_ref().map_or(0, |explorer| {
            explorer.sidebar_columns(self.columns, band_rows)
        })
    }

    /// How many rows a left band may span on this screen.
    ///
    /// ⭐ **The document's own row count**, from the frame rather than
    /// recomputed here: the band is a peer of the document, taking columns
    /// where the search panel takes rows, so it covers neither the statusline
    /// nor a panel that already took its rows from the same place. A second
    /// copy of that arithmetic in this file would be a second thing to keep
    /// true, and it would be wrong the first time the panel's height changed.
    pub(super) const fn band_rows(&self) -> usize {
        iridium_tui::frame::document_rows(self.rows.saturating_sub(1), self.search_open)
    }

    /// Moves the caret to a line, counting from one.
    ///
    /// A line past the end of the document lands on the last one and says so:
    /// the request was understood and answered as closely as it can be, which
    /// is more use than refusing it.
    pub(super) fn goto(&mut self, line: usize) {
        let Some(editor) = self.workspace.active_editor() else {
            return;
        };
        let last = editor.state().document.line_count().saturating_sub(1);
        let target = line.saturating_sub(1).min(last);
        if target + 1 != line {
            self.message = Some(Message::notice(format!(
                "line {line} is past the end; went to line {}",
                target + 1
            )));
        }
        self.reveal(target);
        if let Some(editor) = self.workspace.active_editor_mut() {
            editor.set_cursor(Position::new(target, 0));
        }
        self.ensure_caret_visible();
    }

    /// Folds or unfolds the innermost region around the caret.
    ///
    /// The region *around* the caret rather than the one starting on its line:
    /// a caret is usually inside the body of what the user means to fold, and
    /// the kernel's own `FoldState::region_containing` is what decides which
    /// region that is.
    pub(super) fn toggle_fold(&mut self) -> Flow {
        let Some(editor) = self.workspace.active_editor_mut() else {
            return Flow::Running;
        };
        let line = editor.cursor().line;
        let start = editor
            .state()
            .fold_state
            .region_containing(line)
            .map(|region| region.start_line);
        let Some(start) = start else {
            self.message = Some(Message::notice("nothing here folds"));
            return Flow::Running;
        };
        editor.toggle_fold_at(start);
        self.fold_carets_to_headers();
        self.ensure_caret_visible();
        Flow::Running
    }

    /// Unfolds whatever hides `line`, so that the caret can be put on it.
    ///
    /// A line can be hidden by several nested folds. Each pass unfolds the
    /// innermost one that still hides it, and the folded set shrinks every
    /// time, so this terminates.
    fn reveal(&mut self, line: usize) {
        let Some(editor) = self.workspace.active_editor_mut() else {
            return;
        };
        while editor.is_line_hidden(line) {
            let Some(start) = innermost_folded_start(editor.state(), line) else {
                break;
            };
            if !editor.unfold_at(start) {
                break;
            }
        }
    }

    /// Writes the screen's geometry into the kernel's viewport.
    ///
    /// Needed whenever the screen changes size and whenever the search panel
    /// opens or closes, because the panel takes its rows from the document's.
    /// Where the document is scrolled to is left alone: that is kernel state,
    /// and a resize does not move it.
    pub(super) fn sync_viewport(&mut self) {
        let sidebar_columns = self.sidebar_columns();
        let Some((editor, session)) = self.workspace.active_editor_and_payload_mut() else {
            return;
        };
        let chrome = Chrome {
            status: Status::default(),
            search: self.search_open.then_some(&session.search),
            sidebar_columns,
            tab_bar: true,
        };
        Frame::sync_viewport(editor, self.columns, self.rows, chrome);
    }

    /// Follows the caret without disturbing keyboard navigation state.
    /// A pending sequence must not trigger a fold-recovery cursor jump.
    pub(super) fn ensure_caret_visible(&mut self) {
        if self
            .workspace
            .active_editor()
            .is_none_or(|editor| !editor.pending_key_sequence().is_empty())
        {
            return;
        }
        let Some(editor) = self.workspace.active_editor_mut() else {
            return;
        };
        editor.reveal_selection_heads();
        let Some(layout) = self.mouse_layout() else {
            return;
        };
        if let Some(editor) = self.workspace.active_editor_mut()
            && let Err(error) = layout.reveal_caret(editor)
        {
            self.message = Some(super::Message::error(error));
        }
    }

    /// An explicit fold moves hidden carets to headers, preserving other targets.
    /// Ordinary visibility handling unfolds instead of moving selections.
    pub(super) fn fold_carets_to_headers(&mut self) {
        let Some(editor) = self.workspace.active_editor_mut() else {
            return;
        };
        let mut cursors = editor.state().cursor.clone();
        let mut changed = false;
        for selection in cursors.all_selections_mut() {
            let line = selection.head.line;
            if editor.is_line_hidden(line) {
                let header = editor.fold_state().visible_header_for_line(line);
                *selection = Selection::collapsed(Position::new(header, 0));
                changed = true;
            }
        }
        if changed {
            editor.set_selections(cursors);
        }
    }
}

/// The start line of the innermost folded region that hides `line`.
///
/// "Innermost" is the largest start line, because a region nested inside
/// another begins after it. `None` when nothing folded covers the line — which
/// includes the line being a fold's own header, since a header is visible.
fn innermost_folded_start(state: &EditorState, line: usize) -> Option<usize> {
    state
        .fold_state
        .folded_lines()
        .filter(|&start| {
            state
                .fold_state
                .region_at(start)
                .is_some_and(|region| start < line && line <= region.end_line)
        })
        .max()
}
