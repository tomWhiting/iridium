//! The system pasteboard.
//!
//! Opened lazily, so a pasteboard that cannot be reached fails on the
//! keystroke that needed it — visibly — rather than at startup. The delete
//! half of a cut applies only once the pasteboard holds the text, because a
//! cut whose copy failed would destroy the only copy.

use iridium_editor::ClipboardOperation;

use super::state::DesktopApp;
use crate::prompt::Message;

impl DesktopApp {
    /// Keeps or supplies clipboard text, on the system pasteboard.
    ///
    /// The delete half of a cut is the kernel's own reversible command,
    /// applied unchanged — and applied only once the pasteboard holds the
    /// text, because a cut whose copy failed would destroy the only copy.
    pub(super) fn clipboard(&mut self, operation: ClipboardOperation) {
        match operation {
            ClipboardOperation::Copy(text) => {
                if let Err(error) = self.clipboard_store(text) {
                    self.message = Some(Message::error(error));
                }
            },
            ClipboardOperation::Cut { text, command } => match self.clipboard_store(text) {
                // The text is on the pasteboard by now, so a missing tab loses
                // nothing — but the delete half did not run, and a cut that
                // silently left the text behind would read as a broken chord.
                Ok(()) => match self.workspace.active_editor_mut() {
                    Some(editor) => editor.apply_command(command),
                    None => {
                        self.message = Some(Message::error("there is no document to cut from"));
                    },
                },
                Err(error) => {
                    self.message = Some(Message::error(format!(
                        "{error} — the cut left the document untouched"
                    )));
                },
            },
            ClipboardOperation::Paste => match self.clipboard_text() {
                Ok(text) => match self.workspace.active_editor_mut() {
                    Some(editor) => editor.paste(&text),
                    None => {
                        self.message = Some(Message::error("there is no document to paste into"));
                    },
                },
                Err(message) => self.message = Some(message),
            },
        }
    }

    /// The system clipboard, opened on first use and kept.
    ///
    /// # Errors
    ///
    /// A user-visible sentence when the pasteboard cannot be reached.
    fn clipboard_handle(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.clipboard.is_none() {
            match arboard::Clipboard::new() {
                Ok(clipboard) => self.clipboard = Some(clipboard),
                Err(error) => {
                    return Err(format!("cannot reach the system clipboard: {error}"));
                },
            }
        }
        self.clipboard
            .as_mut()
            .ok_or_else(|| "cannot reach the system clipboard".to_owned())
    }

    /// Puts text on the system pasteboard.
    fn clipboard_store(&mut self, text: String) -> Result<(), String> {
        let clipboard = self.clipboard_handle()?;
        clipboard
            .set_text(text)
            .map_err(|error| format!("cannot write to the clipboard: {error}"))
    }

    /// Reads text from the system pasteboard.
    ///
    /// # Errors
    ///
    /// A ready-to-show [`Message`]: a pasteboard holding no text is a notice
    /// — it is an answer, not a failure — and anything else is an error.
    pub(super) fn clipboard_text(&mut self) -> Result<String, Message> {
        let clipboard = self.clipboard_handle().map_err(Message::error)?;
        match clipboard.get_text() {
            Ok(text) => Ok(text),
            Err(arboard::Error::ContentNotAvailable) => {
                Err(Message::notice("the clipboard holds no text"))
            },
            Err(error) => Err(Message::error(format!(
                "cannot read the clipboard: {error}"
            ))),
        }
    }
}
