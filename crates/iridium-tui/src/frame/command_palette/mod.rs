//! The command palette: a floating panel that runs any command by name.
//!
//! This is the terminal face of the kernel's `palette.open` host command — the
//! UI the kernel names but cannot draw. Until this module existed, 41 kernel
//! commands that are palette-only by design were unreachable in the terminal.
//!
//! # The engine is the kernel's and none of it is repeated here
//!
//! Nothing in this module matches, scores, ranks or remembers. Every keystroke
//! re-runs [`palette::search_text`]
//! against the kernel's own registry — the same matcher, the same recency
//! bonus, the same total order every face uses, so the same query can never put
//! a different command first here than in the GPU face. What is here is a text
//! field, a selection, a scroll window and a box.
//!
//! Recency lives in a [`CommandMru`] the *host* owns and records into after a
//! command actually runs; the panel only reads it. A panel that recorded on
//! `Enter` would remember commands whose execution then failed.
//!
//! # The panel is modal
//!
//! Every key is consumed while it is open, exactly like the prompt line: the
//! palette exists to run any command by name, and a chord falling through to
//! the document while the user is aiming at a command list would edit text they
//! are not looking at. `Escape` closes it; so does the `Ctrl+K` that opened it,
//! because a toggle is what the finger expects.
//!
//! | Key | Action |
//! |---|---|
//! | any printable | insert into the query |
//! | `Left` `Right` `Home` `End` `Backspace` `Delete` | edit the query |
//! | `Down`, `Ctrl+N` / `Up`, `Ctrl+P` | move the selection, clamping |
//! | `PageDown` / `PageUp` | hop by one windowful |
//! | `Enter` | run the selected command |
//! | `Escape`, `Ctrl+K` | close |
//!
//! The selection **clamps** at both ends rather than wrapping: wrapping
//! overshoots on key repeat, and a list whose ends are walls can be leaned on.
//!
//! # The panel floats
//!
//! Unlike the search panel it takes no rows from the document: it is painted
//! over the frame after [`Frame::render`](super::Frame::render), the way the
//! prompt paints over the statusline. The kernel's viewport is untouched, so
//! opening the palette scrolls nothing and closing it restores the exact
//! screen underneath.

mod keymap;
mod paint;
mod resolve;
mod verb;

#[cfg(test)]
mod keymap_tests;
#[cfg(test)]
mod tests;

use iridium_editor::commands::palette::{self, CommandMru};
use iridium_editor::{CommandId, Editor, KeyCode, KeyEvent, KeyPress, KeymapStack};

use self::resolve::{Resolved, types_text};
use self::verb::Verb;
use super::CellPosition;
use super::field::Field;
use super::palette::Palette;
use crate::cell::CellBuffer;

use self::keymap::default_keymap;

/// What became of a key handed to the palette.
///
/// There is no `Ignored`: the palette is modal, and a key it does not bind is
/// swallowed rather than falling through to the document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteOutcome {
    /// The palette consumed the key and stays open.
    Handled,
    /// The palette was closed without running anything.
    Closed,
    /// The palette was closed and this command should now run.
    ///
    /// Resolution happened against the same ranked search the panel painted —
    /// the kernel's order is total, so what was highlighted is what runs.
    Run(CommandId),
}

/// The command palette panel.
///
/// The host owns one of these, opens it when the kernel reports the
/// `palette.open` host command, hands it every key while it is open, and paints
/// it over the finished frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPalette {
    /// The query field.
    query: Field,
    /// The selected result, as an index into the full ranked result list.
    ///
    /// Clamped against the result count wherever it is read, because the count
    /// changes under it with every edit to the query.
    selected: usize,
    /// The first visible result: the scroll position of the list window.
    scroll: usize,
    /// How many result rows the last paint showed — the page the page keys hop.
    ///
    /// Before the first paint it is a reasonable page rather than zero, so
    /// `PageDown` on a panel that has not reached the screen yet still moves.
    window: usize,
    /// The panel's own keymap: its defaults, with the user's layer over them.
    keys: KeymapStack,
    /// The strokes of a multi-stroke sequence typed so far.
    pending: Vec<KeyPress>,
}

