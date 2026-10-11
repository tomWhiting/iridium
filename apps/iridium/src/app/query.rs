//! A live source selection preview, isolated from edits until accepted.
use super::prompt::{Answer, Message, Prompt};
use super::{App, Flow};
use crate::query_worker::{Job, Snapshot};
use iridium_editor::render::Viewport;
use iridium_editor::workspace::NodeId;
use iridium_editor::{CursorState, Editor, KeyEvent, Selection};
use iridium_query::{Format, Query, QueryResult, SourceRange, Target};
use std::time::{Duration, Instant};

pub(super) struct QuerySession {
    tab: NodeId,
    revision: u64,
    original: CursorState,
    viewport: Viewport,
    snapshot: Snapshot,
    text: String,
    format: Format,
    due: Option<Instant>,
    job: Option<Job>,
    count: Option<usize>,
    accept: bool,
}

impl QuerySession {
    fn cancel(&mut self) -> Result<(), String> {
        if let Some(mut job) = self.job.take() {
            job.cancel()?;
        }
        Ok(())
    }

    fn restore(&self, editor: &mut Editor) {
        editor.set_selections(self.original.clone());
        editor.set_viewport(self.viewport.clone());
    }

    fn poll(&mut self) -> Option<QueryResult> {
        if self.due.is_some_and(|due| Instant::now() >= due) {
            self.due = None;
            let query = parse(&self.text, self.format);
            match Job::spawn(&self.snapshot, &query) {
                Ok(job) => self.job = Some(job),
                Err(error) => return Some(Err(error)),
            }
        }
        let result = self.job.as_mut()?.poll();
        if result.is_some() {
            self.job = None;
        }
        result
    }
}

impl App {
    pub(super) fn open_query(&mut self, text: &str) -> Flow {
        let Some(editor) = self.editor() else {
            return Flow::Running;
        };
        let Some(tab) = self.workspace.active() else {
            return Flow::Running;
        };
        let snapshot = match Snapshot::new(&editor.content()) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                self.message = Some(Message::error(error));
                return Flow::Running;
            },
        };
        let format = if self
            .file()
            .and_then(|file| file.path().extension())
            .is_some_and(|ext| ext == "jsonl" || ext == "ndjson")
        {
            Format::JsonLines
        } else {
            Format::Auto
        };
        self.query = Some(QuerySession {
            tab,
            revision: editor.state().document.revision(),
            original: editor.state().cursor.clone(),
            viewport: editor.state().viewport.clone(),
            snapshot,
            text: String::new(),
            format,
            due: None,
            job: None,
            count: None,
            accept: false,
        });
        self.palette_open = false;
        self.prompt = Some(Prompt::query(text));
        self.refresh_query();
        Flow::Running
    }

    pub(super) fn query_waiting(&self) -> bool {
        self.query
            .as_ref()
            .is_some_and(|query| query.due.is_some() || query.job.is_some())
    }

    pub(super) fn refresh_query(&mut self) {
        let text = self
            .prompt
            .as_ref()
            .and_then(Prompt::query_text)
            .unwrap_or("")
            .to_owned();
        let Some(query) = self.query.as_mut() else {
            return;
        };
        if text == query.text {
            return;
        }
        query.text = text;
        query.count = None;
        query.accept = false;
        query.due = None;
        let cancelled = query.cancel();
        if let Some(editor) = self.workspace.active_editor_mut() {
            query.restore(editor);
        }
        let feedback = match cancelled {
            Err(error) => error,
            Ok(()) if query.text.trim().is_empty() => {
                "type a filter · Enter keep · Esc cancel".into()
            },
            Ok(()) => {
                query.due = Some(Instant::now() + Duration::from_millis(150));
                "searching…".into()
            },
        };
        self.query_feedback(&feedback);
    }

    fn query_feedback(&mut self, feedback: &str) {
        if let Some(prompt) = self.prompt.as_mut() {
            prompt.set_query_feedback(feedback);
        }
    }

    pub(super) fn poll_query(&mut self) {
        let Some(mut query) = self.query.take() else {
            return;
        };
        let valid = self.workspace.active() == Some(query.tab)
            && self
                .editor()
                .is_some_and(|editor| editor.state().document.revision() == query.revision);
        if !valid {
            let cleanup = query.cancel();
            self.prompt = None;
            self.message = Some(Message::error(
                cleanup
                    .err()
                    .unwrap_or_else(|| "jq cancelled: document changed".into()),
            ));
            return;
        }
        if let Some(result) = query.poll() {
            let outcome = self
                .workspace
                .active_editor_mut()
                .ok_or_else(|| "jq document is no longer open".to_owned())
                .and_then(|editor| result.and_then(|ranges| preview(editor, &query, ranges)));
            match outcome {
                Ok(count) => {
                    query.count = Some(count);
                    self.query_feedback(&format!("{count} matches · Enter keep · Esc cancel"));
                    self.ensure_caret_visible();
                },
                Err(error) => {
                    query.accept = false;
                    if let Some(editor) = self.workspace.active_editor_mut() {
                        query.restore(editor);
                    }
                    self.query_feedback(&error);
                },
            }
        }
        let accepted = query.accept && query.count.is_some_and(|count| count > 0);
        let count = query.count;
        if accepted {
            self.select_mode();
            self.prompt = None;
            self.message = Some(Message::notice(format!(
                "{} jq matches selected",
                count.unwrap_or(0)
            )));
        } else {
            self.query = Some(query);
        }
    }

    pub(super) fn answer_query(&mut self, event: &KeyEvent) -> Flow {
        let answer = self.prompt.as_mut().map(|prompt| prompt.answer(event));
        match answer {
            Some(Answer::Cancelled) => {
                if let Some(mut query) = self.query.take() {
                    if self.workspace.active() == Some(query.tab)
                        && let Some(editor) = self.workspace.active_editor_mut()
                        && editor.state().document.revision() == query.revision
                    {
                        query.restore(editor);
                    }
                    if let Err(error) = query.cancel() {
                        self.message = Some(Message::error(error));
                    }
                }
                self.prompt = None;
            },
            Some(Answer::Query) => {
                if let Some(query) = self.query.as_mut() {
                    query.accept = true;
                }
                self.poll_query();
            },
            _ => self.refresh_query(),
        }
        Flow::Running
    }
}

