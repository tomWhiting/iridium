//! The prompt strip's questions and messages — state only, no pixels.
//!
//! This is the desktop counterpart of the terminal face's `app::prompt`, and
//! it keeps that module's semantics exactly: a prompt is **modal** (every key
//! goes to it, chords included, and the ones it does not name are swallowed —
//! a save chord typed into a question about quitting must not save), `Escape`
//! cancels, `Enter` confirms, and typed text edits the field. What it does not
//! keep is the painting: the terminal paints cells, this face paints the strip
//! on the GPU ([`crate::overlay`]), so this module ends at strings and a caret
//! column.
//!
//! # This is not a second editing model
//!
//! [`Entry`] is a line of text with a caret, not a document: no history, no
//! selections, no commands, and nothing in it reachable from document text.
//! The kernel's editing verbs act on a `Document` through reversible
//! commands, and a file name being typed is a `&str` argument. The terminal
//! face's prompt makes the same argument at more length.
//!
//! The caret is a byte offset that always sits on a grapheme-cluster
//! boundary. Every motion and deletion moves whole clusters — removing one
//! `char` of a joined emoji would leave a dangling zero-width joiner in a
//! file name. The caret's *column*, by contrast, is counted in `char`s,
//! because that is the grid the GPU face draws in: the compositor places the
//! document caret at `column × char_width`, and the strip's caret follows the
//! same convention rather than inventing a second one.

use std::ops::Range;
use std::path::{Path, PathBuf};

use iridium_editor::{KeyCode, KeyEvent, Modifiers};
use unicode_segmentation::UnicodeSegmentation as _;

/// What answering "yes" to a confirmation asks for.
///
/// Every variant here destroys something that cannot be got back, which is
/// the only thing this face asks permission for. There was a third — replacing
/// the buffer with a dropped file — and it went away rather than changing when
/// opening a file became *additive*: a drop now makes a tab beside what is open
/// and destroys nothing, so there is nothing left to ask about.
///
/// ⚠️ **[`OverwriteWith`](Self::OverwriteWith) is the one that destroys work
/// that is not the user's own.** The other two discard an unsaved buffer, which
/// its author typed and can retype; this one replaces a file on disk that
/// something else wrote. It is a confirmation rather than a refusal because a
/// save-as that could never overwrite is a save-as that cannot answer "put this
/// over that", which is half of what the verb is for.
///
/// This carries a [`PathBuf`], so the enum is [`Clone`] rather than [`Copy`].
/// The path is *the answer*, not a lookup key: the file the buffer is currently
/// attached to is deliberately not the one being written, and asking the
/// application for "the path" at the moment the answer comes back would find
/// the wrong one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deed {
    /// Leave the editor, discarding unsaved changes.
    Quit,
    /// Close the active tab, discarding its unsaved changes.
    CloseTab,
    /// Write the document to this path even though a file is already there.
    OverwriteWith(PathBuf),
}

/// What the application must do about a key handed to an open prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// The prompt is still open. The key was consumed either way.
    Pending,
    /// The prompt was dismissed with nothing to do.
    Cancelled,
    /// Write the document to this path.
    SaveAs(PathBuf),
    /// Do what the confirmation asked about.
    Do(Deed),
}

