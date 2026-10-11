//! Live replacement preview and explicit review choices.
mod detail;
mod mouse;
pub(super) mod operation;
mod paint;
#[cfg(test)]
mod tests;
use iridium_editor::{KeyCode, KeyEvent};
use iridium_search::{Event, Job, Match, QUERY_LIMIT, Query, Replacement};
use iridium_tui::frame::TextField;
use operation::Operation;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Replacement,
    SelectingReplacement,
    Results,
}
impl Focus {
    const fn is_replacement(self) -> bool {
        matches!(self, Self::Replacement | Self::SelectingReplacement)
    }
}
pub(super) enum Action {
    Continue,
    Apply(bool),
}
pub(super) struct Review {
    pub open: bool,
    pub hits: Vec<Match>,
    pub root: std::path::PathBuf,
    query: Query,
    entry: TextField,
    focus: Focus,
    pub selected: usize,
    pub included: Vec<bool>,
    pub preview: Option<Vec<Replacement>>,
    pub feedback: String,
    pub outcomes: Vec<String>,
    pub operation: Option<Operation>,
    job: Option<Job>,
    due: Option<Instant>,
    stale: bool,
    pub completed: bool,
    issues: usize,
    detail: Option<detail::Detail>,
}
impl Review {
    fn stop_drag(&mut self) {
        if self.focus == Focus::SelectingReplacement {
            self.focus = Focus::Replacement;
        }
    }
    pub fn new(query: Query, hits: Vec<Match>, root: std::path::PathBuf, issues: usize) -> Self {
        let included = vec![true; hits.len()];
        Self {
            open: true,
            query,
            hits,
            root,
            entry: TextField::default(),
            focus: Focus::Replacement,
            selected: 0,
            included,
            preview: None,
            feedback: "Preparing deletion preview…".into(),
            outcomes: Vec::new(),
            operation: None,
            job: None,
            due: Some(Instant::now()),
            stale: false,
            completed: false,
            issues,
            detail: None,
        }
    }
    pub fn close(&mut self) {
        self.stop_drag();
        self.entry.invalidate_cut();
        if let Some(job) = self.job.as_mut() {
            job.cancel();
        }
        self.due = None;
        self.stale = true;
        self.open = false;
    }
    pub const fn waiting(&self) -> bool {
        self.job.is_some() || self.due.is_some() || self.operation.is_some()
    }
    pub fn poll(&mut self) {
        if let Some(job) = self.job.as_mut() {
            let result = job.poll();
            if !job.is_running() {
                self.job = None;
            }
            match result {
                Err(error) => {
                    self.feedback = error;
                    self.job = None;
                },
                Ok(events) if !self.stale => {
                    for event in events {
                        if let Event::Preview(result) = event {
                            match result {
                                Ok(preview) => {
                                    self.preview = Some(preview);
                                    self.feedback = "Review changes before applying".into();
                                },
                                Err(error) => self.feedback = error,
                            }
                        }
                    }
                },
                Ok(_) => {},
            }
        }
        if self.job.is_none() {
            self.stale = false;
            if self.due.is_some_and(|due| Instant::now() >= due) {
                self.due = None;
                match Job::preview(
                    self.query.clone(),
                    self.hits.clone(),
                    self.entry.text().to_owned(),
                ) {
                    Ok(job) => self.job = Some(job),
                    Err(error) => self.feedback = error,
                }
            }
        }
    }
    fn restart(&mut self) {
        if let Some(job) = self.job.as_mut() {
            job.cancel();
        }
        self.preview = None;
        self.stale = true;
        self.feedback = "Updating preview…".into();
        self.due = Some(Instant::now() + Duration::from_millis(150));
    }
    pub fn paste(&mut self, text: &str) {
        self.stop_drag();
        if self.detail.is_some()
            || self.operation.is_some()
            || self.completed
            || !self.focus.is_replacement()
        {
            return;
        }
        if let Err(error) = self.entry.paste(text, QUERY_LIMIT) {
            self.feedback = error;
            return;
        }
        self.restart();
    }
    fn edit_field(&mut self, event: &KeyEvent) {
        let before = self.entry.text().to_owned();
        match self.entry.handle_key(event, QUERY_LIMIT) {
            Err(error) => self.feedback = error,
            Ok(_) if self.entry.text() != before => self.restart(),
            Ok(_) => {},
        }
    }
    pub fn key(&mut self, event: &KeyEvent, width: usize, height: usize) -> Action {
        self.stop_drag();
        if let Some(mut detail) = self.detail.take() {
            if detail.key(event, self, width, height) {
                self.detail = Some(detail);
            }
            return Action::Continue;
        }
        if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.meta {
            if self.focus.is_replacement() && self.operation.is_none() && !self.completed {
                self.edit_field(event);
            }
            return Action::Continue;
        }
        if event.key == KeyCode::Escape {
            if let Some(operation) = self.operation.as_mut() {
                operation.cancel();
            } else {
                self.close();
            }
            return Action::Continue;
        }
        if self.completed {
            let last = self.outcomes.len().saturating_sub(1);
            match event.key {
                KeyCode::Up => self.selected = self.selected.saturating_sub(1),
                KeyCode::Down => self.selected = self.selected.saturating_add(1).min(last),
                KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
                KeyCode::PageDown => self.selected = self.selected.saturating_add(10).min(last),
                _ => {},
            }
            return Action::Continue;
        }
        if self.operation.is_some() {
            return Action::Continue;
        }
        match event.key {
            KeyCode::F4 => {
                if !self.waiting()
                    && self
                        .preview
                        .as_ref()
                        .is_some_and(|p| p.get(self.selected).is_some())
                {
                    self.detail = Some(detail::Detail::default());
                } else {
                    self.feedback = "Wait for a completed match preview before inspecting".into();
                }
            },
            KeyCode::F6 | KeyCode::F9 => {
                if !self.waiting() && self.preview.is_some() {
                    return Action::Apply(event.key == KeyCode::F9);
                }
                self.feedback = "Wait for the latest preview before applying".into();
            },
            KeyCode::Tab => {
                self.focus = if self.focus.is_replacement() {
                    Focus::Results
                } else {
                    Focus::Replacement
                }
            },
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.hits.len().saturating_sub(1));
            },
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.hits.len().saturating_sub(1));
            },
            KeyCode::F7 | KeyCode::Char(' ') if self.focus == Focus::Results => {
                if let Some(included) = self.included.get(self.selected).copied() {
                    if event.modifiers.shift || event.key == KeyCode::F7 {
                        if let Some(hit) = self.hits.get(self.selected) {
                            for (other, checked) in self.hits.iter().zip(&mut self.included) {
                                if hit.path == other.path {
                                    *checked = !included;
                                }
                            }
                        }
                    } else {
                        self.included[self.selected] = !included;
                    }
                }
            },
            _ if self.focus.is_replacement() => self.edit_field(event),
            _ => {},
        }
        Action::Continue
    }
}

impl Review {
    pub(super) fn clipboard_field(&self) -> Option<&TextField> {
        (self.open
            && self.detail.is_none()
            && !self.completed
            && self.operation.is_none()
            && self.focus.is_replacement())
        .then_some(&self.entry)
    }
    pub(super) fn finish_field_cut(
        &mut self,
        cut: &iridium_tui::frame::TextFieldCut,
    ) -> Result<(), String> {
        if self.clipboard_field().is_none() {
            return Err("Text copied; replacement focus changed, so cut was cancelled".into());
        }
        self.entry.finish_cut(cut)?;
        self.restart();
        Ok(())
    }
}
