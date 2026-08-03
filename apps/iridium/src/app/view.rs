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

        let name = self.file.as_ref().map(TextFile::display_name);
        let dirty = self.is_dirty();
        let chrome = Chrome {
            status: Status {
                name: name.as_deref(),
                dirty,
            },
            search: self.search_open.then_some(&self.search),
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

        cursor
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
        self.ensure_caret_visible();
    }

    /// Folds or unfolds the innermost region around the caret.
    ///
    /// The region *around* the caret rather than the one starting on its line:
    /// a caret is usually inside the body of what the user means to fold, and
    /// the kernel's own `FoldState::region_containing` is what decides which
    /// region that is.
    pub(super) fn toggle_fold(&mut self) -> Flow {
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
        self.ensure_caret_visible();
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
    /// Where the document is scrolled to is left alone: that is kernel state,
    /// and a resize does not move it.
    pub(super) fn sync_viewport(&mut self) {
        let name = self.file.as_ref().map(TextFile::display_name);
        let dirty = self.is_dirty();
        let chrome = Chrome {
            status: Status {
                name: name.as_deref(),
                dirty,
            },
            search: self.search_open.then_some(&self.search),
        };
        Frame::sync_viewport(&mut self.editor, self.columns, self.rows, chrome);
    }

    /// Scrolls so the caret is on screen, as cheaply as the kernel allows.
    ///
    /// See the `app` module documentation for what the mutation at the end of
    /// this costs, and why the two guards in front of it are there: a pending
    /// key sequence means the caret has not moved and that aborting the chord
    /// would eat the next keystroke, and a caret already on screen needs
    /// nothing at all.
    pub(super) fn ensure_caret_visible(&mut self) {
        if !self.editor.pending_key_sequence().is_empty() {
            return;
        }
        self.lift_caret_out_of_a_fold();

        let head = self.editor.cursor();
        let state = self.editor.state();
        if state
            .viewport
            .is_line_visible_with_folds(head.line, &state.fold_state)
        {
            return;
        }

        let state = self.editor.state_mut();
        state
            .viewport
            .scroll_to_position_with_folds(head, &state.fold_state);
    }

    /// Moves a caret that a fold has hidden onto that fold's header line.
    ///
    /// Folding the region the caret is in leaves it on a line nobody can see,
    /// and every later frame would then ask the viewport to scroll to a line
    /// that is not there — paying the cost above on every keystroke. The header
    /// is what the fold put on screen in its place, and moving to it is a
    /// kernel `set_cursor` like any other.
    fn lift_caret_out_of_a_fold(&mut self) {
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