/// A question occupying the prompt strip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
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
    /// A prompt asking for a file name, with nothing typed into it yet.
    #[must_use]
    pub fn save_as() -> Self {
        Self::SaveAs(Entry::new())
    }

    /// A prompt asking for a file name, starting from the one the document
    /// already has.
    ///
    /// ⭐ **The field is filled rather than left empty, and that is the whole
    /// difference between a usable "Save As" and a paragraph of retyping.** A
    /// named document's path is often forty characters of directory and six of
    /// file name, and the change wanted is nearly always in the last six. An
    /// empty field asks for all forty-six again and invites the typo that
    /// writes the file somewhere nobody will look for it.
    ///
    /// The caret lands at the **end**, because [`Entry`] has no selection: the
    /// macOS convention of pre-selecting the stem needs one, and a caret at the
    /// start would put every typed character in front of the directory. End is
    /// the position from which the common edit — change the extension, add a
    /// suffix — is one keystroke away.
    #[must_use]
    pub fn save_as_named(path: &Path) -> Self {
        Self::SaveAs(Entry::with_text(&path.to_string_lossy()))
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
    /// Every key is consumed: see the module documentation for why a prompt
    /// is modal.
    pub fn answer(&mut self, event: &KeyEvent) -> Answer {
        if !is_plain(event.modifiers) {
            return Answer::Pending;
        }
        match self {
            Self::SaveAs(entry) => match event.key {
                KeyCode::Escape => Answer::Cancelled,
                KeyCode::Enter => {
                    if entry.text().is_empty() {
                        // Nothing was asked for, so nothing is answered.
                        return Answer::Cancelled;
                    }
                    Answer::SaveAs(PathBuf::from(entry.text()))
                },
                KeyCode::Char(character) => {
                    let _ = entry.insert(character);
                    Answer::Pending
                },
                key => {
                    entry.edit(key);
                    Answer::Pending
                },
            },
            Self::Confirm { deed, .. } => match event.key {
                KeyCode::Char('y' | 'Y') => Answer::Do(deed.clone()),
                KeyCode::Char('n' | 'N') | KeyCode::Escape => Answer::Cancelled,
                _ => Answer::Pending,
            },
        }
    }

    /// Inserts pasted text into the field, if there is one.
    ///
    /// Filtered rather than refused, because pasting a path into "Save as" is
    /// the reason the paste chord reaches an open prompt at all. Characters
    /// the field will not take are dropped one at a time, so a two-line paste
    /// becomes the two lines run together rather than a name with a newline
    /// in it; a confirmation has nothing to type into and ignores the paste
    /// whole.
    pub fn paste(&mut self, text: &str) {
        let Self::SaveAs(entry) = self else {
            return;
        };
        for character in text.chars() {
            let _ = entry.insert(character);
        }
    }

    /// The whole line as it reads on screen.
    #[must_use]
    pub fn line(&self) -> String {
        let mut line = String::from(self.label());
        if let Self::SaveAs(entry) = self {
            line.push_str(entry.text());
        }
        line
    }

    /// The caret's column in `char`s, counted from the start of the line.
    ///
    /// `None` for a confirmation: it has no field to type into, and a caret
    /// parked beside a yes-or-no question invites typing that is thrown away.
    #[must_use]
    pub fn caret_column(&self) -> Option<usize> {
        let Self::SaveAs(entry) = self else {
            return None;
        };
        Some(self.label().chars().count() + entry.caret_column())
    }

    /// The fixed text in front of the field, or the question itself.
    fn label(&self) -> &str {
        match self {
            Self::SaveAs(_) => "Save as: ",
            Self::Confirm { question, .. } => question,
        }
    }
}

/// Something the editor has to say, shown on the prompt strip until the next
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
}

