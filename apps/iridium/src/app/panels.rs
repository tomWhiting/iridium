//! Modal panel routing and project explorer lifecycle.
use super::prompt::Message;
use super::{App, Flow};
use crate::file::TextFile;
use iridium_editor::input::CommandRunError;
use iridium_editor::{CommandArgs, CommandId, KeyEvent};
use iridium_tui::frame::{FileExplorerPanel, HistoryOutcome, PaletteOutcome};

impl App {
    /// Repaint pending directory reads without waking an idle editor.
    #[must_use]
    pub fn next_frame_timeout(&self) -> Option<std::time::Duration> {
        let panel = (self.project_search_waiting()
            || self.query_waiting()
            || self
                .explorer
                .as_ref()
                .is_some_and(FileExplorerPanel::is_waiting))
        .then_some(std::time::Duration::from_millis(16));
        let mouse = self
            .mouse_scroll
            .as_ref()
            .map(super::drag_scroll::DragScroll::remaining);
        panel.into_iter().chain(mouse).min()
    }

    /// Hands a key to the open undo-tree panel and acts on the outcome.
    pub(super) fn drive_history(&mut self, event: &KeyEvent) -> Flow {
        let Some(editor) = self.workspace.active_editor_mut() else {
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
                if !editor.jump_to_history_node(node) {
                    self.message = Some(Message::error(
                        "that history state no longer exists".to_owned(),
                    ));
                }
                self.ensure_caret_visible();
                Flow::Running
            },
        }
    }

    /// Opens the file explorer, or closes it if it is already up.
    ///
    /// ⚠️ **Refused while the panel holds unapplied edits**, exactly as the GPU
    /// face refuses it. Closing here drops the panel outright, so the refusal
    /// `FileExplorer` keeps for its own `Escape` is never asked — it has to be
    /// asked here instead, or a directory's worth of typed renames goes with
    /// one chord and nothing says so.
    pub(super) fn toggle_explorer(&mut self) -> Flow {
        let band = self.sidebar_columns();
        if let Some(explorer) = self.explorer.as_ref() {
            if explorer.has_unapplied_edits() {
                self.message = Some(Message::error(explorer.unapplied_edits_message()));
                return Flow::Running;
            }
            self.close_explorer();
            self.resync_after_band_change(band);
            return Flow::Running;
        }
        if let Some(bookmark) = self.explorer_bookmark.as_ref() {
            match FileExplorerPanel::open_bookmark(bookmark, &self.user_keys) {
                Ok(explorer) => {
                    self.explorer = Some(explorer);
                    self.focus = crate::app::sidebar::Focus::Explorer;
                },
                Err(error) => {
                    self.message = Some(Message::error(format!("the file explorer: {error}")));
                },
            }
            self.resync_after_band_change(band);
            return Flow::Running;
        }
        match self.explorer_root() {
            Ok(root) => self.open_explorer(root),
            Err(error) => {
                self.message = Some(Message::error(format!(
                    "cannot locate the project directory: {error}"
                )));
            },
        }
        self.resync_after_band_change(band);
        Flow::Running
    }

    /// Opens the shared browser; the caller chooses picker or persistent sidebar.
    pub(super) fn open_explorer(&mut self, root: iridium_panel::explorer::ExplorerRoot) {
        match FileExplorerPanel::open(root.path, root.crawl, &self.user_keys) {
            Ok(explorer) => {
                self.explorer = Some(explorer);
                self.focus = crate::app::sidebar::Focus::Explorer;
            },
            Err(error) => {
                self.message = Some(Message::error(format!("the file explorer: {error}")));
            },
        }
    }

    /// Called only after the panel or host has resolved unapplied edits.
    pub(super) fn close_explorer(&mut self) {
        self.focus = crate::app::sidebar::Focus::Document;
        if let Some(explorer) = self.explorer.take() {
            self.explorer_bookmark = Some(explorer.bookmark());
        }
    }

    /// Writes the kernel's viewport again when the left band's width moved.
    ///
    /// ⚠️ **The frame's geometry and the kernel's viewport are two different
    /// things.** `Chrome` reaches the geometry every frame, so what is *drawn*
    /// follows the band immediately; the kernel's own viewport is only written
    /// by `sync_viewport`, and it is what the scroll verbs measure against. Skip
    /// this and the document draws in the columns it has while the kernel keeps
    /// scrolling against the columns it used to have.
    ///
    /// Guarded on an actual change to avoid recomputing geometry for every
    /// explorer key. The dedicated viewport API preserves keyboard state.
    pub(super) fn resync_after_band_change(&mut self, before: usize) {
        if self.sidebar_columns() != before {
            self.sync_viewport();
        }
    }

    /// Where this face opens the explorer when nobody said.
    ///
    /// ⭐ **The *choice* is [`chosen_root`] and is shared; this *guess* is this
    /// face's, and the two faces genuinely guess differently.** The GPU face
    /// must distrust its working directory — a bundled application launched
    /// from Finder inherits `/`, and rooting there once sent a search crawl
    /// across an entire disk. A terminal has the opposite property: its working
    /// directory is where a person `cd`-ed to, which is the most reliable
    /// statement of intent either face ever gets.
    ///
    /// So: the open file's folder if there is one, and otherwise the working
    /// directory, trusted. `chosen_root` then answers the only question that
    /// means the same thing in both faces — whether a search may read past it.
    pub(super) fn explorer_root(&self) -> std::io::Result<iridium_panel::explorer::ExplorerRoot> {
        let from_file = self
            .file()
            .map(TextFile::path)
            .and_then(|path| path.parent())
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(std::path::Path::to_path_buf);
        self.project
            .clone()
            .or(from_file)
            .map_or_else(std::env::current_dir, Ok)
            .map(iridium_panel::explorer::chosen_root)
    }

    /// Hands a key to the open explorer and acts on the outcome.
    pub(super) fn drive_explorer(&mut self, event: &KeyEvent) -> Flow {
        let band = self.sidebar_columns();
        let Some(explorer) = self.explorer.as_mut() else {
            return Flow::Running;
        };
        let action = explorer.handle_key(event);
        let flow = self.explorer_action(action);
        // The sidebar chord reports `Handled` like any other key — the
        // placement is the panel's own state — so the band's width is what
        // says it moved.
        self.resync_after_band_change(band);
        flow
    }

    /// Hands a key to the open palette and acts on the outcome.
    pub(super) fn drive_palette(&mut self, event: &KeyEvent) -> Flow {
        if event.key == iridium_editor::KeyCode::Enter
            && !event.modifiers.ctrl
            && !event.modifiers.alt
            && !event.modifiers.meta
        {
            if let Some(filter) =
                super::project_search::command_filter(self.palette.query()).map(str::to_owned)
            {
                return self.open_project_search(&filter);
            }
            if let Some(filter) =
                super::query::command_filter(self.palette.query()).map(str::to_owned)
            {
                return self.open_query(&filter);
            }
            if let Some(command) = super::ex::parse(self.palette.query()) {
                self.palette_open = false;
                return self.run_palette_command(&command);
            }
        }
        let Some(editor) = self.workspace.active_editor() else {
            return Flow::Running;
        };
        match self.palette.handle_key(event, editor, &self.mru) {
            PaletteOutcome::Handled => {
                self.start_palette_query();
                Flow::Running
            },
            PaletteOutcome::Closed => {
                self.palette_open = false;
                Flow::Running
            },
            PaletteOutcome::Run(command) => {
                self.palette_open = false;
                self.run_palette_command(&command)
            },
        }
    }

    /// Runs a command the palette resolved: the kernel's own if it implements
    /// it, this face's if not, and an honest message when neither does.
    ///
    /// The recency list records only commands that actually dispatched, so a
    /// palette entry nothing runs cannot become a ranked favourite. Read-only
    /// refusals are not failures here: the kernel consumes them the same way a
    /// keypress does, silently leaving the document alone.
    pub(super) fn run_palette_command(&mut self, command: &CommandId) -> Flow {
        let Some(editor) = self.workspace.active_editor_mut() else {
            return Flow::Running;
        };
        let before = editor.state().cursor.clone();
        let revision = editor.state().document.revision();
        match editor.run_command(command.as_str(), CommandArgs::NONE) {
            Ok(result) => {
                let selected = editor.state().document.revision() == revision
                    && editor.state().cursor != before
                    && editor
                        .state()
                        .cursor
                        .all_selections()
                        .any(|s| !s.is_collapsed());
                self.mru.record(command);
                let flow = self.consume(result);
                if selected {
                    self.select_mode();
                }
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
                    "`{command}` is not available in the terminal yet"
                )));
                Flow::Running
            },
        }
    }
}

impl App {
    pub(super) fn start_palette_query(&mut self) {
        let text = self.palette.query();
        if text.trim_start().starts_with("grep ")
            && let Some(filter) = super::project_search::command_filter(text).map(str::to_owned)
        {
            self.open_project_search(&filter);
            return;
        }
        if text.trim_start().starts_with("jq ")
            && let Some(filter) = super::query::command_filter(text).map(str::to_owned)
        {
            self.open_query(&filter);
        }
    }
}