fn preview(
    editor: &mut Editor,
    query: &QuerySession,
    ranges: Vec<SourceRange>,
) -> Result<usize, String> {
    let count = ranges.len();
    let document = &editor.state().document;
    let mut selections = Vec::with_capacity(count);
    for range in ranges {
        let start = document
            .offset_to_position(range.start)
            .ok_or("jq returned an invalid source start")?;
        let end = document
            .offset_to_position(range.end)
            .ok_or("jq returned an invalid source end")?;
        if document.position_to_offset(start) != Some(range.start)
            || document.position_to_offset(end) != Some(range.end)
        {
            return Err("jq returned an offset inside a source character".into());
        }
        selections.push(Selection::new(start, end));
    }
    let mut iter = selections.into_iter();
    if let Some(primary) = iter.next() {
        editor.set_selections(CursorState::from_selections(primary, iter.collect()));
    } else {
        query.restore(editor);
    }
    Ok(count)
}

fn parse(text: &str, mut format: Format) -> Query {
    let mut target = Target::Values;
    let mut filter = text.trim_start();
    loop {
        let (word, tail) = filter
            .split_once(char::is_whitespace)
            .unwrap_or((filter, ""));
        match word {
            "--keys" => target = Target::Keys,
            "--values" => target = Target::Values,
            "--jsonl" => format = Format::JsonLines,
            "--json" => format = Format::Json,
            _ => break,
        }
        filter = tail.trim_start();
    }
    Query {
        filter: filter.into(),
        format,
        target,
    }
}

pub(super) fn command_filter(text: &str) -> Option<&str> {
    let tail = text.trim_start().strip_prefix("jq")?;
    (tail.is_empty() || tail.starts_with(char::is_whitespace)).then(|| tail.trim_start())
}

#[cfg(test)]
mod tests;
