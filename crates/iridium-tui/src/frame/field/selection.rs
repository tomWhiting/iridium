//! Selection and navigation within a query, independent of document cursors.

use super::{Field, visible_text};
use crate::cell::{CellBuffer, Style};
use crate::frame::{
    line::LineLayout,
    text::{self, TextArea},
};
use core::ops::Range;
use iridium_editor::input::keyboard::motions::{word_left_byte, word_right_byte};

/// A destination in the field. Word boundaries use the shared code-word rules.
#[derive(Clone, Copy)]
pub enum Motion {
    Left,
    Right,
    Home,
    End,
    WordLeft,
    WordRight,
}

impl Field {
    pub(in crate::frame) fn selection(&self) -> Option<Range<usize>> {
        self.anchor
            .filter(|anchor| *anchor != self.caret)
            .map(|anchor| anchor.min(self.caret)..anchor.max(self.caret))
    }

    pub(in crate::frame) fn selected_text(&self) -> Option<&str> {
        self.selection().and_then(|range| self.text.get(range))
    }

    pub(in crate::frame) fn select_all(&mut self) -> bool {
        let changed = self.caret != self.text.len() || self.anchor != Some(0);
        self.anchor = Some(0);
        self.caret = self.text.len();
        changed
    }

    pub(in crate::frame) fn delete_selection(&mut self) -> bool {
        let selection = self.selection();
        self.anchor = None;
        if let Some(range) = selection {
            self.caret = range.start;
            return self.remove(range);
        }
        false
    }

    pub(in crate::frame) fn navigate(&mut self, motion: Motion, extend: bool) -> bool {
        let before = (self.caret, self.anchor);
        let collapse = if extend { None } else { self.selection() };
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = match (motion, collapse) {
            (Motion::Left, Some(range)) => range.start,
            (Motion::Right, Some(range)) => range.end,
            (Motion::Left, None) => self.cluster_before().map_or(self.caret, |r| r.start),
            (Motion::Right, None) => self.cluster_at().map_or(self.caret, |r| r.end),
            (Motion::Home, _) => 0,
            (Motion::End, _) => self.text.len(),
            (Motion::WordLeft, _) => word_left_byte(&self.text, self.caret),
            (Motion::WordRight, _) => word_right_byte(&self.text, self.caret),
        };
        before != (self.caret, self.anchor)
    }

    pub(in crate::frame) fn delete_word(&mut self, forward: bool) -> bool {
        if self.delete_selection() {
            return true;
        }
        let motion = if forward {
            Motion::WordRight
        } else {
            Motion::WordLeft
        };
        self.navigate(motion, true);
        self.delete_selection()
    }

    /// Restyles the visible cells, including complete markers for pasted controls.
    pub(in crate::frame) fn paint_selection(
        &self,
        buffer: &mut CellBuffer,
        row: usize,
        area: TextArea,
        style: Style,
    ) {
        let Some(range) = self.selection() else {
            return;
        };
        let start = LineLayout::new(&visible_text(&self.text[..range.start]), 1).width();
        let end = LineLayout::new(&visible_text(&self.text[..range.end]), 1).width();
        text::restyle_columns(buffer, row, start..end, area, |_| style);
    }
}

impl Field {
    /// Resolve a displayed cell to a raw grapheme boundary, including markers.
    pub(in crate::frame) fn point_cell(&mut self, target: usize, extend: bool) {
        let mut cell = 0;
        let mut byte = self.text.len();
        for cluster in self.layout().clusters() {
            let start = cluster.byte_index();
            let end = start + cluster.byte_count();
            if target == cell {
                byte = start;
                break;
            }
            let width = LineLayout::new(&visible_text(&self.text[start..end]), 1).width();
            if target < cell + width {
                byte = if (target - cell) * 2 < width {
                    start
                } else {
                    end
                };
                break;
            }
            cell += width;
        }
        if extend {
            self.anchor.get_or_insert(self.caret);
        } else {
            self.anchor = None;
        }
        self.caret = byte;
    }
}
