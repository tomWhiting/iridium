//! What reaches the screen: one frame, where the caret goes, and the folds and
//! scrolling that decide what is on it.
//!
//! Drawing is [`iridium_tui::frame`]'s and layout is the kernel's. What is left
//! here is the two things neither can know: the [`Chrome`] the host owns — the
//! file's name, whether it has unsaved changes, whether the search panel is
//! open — and the prompt line, which is painted over the statusline after the
//! frame rather than given a row of its own. See [`prompt`](super::prompt) for
//! why it displaces the statusline instead of shrinking the document.

use iridium_editor::{EditorState, Position};
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

        // ⚠️ Collected here, on the frame, like the explorer's listings: the
        // parser thread wakes the loop when a parse lands, and taking it is
        // what puts the structure it found on screen. Never waits.
        self.editor.poll_syntax();

        let name = self.file.as_ref().map(TextFile::display_name);
        let dirty = self.is_dirty();
        let band_rows = self.band_rows();
        let chrome = Chrome {
            status: Status {
                name: name.as_deref(),
                dirty,
            },
            search: self.search_open.then_some(&self.search),
            sidebar_columns: self.sidebar_columns(),
        };
        let layout = self.frame.render(&self.editor, chrome, surface.back_mut());

        let mut cursor = layout
            .caret()
            .map_or(CursorState::Hidden, |cell| CursorState::At {
                row: cell.row,
                column: cell.column,
            });

        if let Some(row) = layout.status_row {
            let palette = Palette::from_theme(self.editor.get_theme());
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

        if self.palette_open {
            // The palette floats over everything, prompt line included, and is
            // modal — so its query field owns the caret while it is open.
            let styles = Palette::from_theme(self.editor.get_theme());
            cursor = self
                .palette
                .paint(surface.back_mut(), &self.editor, &self.mru, &styles)
                .map_or(CursorState::Hidden, |cell| CursorState::At {
                    row: cell.row,
                    column: cell.column,
                });
        }

        if let Some(explorer) = self.explorer.as_mut() {
            // ⚠️ Polled here, on the frame: the reader thread wakes the loop
            // when a listing lands, and this is where it is collected.
            explorer.poll();
            let styles = Palette::from_theme(self.editor.get_theme());
            let theme = self.editor.get_theme().clone();
            cursor = explorer
                .paint(surface.back_mut(), band_rows, &theme, &styles)
                .map_or(CursorState::Hidden, |cell| CursorState::At {
                    row: cell.row,
                    column: cell.column,
                });
        }

        if self.search_target == super::SearchTarget::Explorer
            && let (Some((first, count)), Some(editor)) = (
                layout.search_rows,
                self.explorer
                    .as_ref()
                    .and_then(iridium_tui::frame::FileExplorerPanel::buffer_editor),
            )
        {
            // The frame drew the panel against the document; the matches and
            // the caret belong to the explorer's text it is aimed at.
            let styles = Palette::from_theme(self.editor.get_theme());
            cursor = self
                .search
                .paint(surface.back_mut(), first, count, editor, &styles)
                .map_or(CursorState::Hidden, |cell| CursorState::At {
                    row: cell.row,
                    column: cell.column,
                });
        }

        if self.history_open {
            // The undo-tree panel has no text field, so no cell owns the
            // caret while it is open: a visible cursor would claim typing
            // goes somewhere it does not.
            let styles = Palette::from_theme(self.editor.get_theme());
            self.history
                .paint(surface.back_mut(), &self.editor, &styles);
            cursor = CursorState::Hidden;
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
        iridium_tui::frame::document_rows(self.rows, self.search_open)
    }

    /// Moves the caret to a line, counting from one.
    ///
    /// A line past the end of the document lands on the last one and says so:
    /// the request was understood and answered as closely as it can be, which
    /// is more use than refusing it.
    pub(super) fn goto(&mut self, line: usize) {
        let last = self.editor.state().document.line_count().saturating_sub(1);
        let target = line.saturating_sub(1).min(last);
        if target + 1 != line {
            self.message = Some(Message::notice(format!(
                "line {line} is past the end; went to line {}",
                target + 1
            )));
        }
        self.reveal(target);
        self.editor.set_cursor(Position::new(target, 0));
        // Asked for by line, so shown even when the caret was already there.
        self.reveal_caret();
    }

    /// Folds or unfolds the innermost region around the caret.
    ///
    /// The region *around* the caret rather than the one starting on its line:
    /// a caret is usually inside the body of what the user means to fold, and
    /// the kernel's own `FoldState::region_containing` is what decides which
    /// region that is.
    pub(super) fn toggle_fold(&mut self) -> Flow {
        // Which region surrounds the caret is structure, and the parse that
        // knows it may still be running; a fold is worth waiting for.
        self.editor.settle_syntax();
        let line = self.editor.cursor().line;
        let start = self
            .editor
            .state()
            .fold_state
            .region_containing(line)
            .map(|region| region.start_line);
        let Some(start) = start else {
            self.message = Some(Message::notice("nothing here folds"));
            return Flow::Running;
        };
        self.editor.toggle_fold_at(start);
        self.reveal_caret();
        Flow::Running
    }

    /// Unfolds whatever hides `line`, so that the caret can be put on it.
    ///
    /// A line can be hidden by several nested folds. Each pass unfolds the
    /// innermost one that still hides it, and the folded set shrinks every
    /// time, so this terminates.
    fn reveal(&mut self, line: usize) {
        while self.editor.is_line_hidden(line) {
            let Some(start) = innermost_folded_start(self.editor.state(), line) else {
                break;
            };
            if !self.editor.unfold_at(start) {
                break;
            }
        }
    }

    /// Writes the screen's geometry into the kernel's viewport.
    ///
    /// Needed whenever the screen changes size and whenever the search panel
    /// opens or closes, because the panel takes its rows from the document's.
    /// Where the document is scrolled to is left alone, unless the caret was on
    /// screen and the new size would hide it: that is the kernel's rule.
    pub(super) fn sync_viewport(&mut self) {
        let name = self.file.as_ref().map(TextFile::display_name);
        let dirty = self.is_dirty();
        let chrome = Chrome {
            status: Status {
                name: name.as_deref(),
                dirty,
            },
            search: self.search_open.then_some(&self.search),
            sidebar_columns: self.sidebar_columns(),
        };
        Frame::sync_viewport(&mut self.editor, self.columns, self.rows, chrome);
    }

    /// Brings the view to the caret if the caret or the text moved since the
    /// view last followed it. What every input ends with.
    ///
    /// A wheel scroll that left the caret off screen is therefore kept until
    /// the next motion or edit; see the `app` module documentation.
    pub(super) fn ensure_caret_visible(&mut self) {
        self.lift_caret_out_of_a_fold();
        self.editor.follow_caret();
    }

    /// Brings the view to the caret whatever happened before: for verbs that
    /// are about the caret's line, such as go-to-line and the folds.
    pub(super) fn reveal_caret(&mut self) {
        self.lift_caret_out_of_a_fold();
        self.editor.reveal_caret();
    }

    /// Moves a caret that a fold has hidden onto that fold's header line.
    ///
    /// Folding the region the caret is in leaves it on a line nobody can see,
    /// and every later frame would then ask the viewport to scroll to a line
    /// that is not there — paying the cost above on every keystroke. The header
    /// is what the fold put on screen in its place, and moving to it is a
    /// kernel `set_cursor` like any other.
    fn lift_caret_out_of_a_fold(&mut self) {
        // `set_cursor` abandons a half-typed chord; a pending chord has not
        // moved the caret, so there is nothing to lift yet.
        if !self.editor.pending_key_sequence().is_empty() {
            return;
        }
        let head = self.editor.cursor();
        let state = self.editor.state();
        if !state.fold_state.is_line_hidden(head.line) {
            return;
        }
        let Some(start) = innermost_folded_start(state, head.line) else {
            return;
        };
        self.editor.set_cursor(Position::new(start, 0));
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
