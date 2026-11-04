//! Moving the viewport, and nothing else.
//!
//! Every method here writes `viewport.first_line` (a visual line, so folds are
//! counted once) and leaves the keyboard handler alone: the sticky column, the
//! multi-cursor addition order and a half-typed chord all survive a scroll.
//! [`Editor::state_mut`] cannot promise that, which is why scrolling has its
//! own surface.

use super::core::{Editor, EditorEvent};
use crate::document::Position;
use crate::input::{CaretAlignment, ViewRequest};

/// Where the primary caret was, and in which text, when the view last
/// followed it. A follow with nothing changed since is free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CaretMark {
    head: Position,
    revision: u64,
    document: u64,
}

/// What the next [`ViewRequest::AlignCycle`] does, and the state it expects to
/// find. Any change to the caret, the text or the view starts the cycle over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct AlignCycle {
    mark: CaretMark,
    top: usize,
    next: CaretAlignment,
}

impl Editor {
    /// The first visual line on screen.
    #[must_use]
    pub const fn view_top(&self) -> usize {
        self.state.viewport.first_line
    }

    /// Scrolls so `top` is the first visual line, clamped to the document.
    ///
    /// Returns whether the view moved. No caret moves.
    pub fn scroll_view_to(&mut self, top: usize) -> bool {
        let top = top.min(self.max_view_top());
        let viewport = &mut self.state.viewport;
        let moved = viewport.first_line != top;
        viewport.first_line = top;
        // Whole lines only: a part-line offset left by a pixel face would
        // otherwise shift every row this view lays out.
        viewport.scroll_offset_y = 0.0;
        if moved && self.has_listeners() {
            let first_line = self.state.fold_state.visual_to_document_line(top);
            self.emit(&EditorEvent::ScrollChanged { first_line });
        }
        moved
    }

    /// Scrolls by `lines` visual lines, negative towards the top. No caret moves.
    ///
    /// Returns whether the view moved: at either end of the document it may not.
    pub fn scroll_view(&mut self, lines: isize) -> bool {
        let top = self.view_top();
        let target = if lines < 0 {
            top.saturating_sub(lines.unsigned_abs())
        } else {
            top.saturating_add(lines.unsigned_abs())
        };
        self.scroll_view_to(target)
    }

    /// Scrolls by `notches` of a mouse wheel, each worth
    /// [`EditorConfig::scroll_wheel_lines`](crate::EditorConfig::scroll_wheel_lines).
    /// Negative is towards the top. No caret moves.
    pub fn scroll_wheel(&mut self, notches: isize) -> bool {
        let step = isize::try_from(self.state.config.scroll_wheel_lines).unwrap_or(isize::MAX);
        self.scroll_view(notches.saturating_mul(step))
    }

    /// Scrolls as little as possible to put the primary caret on screen, at
    /// least [`EditorConfig::scroll_margin`](crate::EditorConfig::scroll_margin)
    /// lines from either edge where the document allows.
    ///
    /// Returns whether the view moved.
    pub fn reveal_caret(&mut self) -> bool {
        self.followed = Some(self.caret_mark());
        let rows = self.state.viewport.visible_lines;
        let Some(line) = self.caret_visual_line() else {
            return false;
        };
        if rows == 0 {
            return false;
        }
        let margin = self.effective_margin();
        let last = self.visual_line_count().saturating_sub(1);
        // The top may be no lower than this, or the margin above is lost...
        let highest = line.saturating_sub(margin);
        // ...and no higher than this, or the margin below is. At the end of the
        // document the margin shrinks rather than scrolling into nothing.
        let lowest = line
            .saturating_add(margin)
            .min(last)
            .saturating_sub(rows - 1);
        let top = self.view_top().max(lowest).min(highest);
        self.scroll_view_to(top)
    }

    /// [`Self::reveal_caret`], but only when the primary caret or the text has
    /// changed since the view last followed it.
    ///
    /// What a face calls after every input: a scroll that left the caret off
    /// screen stays put until the caret moves or the text changes, so saving or
    /// pressing a key that does nothing does not snap the view back.
    pub fn follow_caret(&mut self) -> bool {
        if self.followed == Some(self.caret_mark()) {
            return false;
        }
        self.reveal_caret()
    }

    /// Scrolls so the caret's line sits where `alignment` says. No caret moves.
    ///
    /// Returns whether the view moved.
    pub fn align_caret(&mut self, alignment: CaretAlignment) -> bool {
        self.followed = Some(self.caret_mark());
        let rows = self.state.viewport.visible_lines;
        let Some(line) = self.caret_visual_line() else {
            return false;
        };
        if rows == 0 {
            return false;
        }
        let margin = self.effective_margin();
        let last = self.visual_line_count().saturating_sub(1);
        let top = match alignment {
            CaretAlignment::Center => line.saturating_sub((rows - 1) / 2),
            CaretAlignment::Top => line.saturating_sub(margin),
            CaretAlignment::Bottom => line
                .saturating_add(margin)
                .min(last)
                .saturating_sub(rows - 1),
        };
        self.scroll_view_to(top)
    }