impl Default for CommandPalette {
    /// ⚠️ **Written out rather than derived.** A derived `Default` would give
    /// an empty [`KeymapStack`], so a palette built that way would answer no
    /// keys at all — a panel that looks constructed and is deaf.
    fn default() -> Self {
        Self::new()
    }
}

impl CommandPalette {
    /// A closed palette with an empty query.
    #[must_use]
    pub fn new() -> Self {
        Self {
            query: Field::default(),
            selected: 0,
            scroll: 0,
            window: super::panel::MAX_VISIBLE_ROWS,
            keys: KeymapStack::with_base(default_keymap()),
            pending: Vec::new(),
        }
    }

    /// Resets the panel for opening: empty query, selection at the top.
    ///
    /// The query is cleared rather than kept — the opposite of the search
    /// panel's choice, deliberately. A search query is a place in the document
    /// the user may want back; a palette query was an *aim*, already hit or
    /// abandoned, and an empty query shows every command ordered by recency,
    /// which is the most useful first screen a palette has.
    pub fn open(&mut self) {
        self.query = Field::default();
        self.selected = 0;
        self.scroll = 0;
        // A sequence half-typed before the panel closed means nothing now.
        self.pending.clear();
    }

    /// The query field's text.
    #[must_use]
    pub fn query(&self) -> &str {
        self.query.text()
    }

    /// Inserts pasted text into the query, one character at a time.
    ///
    /// The field refuses control characters, so a multi-line paste contributes
    /// its printable characters only — the newlines that would silently change
    /// what the query matched never enter it.
    pub fn paste(&mut self, text: &str) {
        let mut changed = false;
        for character in text.chars() {
            changed |= self.query.insert(character);
        }
        if changed {
            self.selected = 0;
            self.scroll = 0;
        }
    }

    /// Handles one key press. Every key is consumed; see the module docs.
    pub fn handle_key(
        &mut self,
        event: &KeyEvent,
        editor: &Editor,
        mru: &CommandMru,
    ) -> PaletteOutcome {
        match self.resolve_key(event) {
            Resolved::Verb(verb) => self.run(verb, editor, mru),
            // A half-typed sequence and a deliberately-suppressed key are both
            // "nothing happened, and the panel stays open". They are separate
            // variants because only one of them may fall through to the query.
            Resolved::Pending | Resolved::Suppressed => PaletteOutcome::Handled,
            Resolved::Unclaimed => self.unclaimed(event),
        }
    }

    /// Runs one resolved verb.
    fn run(&mut self, verb: Verb, editor: &Editor, mru: &CommandMru) -> PaletteOutcome {
        let page = isize_of(self.window.max(1));
        match verb {
            Verb::Dismiss => PaletteOutcome::Closed,
            Verb::Accept => self.accept(editor, mru),
            Verb::SelectPrevious => self.move_selection(editor, mru, -1),
            Verb::SelectNext => self.move_selection(editor, mru, 1),
            Verb::SelectPageUp => self.move_selection(editor, mru, -page),
            Verb::SelectPageDown => self.move_selection(editor, mru, page),
            Verb::CaretLeft => self.edit(Field::move_left, false),
            Verb::CaretRight => self.edit(Field::move_right, false),
            Verb::CaretHome => self.edit(Field::move_home, false),
            Verb::CaretEnd => self.edit(Field::move_end, false),
            Verb::QueryBackspace => self.edit(Field::backspace, true),
            Verb::QueryDelete => self.edit(Field::delete, true),
        }
    }

