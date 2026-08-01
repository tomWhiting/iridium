//! The prompt line: the row a question or a message takes while one is open.
//!
//! # It takes the statusline's row rather than a row of its own
//!
//! The screen's rows are divided by the kernel's layout: document text, the
//! search panel, then one statusline. A prompt that claimed a row of its own
//! would shrink the document every time it opened and grow it again when it
//! closed — the viewport resizing on every question, and the text under the
//! reader moving with it. So a prompt is painted **over** the statusline. The
//! cost is that the file's name and its unsaved marker are hidden while a
//! question is on screen, and a question is answered in one keystroke.
//!
//! # A prompt is modal, deliberately
//!
//! Every key goes to the open prompt, including chords the editor binds, and
//! the ones it does not bind are swallowed. The search panel is the opposite —
//! it reports `SearchOutcome::Ignored` and lets the host act — and the
//! difference is what the two are for. A search field is a place to type while
//! the document stays live; a prompt is a question, and a key that fell through
//! to the document would edit the file behind a question about saving it.
//!
//! # This is not a second editing model
//!
//! [`Entry`] is a line of text with a caret, not a document: no history, no
//! selections, no commands, and nothing in it reachable from document text. The
//! kernel's editing verbs act on a [`Document`](iridium_editor::Document)
//! through reversible commands, and a file name being typed is a `&str`
//! argument. `iridium-tui` has a field of its own for the search panel on the
//! same reasoning; it is private to that crate, so this is a second *field*
//! rather than a second editing model.
//!
//! The caret is a byte offset that always sits on a grapheme-cluster boundary,
//! never a `char` index and never a cell. A `char` index would let it land
//! between the two `char`s of `e` plus a combining acute, which is a position no
//! terminal can draw a cursor at; a cell would be ambiguous across the two
//! halves of a double-width glyph. Every motion moves whole clusters, measured
//! by [`LineLayout`], which is the face's single place for cell arithmetic.

use std::ops::Range;
use std::path::PathBuf;

use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use iridium_tui::cell::{CellBuffer, Style, WriteOutcome};
use iridium_tui::frame::{LineLayout, Palette};

/// What answering "yes" to a confirmation asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Deed {
    /// Leave the editor, discarding unsaved changes.
    Quit,
    /// Re-read the file, discarding unsaved changes.
    Reload,
}

/// What the application must do about a key handed to an open prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The prompt is still open. The key was consumed either way.
    Pending,
    /// The prompt was dismissed with nothing to do.
    Cancelled,
    /// Move the caret to this line, counting from one.
    Goto(usize),
    /// Write the document to this path.
    SaveAs(PathBuf),
    /// Do what the confirmation asked about.
    Do(Deed),
}

/// A question occupying the prompt line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// A line number to move the caret to.
    GotoLine(Entry),
    /// A name to write an unnamed buffer under.
    SaveAs(Entry),
    /// A question with two answers, `y` and `n`.
    Confirm {
        /// What is being asked, including its `(y/n)`.
        question: String,
        /// What "yes" does.
        deed: Deed,
    },
}

impl Prompt {
    /// A prompt asking for a line number.
    #[must_use]
    pub fn goto_line() -> Self {
        Self::GotoLine(Entry::new())
    }

    /// A prompt asking for a file name.
    #[must_use]
    pub fn save_as() -> Self {
        Self::SaveAs(Entry::new())
    }

    /// A prompt asking a yes-or-no question.
    #[must_use]
    pub fn confirm(question: impl Into<String>, deed: Deed) -> Self {
        Self::Confirm {
            question: question.into(),
            deed,
        }
    }