    /// Performs a [`ViewRequest`], returning whether the view moved.
    ///
    /// The one place a view command is carried out, whether it came from a key,
    /// the palette, or a face that drives the keyboard handler itself.
    pub fn perform_view_request(&mut self, request: ViewRequest) -> bool {
        match request {
            ViewRequest::ScrollLines(lines) => self.scroll_view(lines),
            ViewRequest::Align(alignment) => self.align_caret(alignment),
            ViewRequest::AlignCycle => self.align_cycle(),
        }
    }

    /// Sizes the viewport, leaving the keyboard handler and the scroll position
    /// alone.
    ///
    /// A caret that was on screen before is kept on screen; one the person had
    /// scrolled away from stays away. A `line_height` that is not a positive
    /// number is ignored, keeping the previous one.
    pub fn set_viewport_geometry(&mut self, width: f32, height: f32, line_height: f32) {
        let caret_was_visible = self.caret_row().is_some();
        if line_height.is_finite() && line_height > 0.0 {
            self.state.viewport.line_height = line_height;
        }
        self.state.viewport.resize(width, height);
        if caret_was_visible {
            self.reveal_caret();
        }
    }

    /// The caret's row on screen, counted from the top, or `None` when it is
    /// off screen.
    #[must_use]
    pub fn caret_row(&self) -> Option<usize> {
        let line = self.caret_visual_line()?;
        let row = line.checked_sub(self.view_top())?;
        (row < self.state.viewport.visible_lines).then_some(row)
    }

    /// Moves the view by as much as a page motion moved the caret, so the caret
    /// keeps `row`. A caret that was off screen is simply revealed.
    pub(super) fn keep_caret_row(&mut self, row: Option<usize>) {
        let rows = self.state.viewport.visible_lines;
        if let (Some(row), Some(line)) = (row, self.caret_visual_line())
            && rows > 0
        {
            let margin = self.effective_margin();
            let row = row.max(margin).min(rows - 1 - margin);
            self.scroll_view_to(line.saturating_sub(row));
        }
        self.reveal_caret();
    }

    /// Centre, then top, then bottom, each press continuing the cycle only when
    /// nothing has moved since the last one.
    fn align_cycle(&mut self) -> bool {
        let mark = self.caret_mark();
        let alignment = match self.align_cycle {
            Some(cycle) if cycle.mark == mark && cycle.top == self.view_top() => cycle.next,
            _ => CaretAlignment::Center,
        };
        let moved = self.align_caret(alignment);
        let next = match alignment {
            CaretAlignment::Center => CaretAlignment::Top,
            CaretAlignment::Top => CaretAlignment::Bottom,
            CaretAlignment::Bottom => CaretAlignment::Center,
        };
        self.align_cycle = Some(AlignCycle {
            mark,
            top: self.view_top(),
            next,
        });
        moved
    }

    /// The highest first line the view may have.
    fn max_view_top(&self) -> usize {
        let total = self.visual_line_count();
        if self.state.config.scroll_past_end {
            total.saturating_sub(1)
        } else {
            total.saturating_sub(self.state.viewport.visible_lines.max(1))
        }
    }

    /// The configured margin, capped so a small viewport can still scroll.
    fn effective_margin(&self) -> usize {
        let rows = self.state.viewport.visible_lines;
        self.state
            .config
            .scroll_margin
            .min(rows.saturating_sub(1) / 2)
    }

    /// Lines on screen if the viewport were infinitely tall: folds count once.
    fn visual_line_count(&self) -> usize {
        self.state
            .fold_state
            .visible_line_count(self.state.document.line_count())
    }

    /// The visual line the primary caret is drawn on: its own, or the header
    /// of the outermost fold hiding it.
    fn caret_visual_line(&self) -> Option<usize> {
        let fold = &self.state.fold_state;
        let line = self.state.cursor.primary.head.line;
        let shown = if fold.is_line_hidden(line) {
            fold.folded_lines()
                .filter(|&start| {
                    fold.region_at(start)
                        .is_some_and(|region| start < line && line <= region.end_line)
                })
                .min()
                .unwrap_or(line)
        } else {
            line
        };
        fold.document_to_visual_line(shown)
    }

    /// The primary caret and the text it is in, for "has anything moved".
    const fn caret_mark(&self) -> CaretMark {
        CaretMark {
            head: self.state.cursor.primary.head,
            revision: self.state.document.revision(),
            document: self.state.document.id(),
        }
    }
}
