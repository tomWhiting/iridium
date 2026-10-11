//! Incremental multi-document preflight and explicit per-file commit outcomes.
use super::Review;
use crate::app::App;
use iridium_editor::{CheckedEdit, workspace::DocumentId};
use iridium_search::{Origin, Replacement, SNAPSHOT_LIMIT};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
};

struct Prepared {
    id: DocumentId,
    revision: u64,
    path: PathBuf,
    edits: Vec<CheckedEdit>,
}
enum Phase {
    Prepare,
    Recheck,
    Apply,
}
pub(in crate::app::project_search) struct Operation {
    pending: VecDeque<Vec<Replacement>>,
    prepared: VecDeque<Prepared>,
    phase: Phase,
    recheck: usize,
    total_bytes: usize,
    save: bool,
    cancelled: bool,
    original_tab: Option<iridium_editor::workspace::NodeId>,
}
impl Operation {
    pub fn new(review: &Review, save: bool, app: &App) -> Result<Self, String> {
        let preview = review
            .preview
            .as_ref()
            .ok_or("no completed replacement preview")?;
        let mut groups = BTreeMap::new();
        for (entry, included) in preview.iter().zip(&review.included) {
            if *included && entry.original() != entry.text() {
                groups
                    .entry(entry.source().path.clone())
                    .or_insert_with(Vec::new)
                    .push(entry.clone());
            }
        }
        if groups.is_empty() {
            return Err("no included changes to apply".into());
        }
        if groups.len() > 128 {
            return Err("replacement exceeds 128 files; narrow the selection".into());
        }
        Ok(Self {
            pending: groups.into_values().collect(),
            prepared: VecDeque::new(),
            phase: Phase::Prepare,
            recheck: 0,
            total_bytes: 0,
            save,
            cancelled: false,
            original_tab: app.workspace.active(),
        })
    }
    pub const fn cancel(&mut self) {
        self.cancelled = true;
    }
    /// One file per frame. Input can cancel between files; OS I/O is synchronous.
    pub fn poll(&mut self, app: &mut App, review: &mut Review) -> bool {
        if self.cancelled {
            self.abort(
                review,
                "cancelled; earlier applied files retain their reported outcomes",
            );
            return self.finish(app, review);
        }
        match self.phase {
            Phase::Prepare => {
                let Some(entries) = self.pending.pop_front() else {
                    self.phase = Phase::Recheck;
                    review.feedback = "Rechecking every file before edits…".into();
                    return false;
                };
                let path = entries[0].source().path.clone();
                match self.prepare(app, &entries) {
                    Ok(prepared) => {
                        review.feedback = format!("Prepared {}", path.display());
                        self.prepared.push_back(prepared);
                    },
                    Err(error) => {
                        review
                            .outcomes
                            .push(format!("NOT APPLIED {}: {error}", path.display()));
                        self.abort(review, "preflight failed; no files edited");
                        return self.finish(app, review);
                    },
                }
            },
            Phase::Recheck => {
                let Some(prepared) = self.prepared.get(self.recheck) else {
                    self.phase = Phase::Apply;
                    return false;
                };
                if let Err(error) = verify(app, prepared) {
                    self.abort(
                        review,
                        &format!("preflight failed: {error}; no files edited"),
                    );
                    return self.finish(app, review);
                }
                self.recheck += 1;
            },
            Phase::Apply => {
                let Some(prepared) = self.prepared.pop_front() else {
                    return self.finish(app, review);
                };
                let result = verify(app, &prepared).and_then(|()| {
                    app.workspace
                        .editor_mut(prepared.id)
                        .ok_or_else(|| "document disappeared".to_owned())?
                        .apply_checked_edits(prepared.revision, &prepared.edits)
                        .map(|_| ())
                });
                match result {
                    Err(error) => review
                        .outcomes
                        .push(format!("NOT APPLIED {}: {error}", prepared.path.display())),
                    Ok(()) if !self.save => review.outcomes.push(format!(
                        "APPLIED, UNSAVED {} · undo once in this document",
                        prepared.path.display()
                    )),
                    Ok(()) => {
                        let saved = save(app, prepared.id);
                        match saved {
                            Ok(()) => review
                                .outcomes
                                .push(format!("APPLIED, SAVED {}", prepared.path.display())),
                            Err(error) => review.outcomes.push(format!(
                                "APPLIED {} · {error} · edit remains undoable",
                                prepared.path.display()
                            )),
                        }
                    },
                }
                review.feedback = format!(
                    "{} file outcomes · {} remaining",
                    review.outcomes.len(),
                    self.prepared.len()
                );
            },
        }
        false
    }
    fn prepare(&mut self, app: &mut App, entries: &[Replacement]) -> Result<Prepared, String> {
        let hit = entries.first().ok_or("no source matches")?.source();
        let size = if let Some(tab) = app.tab_for_path(&hit.path) {
            let id = app
                .workspace
                .node(tab)
                .and_then(iridium_editor::workspace::Node::document)
                .ok_or("source tab disappeared")?;
            app.workspace
                .editor(id)
                .ok_or("source editor disappeared")?
                .state()
                .document
                .byte_count()
        } else {
            usize::try_from(
                std::fs::metadata(hit.path.as_ref())
                    .map_err(|e| e.to_string())?
                    .len(),
            )
            .map_err(|e| e.to_string())?
        };
        self.total_bytes = self
            .total_bytes
            .checked_add(size)
            .ok_or("replacement source size overflow")?;
        if self.total_bytes > SNAPSHOT_LIMIT {
            return Err("replacement source documents exceed 32 MiB; narrow selection".into());
        }
        if entries
            .iter()
            .any(|entry| entry.source().origin != hit.origin)
        {
            return Err("source matches disagree about the file revision".into());
        }
        match &hit.origin {
            Origin::Disk(stamp) => {
                if !stamp.agrees(&hit.path).map_err(|e| e.to_string())? {
                    return Err("disk file changed since search; rerun query".into());
                }
            },
            Origin::Buffer { id, revision } => {
                let id = DocumentId::from_raw(*id);
                let editor = app.workspace.editor(id).ok_or("source buffer closed")?;
                if editor.state().document.revision() != *revision {
                    return Err("source buffer changed since search".into());
                }
                if app
                    .workspace
                    .payload(id)
                    .and_then(|s| s.file.as_ref())
                    .is_none_or(|file| file.path() != hit.path.as_ref())
                {
                    return Err("source buffer path changed".into());
                }
            },
        }
        app.add_file_with_limit(
            &hit.path,
            SNAPSHOT_LIMIT.saturating_sub(self.total_bytes.saturating_sub(size)),
        )
        .map_err(|e| e.to_string())?;
        let id = app
            .workspace
            .active_document()
            .ok_or("no active source document")?;
        let editor = app
            .workspace
            .editor(id)
            .ok_or("source document disappeared")?;
        let document = &editor.state().document;
        self.total_bytes = self
            .total_bytes
            .saturating_sub(size)
            .checked_add(document.byte_count())
            .ok_or("replacement source size overflow")?;
        if self.total_bytes > SNAPSHOT_LIMIT {
            return Err("replacement sources grew beyond 32 MiB".into());
        }
        let mut checked_lines = std::collections::HashSet::new();
        let mut edits = Vec::with_capacity(entries.len());
        for entry in entries {
            let source = entry.source();
            if checked_lines.insert(source.line)
                && document.line(source.line).as_deref() != Some(source.line_text.as_ref())
            {
                return Err("source line changed since search".into());
            }
            let offset = document
                .line_to_byte_offset(source.line)
                .ok_or("source line disappeared")?;
            edits.push(CheckedEdit {
                bytes: offset + source.range.start..offset + source.range.end,
                expected: entry.original().to_owned(),
                replacement: entry.text().to_owned(),
            });
        }
        let prepared = Prepared {
            id,
            revision: document.revision(),
            path: hit.path.as_ref().clone(),
            edits,
        };
        verify(app, &prepared)?;
        Ok(prepared)
    }
    fn abort(&mut self, review: &mut Review, reason: &str) {
        for prepared in self.prepared.drain(..) {
            review
                .outcomes
                .push(format!("NOT APPLIED {}: {reason}", prepared.path.display()));
        }
        for entries in self.pending.drain(..) {
            if let Some(entry) = entries.first() {
                review.outcomes.push(format!(
                    "NOT APPLIED {}: {reason}",
                    entry.source().path.display()
                ));
            }
        }
        review.feedback = reason.into();
    }
    fn finish(&self, app: &mut App, review: &mut Review) -> bool {
        review.completed = true;
        review.selected = 0;
        if let Some(tab) = self.original_tab {
            if app.workspace.activate(tab) {
                app.after_tab_change();
            } else {
                review
                    .outcomes
                    .push("Original tab was closed; current tab retained".into());
            }
        }
        true
    }
}
fn verify(app: &App, prepared: &Prepared) -> Result<(), String> {
    let file = app
        .workspace
        .payload(prepared.id)
        .and_then(|session| session.file.as_ref())
        .ok_or("source file disappeared")?;
    if file.path() != prepared.path {
        return Err("source file path changed".into());
    }
    if !file.disk_state().map_err(|e| e.to_string())?.permits_save() {
        return Err(format!("disk changed: {}", prepared.path.display()));
    }
    app.workspace
        .editor(prepared.id)
        .ok_or("source editor disappeared")?
        .validate_checked_edits(prepared.revision, &prepared.edits)
}
fn save(app: &mut App, id: DocumentId) -> Result<(), String> {
    let (editor, session) = app
        .workspace
        .editor_and_payload_mut(id)
        .ok_or("source document disappeared")?;
    let file = session.file.as_mut().ok_or("source file disappeared")?;
    file.save(&editor.content(), false)
        .map_err(|error| error.to_string())
}