    /// Hands one key press to the prompt.
    ///
    /// Every key is consumed: see the module documentation for why a prompt is
    /// modal where the search panel is not.
    pub fn answer(&mut self, event: &KeyEvent) -> Answer {
        if !is_plain(event.modifiers) {
            return Answer::Pending;
        }
        match self {
            Self::GotoLine(entry) => match event.key {
                KeyCode::Escape => Answer::Cancelled,
                KeyCode::Enter => {
                    if entry.text().is_empty() {
                        // Nothing was asked for, so nothing is answered.
                        return Answer::Cancelled;
                    }
                    // The field holds digits and nothing else, so the only
                    // parse failure possible is a number larger than `usize` —
                    // which is past the end of any document, and the caller
                    // clamps it to the last line.
                    Answer::Goto(entry.text().parse().unwrap_or(usize::MAX))
                },
                KeyCode::Char(character) if character.is_ascii_digit() => {
                    entry.insert(character);
                    Answer::Pending
                },
                key => {
                    entry.edit(key);
                    Answer::Pending
                },
            },
            Self::SaveAs(entry) => match event.key {
                KeyCode::Escape => Answer::Cancelled,
                KeyCode::Enter => {
                    if entry.text().is_empty() {
                        return Answer::Cancelled;
                    }
                    Answer::SaveAs(PathBuf::from(entry.text()))
                },
                KeyCode::Char(character) => {
                    entry.insert(character);
                    Answer::Pending
                },
                key => {
                    entry.edit(key);
                    Answer::Pending
                },
            },
            Self::Confirm { deed, .. } => match event.key {
                KeyCode::Char('y' | 'Y') => Answer::Do(*deed),
                KeyCode::Char('n' | 'N') | KeyCode::Escape => Answer::Cancelled,
                _ => Answer::Pending,
            },
        }
    }

    /// Inserts pasted text into the field, if there is one.
    ///
    /// Filtered rather than refused, because pasting a path into "Save as" is
    /// the reason bracketed paste is worth having here. Characters the field
    /// will not take are dropped one at a time, so a two-line paste becomes the
    /// two lines run together rather than a name with a newline in it; a
    /// confirmation has nothing to type into and ignores the paste whole.
    pub fn paste(&mut self, text: &str) {
        let digits_only = matches!(self, Self::GotoLine(_));
        let Some(entry) = self.entry_mut() else {
            return;
        };
        for character in text.chars() {
            if digits_only && !character.is_ascii_digit() {
                continue;
            }
            entry.insert(character);
        }
    }

    /// The whole line as it reads on screen.
    #[must_use]
    pub fn line(&self) -> String {
        let mut line = String::from(self.label());
        if let Some(entry) = self.entry() {
            line.push_str(entry.text());
        }
        line
    }

    /// The caret's cell, counted from the start of the line.
    ///
    /// `None` for a confirmation: it has no field to type into, and a cursor
    /// parked beside a yes-or-no question invites typing that is thrown away.
    #[must_use]
    pub fn caret_cell(&self) -> Option<usize> {
        let entry = self.entry()?;
        Some(LineLayout::new(self.label(), 1).width() + entry.caret_cell())
    }

    /// Paints the prompt across `row`, returning the caret's buffer column.
    ///
    /// The line is scrolled just far enough to keep the caret on screen, which
    /// is what makes a path longer than the terminal is wide typeable. `None`
    /// means no cursor should be shown: a confirmation has no field, and a row
    /// that is not on the screen has no cell to put one in.
    pub fn paint(&self, buffer: &mut CellBuffer, row: usize, palette: &Palette) -> Option<usize> {
        let width = buffer.width();
        let caret = self.caret_cell();
        let scroll = caret.map_or(0, |cell| scroll_for(cell, width));
        paint_line(buffer, row, &self.line(), palette.status(), scroll);
        if width == 0 || row >= buffer.height() {
            return None;
        }
        caret.map(|cell| cell.saturating_sub(scroll).min(width - 1))
    }

    /// The fixed text in front of the field, or the question itself.
    fn label(&self) -> &str {
        match self {
            Self::GotoLine(_) => "Go to line: ",
            Self::SaveAs(_) => "Save as: ",
            Self::Confirm { question, .. } => question,
        }
    }

    /// The field being typed into, when there is one.
    const fn entry(&self) -> Option<&Entry> {
        match self {
            Self::GotoLine(entry) | Self::SaveAs(entry) => Some(entry),
            Self::Confirm { .. } => None,
        }
    }

