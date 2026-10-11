//! Project-search lifecycle; the worker owns scanning, the workspace owns text.
pub(super) mod clipboard;
mod mouse;
pub(super) mod panel;
mod review;
mod selection;
#[cfg(test)]
mod tests;
use super::{App, Flow, prompt::Message};
use iridium_editor::{KeyCode, KeyEvent, workspace::DocumentId};
use iridium_search::{Buffer, Job, Origin, SNAPSHOT_LIMIT};
use panel::Panel;
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};

impl App {
    pub(super) fn open_project_search(&mut self, text: &str) -> Flow {
        if text.len() > iridium_search::QUERY_LIMIT {
            self.message = Some(Message::error("Project query exceeds 16 KiB"));
            return Flow::Running;
        }
        self.palette_open = false;
        if self.project_search.is_none() {
            self.project_search = Some(Panel::new());
        }
        if let Some(panel) = self.project_search.as_mut() {
            panel.open = true;
            panel.clipboard_keys.set_user_keymap(&self.user_keys);
            if !text.is_empty() {
                panel.set_text(text);
            }
            panel.restart();
        }
        Flow::Running
    }
    pub(super) fn project_search_waiting(&self) -> bool {
        self.project_search.as_ref().is_some_and(Panel::waiting)
    }
    pub(super) fn project_search_open(&self) -> bool {
        self.project_search.as_ref().is_some_and(|panel| panel.open)
    }
    pub(super) fn poll_project_search(&mut self) {
        let Some(mut panel) = self.project_search.take() else {
            return;
        };
        panel.poll();
        if let Some(review) = panel.review.as_mut()
            && let Some(mut operation) = review.operation.take()
            && !operation.poll(self, review)
        {
            review.operation = Some(operation);
        }
        if panel.job.is_none() && panel.due.is_some_and(|due| Instant::now() >= due) {
            panel.due = None;
            if panel.query.text.is_empty() {
                panel.feedback = "Type a project search".into();
            } else {
                let result = self
                    .explorer_root()
                    .map_err(|e| e.to_string())
                    .and_then(|root| std::fs::canonicalize(root.path).map_err(|e| e.to_string()))
                    .and_then(|root| {
                        self.search_buffers(&root).and_then(|buffers| {
                            panel.root.clone_from(&root);
                            Job::start(root, &panel.query, buffers)
                        })
                    });
                match result {
                    Ok(job) => {
                        panel.job = Some(job);
                        panel.feedback = "Searching…".into();
                    },
                    Err(error) => panel.feedback = error,
                }
            }
        }
        self.project_search = Some(panel);
    }
    fn search_buffers(&self, root: &std::path::Path) -> Result<Vec<Buffer>, String> {
        let mut buffers = Vec::new();
        let mut ids = HashSet::new();
        let mut bytes = 0usize;
        for tab in self.workspace.tabs() {
            let Some(id) = self
                .workspace
                .node(tab)
                .and_then(iridium_editor::workspace::Node::document)
            else {
                continue;
            };
            if !ids.insert(id) {
                continue;
            }
            let Some(file) = self.workspace.payload(id).and_then(|s| s.file.as_ref()) else {
                continue;
            };
            if !file.path().starts_with(root) {
                continue;
            }
            let Some(editor) = self.workspace.editor(id) else {
                return Err("search document disappeared".into());
            };
            bytes = bytes
                .checked_add(editor.state().document.rope().len())
                .ok_or("snapshot size overflow")?;
            if bytes > SNAPSHOT_LIMIT {
                return Err("open-buffer search snapshots exceed 32 MiB".into());
            }
            buffers.push(Buffer {
                id: id.as_raw(),
                revision: editor.state().document.revision(),
                path: file.path().to_owned(),
                text: Arc::from(editor.content()),
            });
        }
        Ok(buffers)
    }
    pub(super) fn drive_project_search(&mut self, event: &KeyEvent) -> Flow {
        if self.drive_project_clipboard(event) {
            return Flow::Running;
        }
        if let Some(panel) = self.project_search.as_mut() {
            panel.clipboard_message = None;
        }
        if self
            .project_search
            .as_ref()
            .is_some_and(|p| p.review.is_some())
        {
            if let Some(mut panel) = self.project_search.take() {
                if let Some(review) = panel.review.as_mut() {
                    if let review::Action::Apply(save) = review.key(event, self.columns, self.rows)
                    {
                        match review::operation::Operation::new(review, save, self) {
                            Ok(operation) => {
                                review.operation = Some(operation);
                                review.feedback = "Checking all documents before edits…".into();
                            },
                            Err(error) => review.feedback = error,
                        }
                    }
                }
                self.project_search = Some(panel);
            }
            return Flow::Running;
        }
        if event.key == KeyCode::F5 {
            if let Some(panel) = self.project_search.as_mut() {
                panel.open_review();
            }
            return Flow::Running;
        }
        if event.key == KeyCode::Enter && !event.modifiers.ctrl && !event.modifiers.meta {
            self.open_project_match(event.modifiers.alt);
        } else if let Some(panel) = self.project_search.as_mut() {
            panel.key(event);
        }
        Flow::Running
    }
    fn open_project_match(&mut self, all: bool) {
        let hit = self
            .project_search
            .as_ref()
            .and_then(|panel| panel.matches.get(panel.selected))
            .cloned();
        let Some(hit) = hit else {
            return;
        };
        let hits: Vec<_> = if all {
            self.project_search
                .as_ref()
                .map(|panel| {
                    panel
                        .matches
                        .iter()
                        .filter(|other| other.path == hit.path)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![hit]
        };
        let outcome = self.navigate_project_matches(&hits);
        match outcome {
            Ok(()) => {
                if let Some(panel) = self.project_search.as_mut() {
                    panel.close();
                }
                self.message = Some(Message::notice(
                    "Project match selected · :grep reopens results",
                ));
            },
            Err(error) => {
                if let Some(panel) = self.project_search.as_mut() {
                    panel.feedback = error;
                }
            },
        }
    }
    fn navigate_project_matches(&mut self, hits: &[iridium_search::Match]) -> Result<(), String> {
        let hit = hits.first().ok_or("no matches selected")?;
        for source in hits {
            if let Origin::Buffer { id, revision } = source.origin {
                let id = DocumentId::from_raw(id);
                let editor = self
                    .workspace
                    .editor(id)
                    .ok_or("source buffer was closed; rerun search")?;
                if editor.state().document.revision() != revision {
                    return Err("source buffer changed; rerun search".into());
                }
                if self
                    .workspace
                    .payload(id)
                    .and_then(|session| session.file.as_ref())
                    .is_none_or(|file| file.path() != source.path.as_ref())
                {
                    return Err("source buffer path changed; rerun search".into());
                }
            }
        }
        if self.tab_for_path(&hit.path).is_none() {
            let size = std::fs::metadata(hit.path.as_ref())
                .map_err(|e| e.to_string())?
                .len();
            if size > 64 * 1024 * 1024 {
                return Err(
                    "match found; opening files over 64 MiB awaits paged editor integration".into(),
                );
            }
        }
        self.add_file_with_limit(&hit.path, 64 * 1024 * 1024)
            .map_err(|e| e.to_string())?;
        let editor = self
            .workspace
            .active_editor_mut()
            .ok_or("source document is unavailable")?;
        let selections = selection::prepare(&editor.state().document, hits)?;
        editor.set_selections(selections);
        editor.reveal_selection_heads();
        self.select_mode();
        self.ensure_caret_visible();
        Ok(())
    }
}
pub(super) fn command_filter(text: &str) -> Option<&str> {
    let tail = text.trim_start().strip_prefix("grep")?;
    (tail.is_empty() || tail.starts_with(char::is_whitespace)).then(|| tail.trim_start())
}
