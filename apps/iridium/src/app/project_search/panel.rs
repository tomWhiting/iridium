//! Terminal project results and query fields, retaining only bounded worker data.
use super::{Duration, Instant};
use iridium_editor::{KeyCode, KeyEvent};
use iridium_search::{Event, Job, Match, QUERY_LIMIT, Query};
use iridium_tui::frame::TextField;
use iridium_tui::{cell::CellBuffer, driver::CursorState, frame::Palette};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Pattern,
    Globs,
}

pub(in crate::app) struct Panel {
    pub open: bool,
    pub(super) review: Option<super::review::Review>,
    pub root: std::path::PathBuf,
    pub query: Query,
    pub(super) entry: TextField,
    pub(super) globs: TextField,
    pub(super) field: Field,
    pub(super) mouse_drag: Option<Field>,
    pub(super) errors: bool,
    pub matches: Vec<Match>,
    pub selected: usize,
    pub feedback: String,
    pub(super) clipboard_message: Option<String>,
    warnings: Vec<String>,
    warning_count: usize,
    pub due: Option<Instant>,
    pub job: Option<Job>,
    stale: bool,
    pub(super) clipboard_keys: super::clipboard::Keys,
}
impl Panel {
    pub fn new() -> Self {
        Self {
            open: true,
            review: None,
            root: std::path::PathBuf::new(),
            query: Query::default(),
            entry: TextField::default(),
            globs: TextField::default(),
            field: Field::Pattern,
            mouse_drag: None,
            errors: false,
            matches: Vec::new(),
            selected: 0,
            feedback: "Type a project search".into(),
            clipboard_message: None,
            warnings: Vec::new(),
            warning_count: 0,
            due: None,
            job: None,
            stale: false,
            clipboard_keys: super::clipboard::Keys::default(),
        }
    }
    pub fn set_text(&mut self, text: &str) {
        if let Err(error) = self.entry.set_text(text, QUERY_LIMIT) {
            self.feedback = error;
        }
    }
    pub fn open_review(&mut self) {
        if self.waiting() {
            self.feedback = "Wait for search to finish before reviewing replacements".into();
            return;
        }
        if self.matches.is_empty() {
            self.feedback = "No matches to replace".into();
            return;
        }
        self.review = Some(super::review::Review::new(
            self.query.clone(),
            self.matches.clone(),
            self.root.clone(),
            self.warning_count,
        ));
    }
    pub fn restart(&mut self) {
        if let Some(job) = self.job.as_mut() {
            job.cancel();
        }
        self.stale = true;
        self.query.text = self.entry.text().to_owned();
        self.query.globs = self
            .globs
            .text()
            .split(';')
            .map(str::trim)
            .filter(|g| !g.is_empty())
            .map(str::to_owned)
            .collect();
        self.matches.clear();
        self.warnings.clear();
        self.warning_count = 0;
        self.selected = 0;
        self.feedback = "Waiting…".into();
        self.due = Some(Instant::now() + Duration::from_millis(150));
    }
    pub fn close(&mut self) {
        self.open = false;
        self.mouse_drag = None;
        self.entry.invalidate_cut();
        self.globs.invalidate_cut();
        self.clipboard_keys.clear();
        self.due = None;
        self.stale = true;
        if let Some(job) = self.job.as_mut() {
            job.cancel();
        }
    }
    pub fn waiting(&self) -> bool {
        self.due.is_some()
            || self.job.is_some()
            || self
                .review
                .as_ref()
                .is_some_and(super::review::Review::waiting)
    }
    pub fn poll(&mut self) {
        if let Some(review) = self.review.as_mut() {
            review.poll();
        }
        if self
            .review
            .as_ref()
            .is_some_and(|r| !r.open && !r.waiting())
        {
            self.review = None;
        }
        let Some(job) = self.job.as_mut() else {
            self.stale = false;
            return;
        };
        let events = job.poll();
        if !job.is_running() {
            self.job = None;
        }
        if self.stale {
            if self.job.is_none() {
                self.stale = false;
            }
            return;
        }
        match events {
            Err(error) => {
                self.feedback = error;
                self.job = None;
            },
            Ok(events) => {
                for event in events {
                    match event {
                        Event::Match(hit) => self.matches.push(hit),
                        Event::Invalidated { path, reason } => {
                            self.matches.retain(|hit| hit.path.as_ref() != &path);
                            self.warn(format!("{}: {reason}", path.display()));
                        },
                        Event::Warning(warning) => self.warn(warning),
                        Event::Preview(_) => {
                            self.warn("Unexpected replacement result on search channel".into());
                        },
                        Event::Finished => self.feedback = "Search finished".into(),
                    }
                }
            },
        }
        self.selected = self.selected.min(self.matches.len().saturating_sub(1));
    }
    fn warn(&mut self, warning: String) {
        self.warning_count += 1;
        if self.warnings.len() < 100 {
            self.warnings.push(warning);
        }
    }
    pub fn paste(&mut self, text: &str) {
        self.mouse_drag = None;
        self.clipboard_message = None;
        if let Some(review) = self.review.as_mut() {
            review.paste(text);
            return;
        }
        let entry = if self.field == Field::Globs {
            &mut self.globs
        } else {
            &mut self.entry
        };
        if let Err(error) = entry.paste(text, QUERY_LIMIT) {
            self.feedback = error;
            return;
        }
        self.restart();
    }
    pub fn key(&mut self, event: &KeyEvent) {
        self.mouse_drag = None;
        if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.meta {
            self.edit_field(event);
            return;
        }
        match event.key {
            KeyCode::Escape => self.close(),
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.matches.len().saturating_sub(1));
            },
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.matches.len().saturating_sub(1));
            },
            KeyCode::Tab => {
                self.field = if self.field == Field::Pattern {
                    Field::Globs
                } else {
                    Field::Pattern
                }
            },
            KeyCode::F2 => {
                self.query.regex = !self.query.regex;
                self.restart();
            },
            KeyCode::F3 => {
                self.query.ignore_case = !self.query.ignore_case;
                self.restart();
            },
            KeyCode::F4 => {
                self.query.whole_word = !self.query.whole_word;
                self.restart();
            },
            KeyCode::F8 => self.errors = !self.errors,
            _ => self.edit_field(event),
        }
    }
    fn edit_field(&mut self, event: &KeyEvent) {
        let entry = if self.field == Field::Globs {
            &mut self.globs
        } else {
            &mut self.entry
        };
        let before = entry.text().to_owned();
        match entry.handle_key(event, QUERY_LIMIT) {
            Err(error) => self.feedback = error,
            Ok(_) if entry.text() != before => self.restart(),
            Ok(_) => {},
        }
    }
    pub fn paint(&self, buffer: &mut CellBuffer, palette: &Palette) -> CursorState {
        let cursor = self.paint_content(buffer, palette);
        if let Some(message) = &self.clipboard_message {
            if let Some(row) = buffer.height().checked_sub(1) {
                buffer.set_str(0, row, &" ".repeat(buffer.width()), palette.status());
                buffer.set_str(0, row, message, palette.status());
            }
        }
        cursor
    }
    fn paint_content(&self, buffer: &mut CellBuffer, palette: &Palette) -> CursorState {
        if let Some(review) = self.review.as_ref() {
            return review.paint(buffer, palette);
        }
        let (width, height) = (buffer.width(), buffer.height());
        if height == 0 || width == 0 {
            return CursorState::Hidden;
        }
        for row in 0..height {
            buffer.set_str(0, row, &" ".repeat(width), palette.overlay());
        }
        let title = format!(
            "Project grep · {} matches · {} issues · {}",
            self.matches.len(),
            self.warning_count,
            self.feedback
        );
        buffer.set_str(0, 0, &clean(&title), palette.status());
        let find_cursor =
            self.entry
                .paint(buffer, 1, "Find: ", self.field == Field::Pattern, palette);
        let glob_cursor =
            self.globs
                .paint(buffer, 2, "Globs: ", self.field == Field::Globs, palette);
        let count = height.saturating_sub(5);
        if self.errors {
            for (row, warning) in self.warnings.iter().take(count).enumerate() {
                buffer.set_str(0, row + 3, &clean(warning), palette.overlay_error());
            }
        } else {
            let start = self.selected.saturating_sub(count.saturating_sub(1));
            for (index, hit) in self.matches.iter().enumerate().skip(start).take(count) {
                let origin = if matches!(hit.origin, iridium_search::Origin::Disk(_)) {
                    "disk"
                } else {
                    "buffer"
                };
                let prefix = &hit.line_text[..hit.range.start];
                let column = hit.column;
                let context_start = prefix
                    .char_indices()
                    .rev()
                    .nth(23)
                    .map_or(0, |(offset, _)| offset);
                let snippet: String = hit.line_text[context_start..].chars().take(140).collect();
                let text = format!(
                    "{} {}:{}:{} [{origin}] {}",
                    if index == self.selected { "›" } else { " " },
                    hit.path
                        .strip_prefix(&self.root)
                        .unwrap_or(&hit.path)
                        .display(),
                    hit.line + 1,
                    column + 1,
                    clean(&snippet)
                );
                buffer.set_str(
                    0,
                    index - start + 3,
                    &clean(&text),
                    if index == self.selected {
                        palette.status()
                    } else {
                        palette.text()
                    },
                );
            }
        }
        if height >= 2 {
            buffer.set_str(0, height - 2, &format!("F2 regex:{} F3 ignore-case:{} F4 word:{} · Tab field · globs separated by ; (! excludes)", self.query.regex, self.query.ignore_case, self.query.whole_word), palette.overlay_quiet());
            buffer.set_str(0, height - 1, "↑↓ choose · Enter open · Alt+Enter select file matches · F5 replace · Esc close · F8 issues", palette.status());
        }
        find_cursor
            .filter(|_| height > 3)
            .map(|column| CursorState::At { row: 1, column })
            .or_else(|| {
                glob_cursor
                    .filter(|_| height > 4)
                    .map(|column| CursorState::At { row: 2, column })
            })
            .unwrap_or(CursorState::Hidden)
    }
}
fn clean(text: &str) -> String {
    text.chars()
        .map(|ch| if ch.is_control() { '�' } else { ch })
        .collect()
}

impl Panel {
    pub(super) fn clipboard_field(&self) -> Option<&TextField> {
        if !self.open {
            return None;
        }
        if let Some(review) = &self.review {
            return review.clipboard_field();
        }
        Some(if self.field == Field::Pattern {
            &self.entry
        } else {
            &self.globs
        })
    }
    pub(in crate::app) fn finish_field_cut(
        &mut self,
        cut: &iridium_tui::frame::TextFieldCut,
    ) -> Result<(), String> {
        if !self.open {
            return Err("Text copied; project panel closed, so cut was cancelled".into());
        }
        if let Some(review) = &mut self.review {
            return review.finish_field_cut(cut);
        }
        let field = if self.field == Field::Pattern {
            &mut self.entry
        } else {
            &mut self.globs
        };
        field.finish_cut(cut)?;
        self.restart();
        Ok(())
    }
    pub(super) fn clipboard_notice(&mut self, text: String) {
        self.clipboard_message = Some(text.clone());
        if let Some(review) = &mut self.review {
            review.feedback = text;
        } else {
            self.feedback = text;
        }
    }
}