    /// The field being typed into, mutably.
    const fn entry_mut(&mut self) -> Option<&mut Entry> {
        match self {
            Self::GotoLine(entry) | Self::SaveAs(entry) => Some(entry),
            Self::Confirm { .. } => None,
        }
    }
}

/// Something the editor has to say, shown on the prompt line until the next
/// key press.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    /// What it says.
    text: String,
    /// Whether it reports a failure.
    is_error: bool,
}

impl Message {
    /// A message reporting that something worked.
    #[must_use]
    pub fn notice(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    /// A message reporting that something did not.
    #[must_use]
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }

    /// What it says.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Whether it reports a failure.
    #[must_use]
    pub const fn is_error(&self) -> bool {
        self.is_error
    }

    /// Paints the message across `row`.
    ///
    /// Never scrolled: there is no caret to keep in view, and a message too
    /// long for the terminal is cut at the right edge with its beginning — the
    /// part that says what happened — intact.
    pub fn paint(&self, buffer: &mut CellBuffer, row: usize, palette: &Palette) {
        paint_line(buffer, row, &self.text, self.style(palette), 0);
    }

    /// The style this message is painted in: the statusline's own colours, with
    /// a failure taking the theme's error foreground.
    ///
    /// The overlay's error style is not used whole because its background
    /// belongs to the search panel, and half a panel's colours on the
    /// statusline row reads as a rendering fault rather than as a message.
    fn style(&self, palette: &Palette) -> Style {
        if self.is_error {
            palette
                .status()
                .with_foreground(palette.overlay_error().foreground)
        } else {
            palette.status()
        }
    }
}

/// One editable line of text with a caret in it.
///
/// The caret is a byte offset on a grapheme-cluster boundary; see the module
/// documentation for why it is neither a `char` index nor a cell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    /// The text, which never contains a control character.
    text: String,
    /// The caret, as a byte offset that is always on a cluster boundary.
    caret: usize,
}

impl Entry {
    /// An empty field.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The field's text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Applies a motion or a deletion named by a key.
    ///
    /// Keys this does not name leave the field alone; the prompt has already
    /// consumed them.
    fn edit(&mut self, key: KeyCode) {
        match key {
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.caret = 0,
            KeyCode::End => self.caret = self.text.len(),
            _ => {},
        }
    }

    /// Inserts a character at the caret, which then sits after it.
    ///
    /// Control characters are refused: nothing in the face can paint one, and a
    /// newline in a file name would be a character the user could see no trace
    /// of while it silently changed which file was written.
    fn insert(&mut self, character: char) {
        if character.is_control() {
            return;
        }
        // The caret is on a cluster boundary by construction. Recovering rather
        // than trusting it is what keeps a mistake anywhere else in this module
        // from becoming a panic with the terminal in raw mode.
        if !self.text.is_char_boundary(self.caret) {
            self.caret = self.text.len();
        }
        self.text.insert(self.caret, character);
        self.caret += character.len_utf8();
    }

    /// Removes the whole grapheme cluster before the caret.
    fn backspace(&mut self) {
        let Some(range) = self.cluster_before() else {
            return;
        };
        self.caret = range.start;
        self.remove(range);
    }

    /// Removes the whole grapheme cluster at the caret, leaving it in place.
    fn delete(&mut self) {
        let Some(range) = self.cluster_at() else {
            return;
        };
        self.remove(range);
    }

    /// Moves the caret one cluster left.
    fn move_left(&mut self) {
        if let Some(range) = self.cluster_before() {
            self.caret = range.start;
        }
    }

    /// Moves the caret one cluster right.
    fn move_right(&mut self) {
        if let Some(range) = self.cluster_at() {
            self.caret = range.end;
        }
    }

    /// The display column the caret sits at, counted from the field's start.
    ///
    /// A cell count, not a `char` count: a caret after one CJK ideograph is at
    /// column two.
    #[must_use]
    pub fn caret_cell(&self) -> usize {
        let layout = self.layout();
        if self.caret >= self.text.len() {
            return layout.width();
        }
        layout
            .clusters()
            .iter()
            .find(|cluster| cluster.byte_index() == self.caret)
            .map_or(layout.width(), |cluster| cluster.column())
    }

