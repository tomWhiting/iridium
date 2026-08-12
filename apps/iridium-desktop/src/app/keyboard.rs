//! The keyboard's modal ladder, and what the kernel makes of a key.
//!
//! [`DesktopApp::press`] is the ladder: the open prompt, then the context
//! menu, then the modal palette, then the modal undo-tree panel, then the
//! search panel (which leaves unbound keys to the kernel), then the kernel.
//! Every rung is one method here, and each returns without consulting the
//! next — "modal" is exactly that shape and nothing more.

use iridium_editor::input::{CommandRunError, SearchAction};
use iridium_editor::{CommandArgs, CommandId, EditorKeyResult, KeyCode, KeyEvent};

use super::state::{DesktopApp, Flow};
use crate::command_palette::PaletteOutcome;
use crate::file_tree::ExplorerOutcome;
use crate::history_overlay::HistoryOutcome;
use crate::prompt::{Answer, Deed, Message};
use crate::search::SearchOutcome;

impl DesktopApp {
    /// Handles one translated key press, in the terminal face's order: the
    /// open prompt if there is one, then the modal palette, then the modal
    /// undo-tree panel, then the search panel (which leaves unbound keys to
    /// the kernel), then the kernel — then scroll-to-caret and a repaint.
    pub(super) fn press(&mut self, event: &KeyEvent) -> Flow {
        // A message describes the key before this one.
        self.message = None;

        let flow = if self.prompt.is_some() {
            self.prompt_key(event)
        } else if self.menu.is_some() {
            self.drive_menu(event)
        } else if self.palette_open {
            self.drive_palette(event)
        } else if self.history_open {
            self.drive_history(event)
        } else if self.explorer_has_focus() {
            self.drive_explorer(event)
        } else {
            self.search_or_document_key(event)
        };

        self.refresh_title();
        if let Some(shell) = &mut self.shell {
            // A caret that stays in its off-phase across a movement looks
            // like a caret that vanished.
            shell.compositor.reset_blink();
            shell.window.request_redraw();
        }
        flow
    }