/// One editable line of text with a caret in it.
///
/// The caret is a byte offset on a grapheme-cluster boundary; see the module
/// documentation for why it is a cluster boundary and why its column is
/// counted in `char`s.
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

    /// A field already holding `text`, with the caret after it.
    ///
    /// Built by [`insert`](Self::insert)ing one character at a time rather than
    /// by assigning the string, so the field's one invariant — **no control
    /// character ever enters a file name** — holds by construction here as it
    /// does for typing. A second way in that assigned the string directly would
    /// be a second way for a newline to reach a path, and the caller's text is
    /// exactly the kind that could carry one: it comes from an
    /// [`OsStr`](std::ffi::OsStr), which permits bytes a keyboard cannot send.
    #[must_use]
    pub fn with_text(text: &str) -> Self {
        let mut entry = Self::new();
        for character in text.chars() {
            let _ = entry.insert(character);
        }
        entry
    }

    /// The field's text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Applies a motion or a deletion named by a key.
    ///
    /// Keys this does not name leave the field alone; the prompt has already
    /// consumed them. The overlays call the named operations directly because
    /// they need to know whether the text changed; the prompt does not.
    fn edit(&mut self, key: KeyCode) {
        let _ = match key {
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.move_left(),
            KeyCode::Right => self.move_right(),
            KeyCode::Home => self.move_home(),
            KeyCode::End => self.move_end(),
            _ => false,
        };
    }

    /// Inserts a character at the caret, which then sits after it.
    ///
    /// Control characters are refused: nothing in the face can paint one, and
    /// a newline in a file name would be a character the user could see no
    /// trace of while it silently changed which file was written.
    ///
    /// Returns whether anything was inserted.
    pub(crate) fn insert(&mut self, character: char) -> bool {
        if character.is_control() {
            return false;
        }
        // The caret is on a cluster boundary by construction. Recovering
        // rather than trusting it is what keeps a mistake anywhere else in
        // this module from becoming a panic mid-frame.
        if !self.text.is_char_boundary(self.caret) {
            self.caret = self.text.len();
        }
        self.text.insert(self.caret, character);
        self.caret += character.len_utf8();
        true
    }

    /// Removes the whole grapheme cluster before the caret.
    ///
    /// Returns whether anything was removed.
    pub(crate) fn backspace(&mut self) -> bool {
        let Some(range) = self.cluster_before() else {
            return false;
        };
        self.caret = range.start;
        self.remove(range)
    }

    /// Removes the whole grapheme cluster at the caret, leaving it in place.
    ///
    /// Returns whether anything was removed.
    pub(crate) fn delete(&mut self) -> bool {
        let Some(range) = self.cluster_at() else {
            return false;
        };
        self.remove(range)
    }

    /// Moves the caret one cluster left. Returns whether it moved.
    pub(crate) fn move_left(&mut self) -> bool {
        let Some(range) = self.cluster_before() else {
            return false;
        };
        self.caret = range.start;
        true
    }

    /// Moves the caret one cluster right. Returns whether it moved.
    pub(crate) fn move_right(&mut self) -> bool {
        let Some(range) = self.cluster_at() else {
            return false;
        };
        self.caret = range.end;
        true
    }

    /// Moves the caret to the start. Returns whether it moved.
    pub(crate) const fn move_home(&mut self) -> bool {
        let moved = self.caret != 0;
        self.caret = 0;
        moved
    }

    /// Moves the caret past the last cluster. Returns whether it moved.
    pub(crate) fn move_end(&mut self) -> bool {
        let moved = self.caret != self.text.len();
        self.caret = self.text.len();
        moved
    }

    /// The caret's column, counted in `char`s from the field's start.
    ///
    /// `char`s rather than cells because the GPU face draws on a
    /// `char × char_width` grid — the same convention the compositor places
    /// the document caret with.
    #[must_use]
    pub fn caret_column(&self) -> usize {
        self.text.get(..self.caret).map_or_else(
            || self.text.chars().count(),
            |prefix| prefix.chars().count(),
        )
    }

    /// The byte range of the cluster ending at the caret, if there is one.
    fn cluster_before(&self) -> Option<Range<usize>> {
        let prefix = self.text.get(..self.caret)?;
        let (start, cluster) = prefix.grapheme_indices(true).next_back()?;
        Some(start..start + cluster.len())
    }

    /// The byte range of the cluster starting at the caret, if there is one.
    fn cluster_at(&self) -> Option<Range<usize>> {
        let suffix = self.text.get(self.caret..)?;
        let cluster = suffix.graphemes(true).next()?;
        Some(self.caret..self.caret + cluster.len())
    }

    /// Removes a byte range, refusing one that is not a valid slice.
    ///
    /// Every range this module produces comes from a cluster and is valid.
    /// The check is here because `String::drain` panics on one that is not.
    fn remove(&mut self, range: Range<usize>) -> bool {
        if range.start >= range.end
            || range.end > self.text.len()
            || !self.text.is_char_boundary(range.start)
            || !self.text.is_char_boundary(range.end)
        {
            return false;
        }
        self.text.drain(range);
        true
    }
}