    /// The cluster layout of the field's text.
    ///
    /// A tab width of one because a field has no tab stops: nothing can insert
    /// a tab into it, and the width is still needed for the general call.
    fn layout(&self) -> LineLayout {
        LineLayout::new(&self.text, 1)
    }

    /// The byte range of the cluster ending at the caret, if there is one.
    fn cluster_before(&self) -> Option<Range<usize>> {
        self.layout()
            .clusters()
            .iter()
            .find(|cluster| cluster.byte_index() + cluster.byte_count() == self.caret)
            .map(|cluster| cluster.byte_index()..self.caret)
    }

    /// The byte range of the cluster starting at the caret, if there is one.
    fn cluster_at(&self) -> Option<Range<usize>> {
        self.layout()
            .clusters()
            .iter()
            .find(|cluster| cluster.byte_index() == self.caret)
            .map(|cluster| cluster.byte_index()..cluster.byte_index() + cluster.byte_count())
    }

    /// Removes a byte range, refusing one that is not a valid slice.
    ///
    /// Every range this module produces comes from a cluster and is valid. The
    /// check is here because `String::drain` panics on one that is not.
    fn remove(&mut self, range: Range<usize>) {
        if range.start >= range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return;
        }
        self.text.drain(range);
    }
}

/// Whether a modifier set leaves the key meaning what it says.
///
/// `Shift` is not consulted: it decided which *character* a printable key
/// produced, which the input adapter has already resolved.
const fn is_plain(modifiers: Modifiers) -> bool {
    !modifiers.ctrl && !modifiers.alt && !modifiers.meta
}

/// The first cell of a line that is shown, so `caret` stays inside `width`.
///
/// Zero until the caret would fall off the right edge, and then just enough to
/// keep it in the last cell. The caret's own cell counts: a caret past the last
/// glyph of a full line must still be visible, or typing into a field that has
/// filled the row gives no feedback at all.
const fn scroll_for(caret: usize, width: usize) -> usize {
    if width == 0 || caret < width {
        return 0;
    }
    caret + 1 - width
}

/// Paints one line of text across `row`, blanking whatever it does not fill.
///
/// A glyph the scroll or the right edge cuts through is left as a blank rather
/// than painted in the cells it still has. Half a double-width glyph is a state
/// no terminal can render, which is why the cell buffer refuses to hold one.
fn paint_line(buffer: &mut CellBuffer, row: usize, text: &str, style: Style, scroll: usize) {
    let width = buffer.width();
    if row >= buffer.height() {
        return;
    }
    for column in 0..width {
        buffer.set_str(column, row, " ", style);
    }
    let layout = LineLayout::new(text, 1);
    for cluster in layout.clusters() {
        let Some(grapheme) = cluster.grapheme() else {
            continue;
        };
        let Some(column) = cluster.column().checked_sub(scroll) else {
            continue;
        };
        if column + cluster.width() > width {
            break;
        }
        match buffer.set_grapheme(column, row, grapheme, style) {
            WriteOutcome::Written(_) => {},
            // Unreachable: the check above proved the glyph fits inside the
            // row. Stopping rather than continuing keeps a wrong answer here
            // from painting the rest of the line into the wrong cells.
            WriteOutcome::Truncated | WriteOutcome::OutOfBounds => break,
        }
    }
}

#[cfg(test)]
mod tests {
    use iridium_editor::Theme;
    use iridium_tui::cell::CellContent;

    use super::*;

    /// A family emoji: seven `char`s, one cluster, two cells.
    const FAMILY: &str = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";

    /// A plain key press.
    fn press(key: KeyCode) -> KeyEvent {
        KeyEvent {
            key,
            modifiers: Modifiers::none(),
            is_repeat: false,
        }
    }

