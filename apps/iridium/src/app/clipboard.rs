//! Clipboard delivery precedes cuts. Failed reads never paste cached stale text.

use super::{App, Message};
use crate::clipboard::{Backend, ClipboardMode};
use iridium_editor::workspace::DocumentId;
use iridium_editor::{ClipboardOperation, Command, CursorState};
use iridium_tui::frame::SearchCut;
use std::io;

pub(super) enum CutTarget {
    Project(Box<iridium_tui::frame::TextFieldCut>),
    Document {
        id: Option<DocumentId>,
        revision: u64,
        cursors: CursorState,
        command: Box<Command>,
    },
    Search {
        id: Option<DocumentId>,
        cut: Box<SearchCut>,
    },
}

pub(super) struct PendingClipboard {
    text: String,
    cut: Option<CutTarget>,
}

impl App {
    /// Selects clipboard transport before handling input. Library sessions
    /// start with an internal clipboard; the binary explicitly installs policy.
    pub fn configure_clipboard(&mut self, mode: ClipboardMode, remote: bool) {
        self.clipboard_backend = Backend::new(mode, remote);
    }

    pub(super) fn clipboard(&mut self, operation: ClipboardOperation) {
        match operation {
            ClipboardOperation::Copy(text) => self.store_clipboard(text, None),
            ClipboardOperation::Cut { text, command } => {
                let Some(editor) = self.workspace.active_editor() else {
                    self.message = Some(Message::error("There is no document to cut from"));
                    return;
                };
                let target = CutTarget::Document {
                    id: self.workspace.active_document(),
                    revision: editor.state().document.revision(),
                    cursors: editor.state().cursor.clone(),
                    command: Box::new(command),
                };
                self.store_clipboard(text, Some(target));
            },
            ClipboardOperation::Paste => {
                let result = match &mut self.clipboard_backend {
                    Backend::Internal => Ok(std::mem::take(&mut self.clipboard)),
                    Backend::System(provider) => provider.read(),
                    Backend::Terminal => Err(
                        "Use your terminal's paste shortcut; terminal clipboard reads are disabled"
                            .into(),
                    ),
                };
                match result {
                    Ok(text) => {
                        self.paste(&text);
                        self.clipboard = text;
                    },
                    Err(error) => self.message = Some(Message::error(error)),
                }
            },
        }
    }

    pub(super) fn cut_search(&mut self, cut: SearchCut) {
        self.store_clipboard(
            cut.text().to_owned(),
            Some(CutTarget::Search {
                id: self.workspace.active_document(),
                cut: Box::new(cut),
            }),
        );
    }

    pub(super) fn store_clipboard(&mut self, text: String, cut: Option<CutTarget>) {
        if self.pending_clipboard.is_some() {
            self.message = Some(Message::error(
                "A terminal clipboard write is pending; this operation was not applied",
            ));
            return;
        }
        let delivered = match &mut self.clipboard_backend {
            Backend::Internal => Ok(Some("Copied to internal clipboard (this process only)")),
            Backend::System(provider) => provider
                .write(&text)
                .map(|()| Some("Copied to system clipboard")),
            Backend::Terminal => {
                if text.len() > iridium_tui::driver::clipboard::MAX_CLIPBOARD_BYTES {
                    Err("Terminal copy exceeds 1 MiB; use --clipboard system or internal".into())
                } else {
                    Ok(None)
                }
            },
        };
        match delivered {
            Ok(Some(notice)) => self.clipboard_delivered(PendingClipboard { text, cut }, notice),
            Ok(None) => self.pending_clipboard = Some(PendingClipboard { text, cut }),
            Err(error) => self.message = Some(Message::error(format!("{error}; no cut applied"))),
        }
    }

    /// Flushes a queued terminal copy before accepting another input event.
    /// The writer confirms byte delivery only; OSC 52 has no acceptance receipt.
    ///
    /// # Errors
    /// A write failure leaves the cut unapplied and is returned so the host can
    /// restore the tty rather than continuing after a partial escape sequence.
    pub fn flush_terminal_clipboard(
        &mut self,
        write: impl FnOnce(&str) -> io::Result<()>,
    ) -> io::Result<()> {
        let Some(request) = self.pending_clipboard.take() else {
            return Ok(());
        };
        if let Err(error) = write(&request.text) {
            let message = format!("Terminal clipboard write failed; no cut applied: {error}");
            self.message = Some(Message::error(&message));
            self.project_clipboard_notice();
            return Err(io::Error::new(error.kind(), message));
        }
        self.clipboard_delivered(
            request,
            "Sent to terminal clipboard; acceptance depends on terminal settings",
        );
        Ok(())
    }

    fn clipboard_delivered(&mut self, request: PendingClipboard, notice: &str) {
        self.clipboard = request.text;
        let outcome = request.cut.map_or(Ok(()), |target| self.apply_cut(target));
        self.message = Some(match outcome {
            Ok(()) => Message::notice(notice),
            Err(error) => Message::error(error),
        });
        self.project_clipboard_notice();
        self.ensure_caret_visible();
    }

    fn apply_cut(&mut self, target: CutTarget) -> Result<(), String> {
        let active = self.workspace.active_document();
        match target {
            CutTarget::Project(cut) => self
                .project_search
                .as_mut()
                .ok_or("Text copied; project panel closed, so cut was cancelled")?
                .finish_field_cut(&cut),
            CutTarget::Document {
                id,
                revision,
                cursors,
                command,
            } => {
                let editor = self
                    .workspace
                    .active_editor_mut()
                    .ok_or("Text copied; document closed, so cut was cancelled")?;
                if active != id
                    || editor.state().document.revision() != revision
                    || editor.state().cursor != cursors
                    || editor.state().read_only
                {
                    return Err(
                        "Text copied; document or selection changed, so cut was cancelled".into(),
                    );
                }
                editor.apply_command(*command);
                Ok(())
            },
            CutTarget::Search { id, cut } => {
                if active != id || !self.search_open {
                    return Err("Text copied; search target changed, so cut was cancelled".into());
                }
                let (editor, session) = self
                    .workspace
                    .active_editor_and_payload_mut()
                    .ok_or("Text copied; search document closed, so cut was cancelled")?;
                session.search.finish_cut(&cut, editor)
            },
        }
    }
}