/// Whether a modifier set leaves the key meaning what it says.
///
/// `Shift` is not consulted: it decided which *character* a printable key
/// produced, which the key translation has already resolved.
const fn is_plain(modifiers: Modifiers) -> bool {
    !modifiers.ctrl && !modifiers.alt && !modifiers.meta
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A family emoji: seven `char`s, one cluster.
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

    #[test]
    fn a_name_is_typed_and_becomes_a_path() {
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, "notes.md");
        assert_eq!(prompt.line(), "Save as: notes.md");
        assert_eq!(
            prompt.answer(&press(KeyCode::Enter)),
            Answer::SaveAs(PathBuf::from("notes.md"))
        );
    }

    #[test]
    fn a_named_document_is_asked_about_starting_from_the_name_it_has() {
        // The difference between a usable Save As and forty-six characters of
        // retyping, and the caret is at the end so the common edit — change the
        // extension — is one keystroke away rather than forty-six.
        let mut prompt = Prompt::save_as_named(Path::new("/tmp/notes.md"));
        assert_eq!(prompt.line(), "Save as: /tmp/notes.md");
        assert_eq!(prompt.caret_column(), Some("Save as: /tmp/notes.md".len()));

        type_into(&mut prompt, "x");
        assert_eq!(
            prompt.answer(&press(KeyCode::Enter)),
            Answer::SaveAs(PathBuf::from("/tmp/notes.mdx")),
            "typing appends at the caret rather than in front of the directory"
        );
    }

    #[test]
    fn a_prefilled_name_cannot_smuggle_in_a_control_character() {
        // The field's one invariant, and the prefill is the way in a keyboard
        // cannot use: a path comes from an `OsStr`, which permits bytes no key
        // sends. Filling through `insert` is what makes this hold by
        // construction rather than by a second check nobody would remember.
        let prompt = Prompt::save_as_named(Path::new("a\nb.md"));
        assert_eq!(prompt.line(), "Save as: ab.md");
    }

    #[test]
    fn a_confirmation_carries_the_path_its_answer_needs() {
        // `OverwriteWith` is the one deed with an argument, because the file
        // being written is deliberately *not* the one the document is attached
        // to — asking the application for "the path" when the answer comes back
        // would find the wrong one.
        let target = PathBuf::from("/tmp/already-there.md");
        let mut prompt = Prompt::confirm(
            "already-there.md exists. Overwrite it? (y/n)",
            Deed::OverwriteWith(target.clone()),
        );
        assert_eq!(
            prompt.answer(&press(KeyCode::Char('y'))),
            Answer::Do(Deed::OverwriteWith(target))
        );
    }

    #[test]
    fn submitting_an_empty_field_cancels() {
        let mut prompt = Prompt::save_as();
        assert_eq!(prompt.answer(&press(KeyCode::Enter)), Answer::Cancelled);
    }

    #[test]
    fn escape_cancels_every_prompt() {
        for mut prompt in [
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
        assert_eq!(prompt.answer(&press(KeyCode::Char('n'))), Answer::Cancelled);
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
        let meta = Modifiers {
            meta: true,
            ..Modifiers::none()
        };
        for modifiers in [Modifiers::ctrl(), meta] {
            let chord = KeyEvent {
                key: KeyCode::Char('s'),
                modifiers,
                is_repeat: false,
            };
            assert_eq!(prompt.answer(&chord), Answer::Pending);
        }
        assert_eq!(prompt.line(), "Save as: ");
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

        let mut prompt = Prompt::confirm("Quit? (y/n)", Deed::Quit);
        prompt.paste("yes");
        assert_eq!(prompt.line(), "Quit? (y/n)");
    }

    #[test]
    fn the_caret_counts_chars_on_the_compositor_grid() {
        // "Save as: " is nine chars; one ideograph is one more. Chars, not
        // cells: the GPU face draws on a char × char_width grid.
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, "\u{6f22}");
        assert_eq!(prompt.caret_column(), Some(10));
    }

    #[test]
    fn a_confirmation_has_no_caret() {
        assert_eq!(
            Prompt::confirm("Quit? (y/n)", Deed::Quit).caret_column(),
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
        assert_eq!(prompt.caret_column(), Some(11));
    }

    #[test]
    fn control_characters_never_enter_a_file_name() {
        let mut prompt = Prompt::save_as();
        type_into(&mut prompt, "a\u{7}b");
        assert_eq!(prompt.line(), "Save as: ab");
    }

    #[test]
    fn a_message_reports_its_text_and_severity() {
        let notice = Message::notice("wrote a.rs");
        assert!(!notice.is_error());
        assert_eq!(notice.text(), "wrote a.rs");

        let failure = Message::error("cannot write a.rs");
        assert!(failure.is_error());
        assert_eq!(failure.text(), "cannot write a.rs");
    }
}