    /// Types a string into a prompt, one plain key press per character.
    fn type_into(prompt: &mut Prompt, text: &str) {
        for character in text.chars() {
            assert_eq!(
                prompt.answer(&press(KeyCode::Char(character))),
                Answer::Pending
            );
        }
    }

    /// The text of a row, with continuation cells contributing nothing.
    fn row_text(buffer: &CellBuffer, row: usize) -> String {
        let mut out = String::new();
        let Some(cells) = buffer.row(row) else {
            return out;
        };
        for cell in cells {
            match cell.content() {
                CellContent::Grapheme(grapheme) => grapheme.push_to(&mut out),
                CellContent::Continuation => {},
            }
        }
        out
    }

    #[test]
    fn a_line_number_is_typed_and_submitted() {
        let mut prompt = Prompt::goto_line();
        type_into(&mut prompt, "412");
        assert_eq!(prompt.line(), "Go to line: 412");
        assert_eq!(prompt.answer(&press(KeyCode::Enter)), Answer::Goto(412));
    }

    #[test]
    fn the_line_prompt_takes_digits_and_nothing_else() {
        let mut prompt = Prompt::goto_line();
        type_into(&mut prompt, "1a2-3 4");
        assert_eq!(prompt.line(), "Go to line: 1234");
    }

    #[test]
    fn a_line_number_larger_than_usize_becomes_the_last_line() {
        // Not an error the user can act on: they asked for the end of the
        // file, and the caller clamps it there.
        let mut prompt = Prompt::goto_line();
        type_into(&mut prompt, "99999999999999999999999999");
        assert_eq!(
            prompt.answer(&press(KeyCode::Enter)),
            Answer::Goto(usize::MAX)
        );
    }

    #[test]
    fn submitting_an_empty_field_cancels() {
        let mut prompt = Prompt::goto_line();
        assert_eq!(prompt.answer(&press(KeyCode::Enter)), Answer::Cancelled);
        let mut prompt = Prompt::save_as();
        assert_eq!(prompt.answer(&press(KeyCode::Enter)), Answer::Cancelled);
    }

    #[test]
    fn escape_cancels_every_prompt() {
        for mut prompt in [
            Prompt::goto_line(),
            Prompt::save_as(),
            Prompt::confirm("Quit? (y/n)", Deed::Quit),
        ] {
            assert_eq!(prompt.answer(&press(KeyCode::Escape)), Answer::Cancelled);
        }
    }

    #[test]
    fn a_confirmation_answers_only_yes_and_no() {
        let mut prompt = Prompt::confirm("Quit? (y/n)", Deed::Quit);
        for key in [
            KeyCode::Char('q'),
            KeyCode::Enter,
            KeyCode::Backspace,
            KeyCode::Left,
        ] {
            assert_eq!(
                prompt.answer(&press(key)),
                Answer::Pending,
                "{key:?} must be swallowed"
            );
        }
        assert_eq!(
            prompt.answer(&press(KeyCode::Char('n'))),
            Answer::Cancelled
        );
        assert_eq!(
            prompt.answer(&press(KeyCode::Char('Y'))),
            Answer::Do(Deed::Quit)
        );
    }

    #[test]
    fn a_chord_is_swallowed_rather_than_reaching_the_document() {
        // The whole point of a modal prompt: a save chord typed into a
        // question about leaving must not save.
        let mut prompt = Prompt::save_as();
        let chord = KeyEvent {
            key: KeyCode::Char('s'),
            modifiers: Modifiers::ctrl(),
            is_repeat: false,
        };
        assert_eq!(prompt.answer(&chord), Answer::Pending);
        assert_eq!(prompt.line(), "Save as: ");
    }