    /// Hands a key to the open search panel, and to the kernel for anything
    /// the panel does not bind.
    ///
    /// The panel leaves what it does not bind to the host, so a save chord is
    /// not dead while a search is open — the exact rule the terminal face
    /// applies.
    fn search_or_document_key(&mut self, event: &KeyEvent) -> Flow {
        // Bound from the workspace rather than through a `&mut self` method:
        // an accessor taking `&mut self` would borrow the whole struct and
        // make `self.search` unreachable in the same expression. Disjoint
        // field borrows are what let these two coexist.
        let Some(editor) = self.workspace.active_editor_mut() else {
            return Flow::Running;
        };
        if self.search_open {
            match self.search.handle_key(event, editor) {
                SearchOutcome::Ignored => {},
                SearchOutcome::Handled | SearchOutcome::Replaced(_) => {
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
                SearchOutcome::Closed => {
                    self.search_open = false;
                    self.ensure_caret_visible();
                    return Flow::Running;
                },
            }
        }
        let Some(editor) = self.workspace.active_editor_mut() else {
            return Flow::Running;
        };
        let result = editor.handle_key(event);
        let flow = self.consume(result);
        self.ensure_caret_visible();
        flow
    }

    /// Hands a key to the open palette and acts on the outcome.
    ///
    /// One chord is intercepted before the palette sees it: the paste chord,
    /// which pastes the system clipboard into the query — the palette is
    /// modal, so a paste belongs to its field, not to the document under it.
    fn drive_palette(&mut self, event: &KeyEvent) -> Flow {
        if is_paste_chord(event) {
            match self.clipboard_text() {
                Ok(text) => self.palette.paste(&text),
                Err(message) => self.message = Some(message),
            }
            return Flow::Running;
        }
        let Some(editor) = self.workspace.active_editor() else {
            return Flow::Running;
        };
        match self.palette.handle_key(event, editor, &self.mru) {
            PaletteOutcome::Handled => Flow::Running,
            PaletteOutcome::Closed => {
                self.palette_open = false;
                Flow::Running
            },
            PaletteOutcome::Run(command) => {
                self.palette_open = false;
                self.run_chosen_command(&command)
            },
        }
    }

    /// Hands a key to the open file explorer and acts on the outcome.
    ///
    /// Opening a file goes through [`open_file`](Self::open_file) — the same
    /// path a drop takes — so the explorer cannot grow its own idea of what
    /// opening means. The panel closes on the way, because the thing it was
    /// for has happened.
    ///
    /// This comment used to name `Ctrl+O` as a third caller. There is no such
    /// binding and never has been: `open_file` has exactly two entry points,
    /// a drop and this panel. The explorer *is* how a file gets opened by
    /// keyboard, which is why it is bound where a file-open dialog would be.
    ///
    /// The paste chord is intercepted first, for the reason it is intercepted
    /// for the palette: the panel is modal and has a text field, so a paste
    /// belongs to that field rather than to the document underneath it.
    fn drive_explorer(&mut self, event: &KeyEvent) -> Flow {
        if is_paste_chord(event) {
            match self.clipboard_text() {
                Ok(text) => {
                    if let Some(explorer) = self.explorer.as_mut() {
                        explorer.paste(&text);
                    }
                },
                Err(message) => self.message = Some(message),
            }
            return Flow::Running;
        }
        let Some(explorer) = self.explorer.as_mut() else {
            return Flow::Running;
        };
        match explorer.handle_key(event) {
            ExplorerOutcome::Handled => Flow::Running,
            // ⭐ **What `Escape` means depends on the placement, and this is
            // the only place that difference is spelled.** A popover is a
            // thing you open, use and dismiss, so it goes. A sidebar is a
            // thing you keep open beside the code — dismissing it on the key
            // that means "give me the document back" would make it a panel you
            // had to reopen after every glance, which is not a sidebar.
            ExplorerOutcome::Dismissed => {
                self.leave_explorer();
                Flow::Running
            },
            // ⚠️ The *toggle chord*, which is a different sentence: the panel
            // goes away in either placement. These two arms were one until a
            // test found `⌘⌥E` doing nothing to a sidebar.
            ExplorerOutcome::Closed => {
                self.explorer = None;
                self.sync_left_inset();
                Flow::Running
            },
            ExplorerOutcome::Open(path) => {
                self.leave_explorer();
                self.open_file(&path);
                Flow::Running
            },
            // The panel asks; the host owns the placement — see
            // [`DesktopApp::toggle_explorer_placement`].
            ExplorerOutcome::ToggleSidebar => self.toggle_sidebar(),
            // The panel stays open. What failed is one operation inside it,
            // not the panel, and closing over an error would take away the
            // thing the user was in the middle of.
            ExplorerOutcome::Failed(message) => {
                self.message = Some(Message::error(message));
                Flow::Running
            },
        }
    }

    /// Hands a key to the open undo-tree panel and acts on the outcome.
    fn drive_history(&mut self, event: &KeyEvent) -> Flow {
        let Some(editor) = self.workspace.active_editor() else {
            return Flow::Running;
        };
        match self.history.handle_key(event, editor) {
            HistoryOutcome::Handled => Flow::Running,
            HistoryOutcome::Closed => {
                self.history_open = false;
                Flow::Running
            },
            HistoryOutcome::Jump(node) => {
                // The panel stays open: hopping between states and watching
                // the document change underneath is what the tree is for.
                let jumped = self
                    .workspace
                    .active_editor_mut()
                    .is_some_and(|editor| editor.jump_to_history_node(node));
                if !jumped {
                    self.message = Some(Message::error(
                        "that history state no longer exists".to_owned(),
                    ));
                }
                self.ensure_caret_visible();
                Flow::Running
            },
        }
    }

    /// Runs a command a panel resolved — the palette's `Enter`, or the
    /// context menu's row: the kernel's own if it implements it, this face's
    /// if not, and an honest message when neither does.
    ///
    /// There is deliberately one of these. A menu verb reaches the document
    /// through the same kernel-first, command-sourced, undoable path a chord
    /// and a palette entry do; a second dispatch would be a second set of
    /// semantics to keep in step.
    ///
    /// The recency list records only commands that actually dispatched, so an
    /// entry nothing runs cannot become a ranked favourite — and a verb run
    /// from the menu trains the same ranking a palette run does.
    pub(super) fn run_chosen_command(&mut self, command: &CommandId) -> Flow {
        let Some(editor) = self.workspace.active_editor_mut() else {
            return Flow::Running;
        };
        match editor.run_command(command.as_str(), CommandArgs::NONE) {
            Ok(result) => {
                self.mru.record(command);
                let flow = self.consume(result);
                self.ensure_caret_visible();
                flow
            },
            Err(CommandRunError::Unimplemented { .. }) => {
                if let Some(flow) = self.dispatch_host_command(command) {
                    self.mru.record(command);
                    self.ensure_caret_visible();
                    return flow;
                }
                self.message = Some(Message::error(format!(
                    "`{command}` is not available in the desktop yet"
                )));
                Flow::Running
            },
        }
    }

    /// Hands a key to the open prompt and acts on the answer.
    ///
    /// One chord is intercepted before the prompt sees it: the paste chord
    /// (`Ctrl+V`/`⌘V`), which pastes the system clipboard into the prompt's
    /// field — typing a long path by hand is the alternative. Everything
    /// else is the prompt's, and the prompt swallows what it does not name.
    fn prompt_key(&mut self, event: &KeyEvent) -> Flow {
        if is_paste_chord(event) {
            match self.clipboard_text() {
                Ok(text) => {
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.paste(&text);
                    }
                },
                // Shown once the prompt closes; a clipboard failure must not
                // tear down the question being asked.
                Err(message) => self.message = Some(message),
            }
            return Flow::Running;
        }

        let Some(prompt) = self.prompt.as_mut() else {
            return Flow::Running;
        };
        let answer = prompt.answer(event);
        if matches!(answer, Answer::Pending) {
            return Flow::Running;
        }
        self.prompt = None;
        match answer {
            Answer::Pending | Answer::Cancelled => Flow::Running,
            Answer::SaveAs(path) => self.save_as(&path),
            Answer::Do(Deed::Quit) => Flow::Exit,
            // Forced: the question that got here *was* the guard, and asking
            // it again from inside the answer would never terminate.
            Answer::Do(Deed::CloseTab) => self.close_active_tab(true),
            // The path travels in the answer rather than being looked up here,
            // for the reason `Deed` states: the document is still attached to
            // the file it had, so "the path" asked for now would be the wrong
            // one.
            Answer::Do(Deed::OverwriteWith(path)) => self.overwrite_as(&path),
        }
    }