    /// A key no binding claimed: text if it is text, swallowed otherwise.
    ///
    /// The fall-through is what makes the panel typable at all, and it is
    /// deliberately *below* the keymap: a user who binds a bare letter to a
    /// verb gets the verb, and every other letter still types.
    fn unclaimed(&mut self, event: &KeyEvent) -> PaletteOutcome {
        match event.key {
            KeyCode::Char(character) if types_text(event.modifiers) => {
                self.edit(|field| field.insert(character), true)
            },
            // Modal: everything else is swallowed, not passed to the document.
            _ => PaletteOutcome::Handled,
        }
    }

    /// Paints the panel over a finished frame, returning where the caret goes.
    ///
    /// `&mut self` because painting is where the panel learns its geometry: the
    /// scroll window follows the selection here, and the page size the page
    /// keys hop by is whatever this paint actually showed. `None` means the
    /// screen is too small for an honest panel and nothing was drawn.
    pub fn paint(
        &mut self,
        buffer: &mut CellBuffer,
        editor: &Editor,
        mru: &CommandMru,
        styles: &Palette,
    ) -> Option<CellPosition> {
        paint::paint(self, buffer, editor, mru, styles)
    }

    /// Resolves the selected entry and closes, or stays open with nothing to run.
    fn accept(&self, editor: &Editor, mru: &CommandMru) -> PaletteOutcome {
        let results = palette::search_text(editor.commands(), mru, self.query.text(), None);
        let Some(entry) = results.get(self.selected.min(results.len().saturating_sub(1))) else {
            return PaletteOutcome::Handled;
        };
        PaletteOutcome::Run(entry.id().clone())
    }

    /// Moves the selection by `delta`, clamping at both ends of the list.
    fn move_selection(
        &mut self,
        editor: &Editor,
        mru: &CommandMru,
        delta: isize,
    ) -> PaletteOutcome {
        let count = palette::search_text(editor.commands(), mru, self.query.text(), None).len();
        let Some(last) = count.checked_sub(1) else {
            self.selected = 0;
            return PaletteOutcome::Handled;
        };
        let current = self.selected.min(last);
        self.selected = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs())
        } else {
            current.saturating_add(delta.unsigned_abs()).min(last)
        };
        PaletteOutcome::Handled
    }

    /// Applies an edit to the query field.
    ///
    /// `rewrites` says whether the edit can change the text. When it did, the
    /// selection returns to the top: the old index pointed into a list that no
    /// longer exists, and the best match for the new query is the thing the
    /// user is narrowing towards.
    fn edit(&mut self, edit: impl FnOnce(&mut Field) -> bool, rewrites: bool) -> PaletteOutcome {
        let changed = edit(&mut self.query);
        if changed && rewrites {
            self.selected = 0;
            self.scroll = 0;
        }
        PaletteOutcome::Handled
    }

    /// The selection, clamped against a list of `count` results.
    pub(super) fn clamped_selection(&self, count: usize) -> usize {
        self.selected.min(count.saturating_sub(1))
    }

    /// Slides the scroll window so the selection is inside `visible` rows, and
    /// remembers `visible` as the page size. Called from the paint pass.
    pub(super) fn follow_selection(&mut self, count: usize, visible: usize) {
        self.window = visible.max(1);
        if visible == 0 || count == 0 {
            self.scroll = 0;
            return;
        }
        let selected = self.clamped_selection(count);
        if selected < self.scroll {
            self.scroll = selected;
        } else if selected >= self.scroll + visible {
            self.scroll = selected + 1 - visible;
        }
        self.scroll = self.scroll.min(count.saturating_sub(visible));
    }

    /// The first visible result row.
    pub(super) const fn scroll(&self) -> usize {
        self.scroll
    }

    /// The query field, for the paint pass.
    pub(super) const fn field(&self) -> &Field {
        &self.query
    }
}

/// `value` as an `isize`, saturating on a page size no terminal can reach.
fn isize_of(value: usize) -> isize {
    isize::try_from(value).unwrap_or(isize::MAX)
}