    #[test]
    fn a_name_is_typed_and_becomes_a_path() {
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, "notes.md");
        assert_eq!(
            prompt.answer(&press(KeyCode::Enter)),
            Answer::SaveAs(PathBuf::from("notes.md"))
        );
    }

    #[test]
    fn editing_a_name_moves_by_whole_clusters() {
        // Removing one `char` of a joined emoji would leave a dangling zero
        // width joiner in the file name.
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, FAMILY);
        assert_eq!(prompt.answer(&press(KeyCode::Backspace)), Answer::Pending);
        assert_eq!(prompt.line(), "Save as: ");
    }

    #[test]
    fn a_paste_is_filtered_rather_than_refused() {
        let mut prompt = Prompt::save_as();
        prompt.paste("notes\n.md");
        assert_eq!(prompt.line(), "Save as: notes.md");

        let mut prompt = Prompt::goto_line();
        prompt.paste("line 42");
        assert_eq!(prompt.line(), "Go to line: 42");

        let mut prompt = Prompt::confirm("Quit? (y/n)", Deed::Quit);
        prompt.paste("yes");
        assert_eq!(prompt.line(), "Quit? (y/n)");
    }

    #[test]
    fn the_caret_counts_cells_not_chars() {
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, "漢");
        // "Save as: " is nine cells, and one ideograph is two more.
        assert_eq!(prompt.caret_cell(), Some(11));
    }

    #[test]
    fn a_confirmation_has_no_caret() {
        assert_eq!(
            Prompt::confirm("Quit? (y/n)", Deed::Quit).caret_cell(),
            None
        );
    }

    #[test]
    fn the_caret_moves_and_insertion_lands_where_it_is() {
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, "ac");
        assert_eq!(prompt.answer(&press(KeyCode::Left)), Answer::Pending);
        type_into(&mut prompt, "b");
        assert_eq!(prompt.line(), "Save as: abc");

        assert_eq!(prompt.answer(&press(KeyCode::Home)), Answer::Pending);
        assert_eq!(prompt.answer(&press(KeyCode::Delete)), Answer::Pending);
        assert_eq!(prompt.line(), "Save as: bc");

        assert_eq!(prompt.answer(&press(KeyCode::End)), Answer::Pending);
        assert_eq!(prompt.caret_cell(), Some(11));
    }

    #[test]
    fn painting_fills_the_row_and_reports_the_caret() {
        let palette = Palette::from_theme(&Theme::dark());
        let mut buffer = CellBuffer::new(20, 1);
        let mut prompt = Prompt::goto_line();
        type_into(&mut prompt, "42");

        let caret = prompt.paint(&mut buffer, 0, &palette);
        assert_eq!(row_text(&buffer, 0), "Go to line: 42      ");
        assert_eq!(caret, Some(14));
    }

    #[test]
    fn a_line_wider_than_the_row_scrolls_to_keep_the_caret_visible() {
        let palette = Palette::from_theme(&Theme::dark());
        let mut buffer = CellBuffer::new(12, 1);
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, "abcdef");

        // "Save as: abcdef" is fifteen cells and the row holds twelve, so the
        // first four are dropped and the caret lands in the last cell.
        let caret = prompt.paint(&mut buffer, 0, &palette);
        assert_eq!(row_text(&buffer, 0), " as: abcdef ");
        assert_eq!(caret, Some(11));
    }

    #[test]
    fn painting_a_row_that_is_not_there_is_a_no_op() {
        let palette = Palette::from_theme(&Theme::dark());
        let mut buffer = CellBuffer::new(0, 0);
        assert_eq!(Prompt::goto_line().paint(&mut buffer, 0, &palette), None);
        Message::notice("wrote it").paint(&mut buffer, 4, &palette);
        assert_eq!(buffer.width(), 0);
    }

    #[test]
    fn a_message_paints_its_text_and_marks_a_failure() {
        let palette = Palette::from_theme(&Theme::dark());
        let mut buffer = CellBuffer::new(16, 1);

        let notice = Message::notice("wrote a.rs");
        notice.paint(&mut buffer, 0, &palette);
        assert!(!notice.is_error());
        assert_eq!(row_text(&buffer, 0), "wrote a.rs      ");

        let failure = Message::error("cannot write a.rs");
        assert!(failure.is_error());
        assert_eq!(failure.text(), "cannot write a.rs");
        assert_ne!(failure.style(&palette), notice.style(&palette));
    }
}