    /// Acts on what the kernel made of a key.
    fn consume(&mut self, result: EditorKeyResult) -> Flow {
        match result {
            EditorKeyResult::None => Flow::Running,
            EditorKeyResult::Clipboard(operation) => {
                self.clipboard(operation);
                Flow::Running
            },
            EditorKeyResult::Search(action) => {
                self.search_action(&action);
                Flow::Running
            },
            // The count and captures are not read for the same reason the
            // terminal face leaves them: nothing in this face's keymap can
            // produce them.
            EditorKeyResult::HostCommand { command, .. } => self.run_host_command(&command),
        }
    }

    /// Opens or closes the search panel.
    ///
    /// Only the two panel actions need anything here: the kernel has already
    /// moved to the next or previous match by the time it reports one.
    fn search_action(&mut self, action: &SearchAction) {
        let Some(editor) = self.workspace.active_editor_mut() else {
            return;
        };
        match action {
            SearchAction::OpenSearch => {
                self.search_open = true;
                self.search.open(editor);
            },
            SearchAction::CloseSearch => {
                if self.search_open {
                    self.search_open = false;
                    self.search.close(editor);
                }
            },
            SearchAction::NextMatch | SearchAction::PreviousMatch => {},
        }
    }
}

/// Whether a key event is the paste chord, in either its mac or its portable
/// spelling.
///
/// Consulted only while a prompt is open, where the keymap cannot run: the
/// prompt is modal, and this is the one chord it forwards to the field.
const fn is_paste_chord(event: &KeyEvent) -> bool {
    matches!(event.key, KeyCode::Char('v' | 'V'))
        && (event.modifiers.ctrl || event.modifiers.meta)
        && !event.modifiers.alt
}
