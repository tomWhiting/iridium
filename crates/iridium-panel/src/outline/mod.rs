//! A hierarchy edited as indented text.
//!
//! Two halves, and they are the seam "edit it as text" is built on:
//!
//! - **Projection**: [`Outline::project`] writes entries as lines, one indent
//!   per level and a trailing [`BRANCH_MARK`] on an entry that can hold
//!   others.
//! - **Reading**: [`Outline::edit`] follows the text through every edit so
//!   each line keeps knowing which entry it was loaded as, and
//!   [`Outline::read`] turns the lines back into entries with parents taken
//!   from the indentation.
//!
//! An edit costs what it touched: only the replaced lines are re-carried, and
//! only lines whose parent the edit could have moved are re-read. How each
//! line compares with what was loaded, how many entries no line holds, and
//! whether anything changed are kept up to date as it goes, so drawing a
//! screenful asks about a screenful.
//!
//! Nothing here knows what an entry is. The explorer's are files keyed by
//! where they were; another source supplies its own key and its own planner.

mod carry;
mod lines;

#[cfg(test)]
mod tests;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;

use lines::{Line, Lines, Parent};

/// One indent level in the projected text.
pub const INDENT: &str = "  ";

/// The columns one [`INDENT`] occupies; a tab advances to the next multiple.
pub const INDENT_WIDTH: usize = 2;

/// The suffix a branch is written with, and read back from.
pub const BRANCH_MARK: char = '/';

/// One entry of a hierarchy, in pre-order, as a source hands it over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineEntry<K> {
    /// How deep the entry sits, zero for the top level.
    pub depth: usize,
    /// The text the entry is written as, without indentation or mark.
    pub label: String,
    /// Whether the entry can hold others, and so is written with the mark.
    pub branch: bool,
    /// What the source knows the entry by.
    pub key: K,
}

/// Why a hierarchy cannot be written as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionError {
    /// A label no single line can hold: empty, or with a control character.
    Unrepresentable {
        /// The label as the source gave it.
        label: String,
    },
}

impl fmt::Display for ProjectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrepresentable { label } => {
                write!(formatter, "{label:?} cannot be written as one line of text")
            },
        }
    }
}

impl std::error::Error for ProjectionError {}

/// One non-blank line of edited text, read back as an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineLine {
    /// Which line of the text it is.
    pub line: usize,
    /// How deep it sits, from the indentation above it.
    pub depth: usize,
    /// The text after the indentation, without one trailing mark.
    pub label: String,
    /// Whether it ends with the mark.
    pub branch: bool,
    /// The line holding it, as an index into [`Reading::lines`].
    pub parent: Option<usize>,
    /// The entry it was loaded as, or `None` for a line somebody typed.
    pub origin: Option<usize>,
}

/// What the edited text now says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    /// Every non-blank line, top to bottom.
    pub lines: Vec<OutlineLine>,
    /// The entries no line holds any more, in loaded order.
    pub removed: Vec<usize>,
}

/// How a line compares with what was loaded, for drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineState {
    /// Nothing on it, so it says nothing.
    Blank,
    /// An entry as it was loaded, under the parent it was loaded under.
    Loaded,
    /// An entry renamed or moved under another parent.
    Changed,
    /// A line no entry was loaded as.
    Typed,
}

/// A hierarchy written as text, and the record of which line is which entry.
#[derive(Debug, Clone)]
pub struct Outline<K> {
    /// The entries as loaded.
    entries: Vec<OutlineEntry<K>>,
    /// Each entry's loaded parent, an index into `entries`.
    parents: Vec<Option<usize>>,
    /// Each entry's projected line.
    projected: Vec<String>,
    /// The text as last followed, and what each line was read as.
    lines: Lines,
    /// Whether a non-blank line holds each entry.
    held: Vec<bool>,
    /// Each entry's current claim; see [`Line::epoch`].
    claims: Vec<u32>,
    /// The entries no non-blank line holds, by label, for a typed line to be
    /// matched against.
    free: HashMap<String, BTreeSet<usize>>,
    /// How many entries no non-blank line holds.
    removed: usize,
    /// How many non-blank lines read as anything but loaded.
    unlike: usize,
    /// How many typed lines carry each label.
    typed: HashMap<String, usize>,
}

impl<K> Outline<K> {
    /// Writes `entries` as text, one line each.
    ///
    /// # Errors
    ///
    /// [`ProjectionError::Unrepresentable`] for a label that cannot be one
    /// line: reading it back would split or lose it, which for a file is a
    /// rename nobody asked for.
    pub fn project(entries: Vec<OutlineEntry<K>>) -> Result<Self, ProjectionError> {
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.label.is_empty() || entry.label.chars().any(char::is_control))
        {
            return Err(ProjectionError::Unrepresentable {
                label: entry.label.clone(),
            });
        }
        let parents = parents_by_depth(entries.iter().map(|entry| entry.depth));
        let projected: Vec<String> = entries.iter().map(projected_line).collect();
        // An empty document still has one line, and following an edit
        // compares line for line with the editor holding it.
        let lines = if projected.is_empty() {
            vec![blank_line(String::new())]
        } else {
            projected
                .iter()
                .zip(&entries)
                .zip(&parents)
                .enumerate()
                .map(|(index, ((text, entry), parent))| Line {
                    text: text.clone(),
                    origin: Some(index),
                    epoch: 0,
                    width: Some(entry.depth.saturating_mul(INDENT_WIDTH)),
                    matches: true,
                    parent: parent.map_or(Parent::Top, Parent::Entry),
                    state: LineState::Loaded,
                })
                .collect()
        };
        let count = entries.len();
        Ok(Self {
            entries,
            parents,
            projected,
            lines: Lines::new(lines),
            held: vec![true; count],
            claims: vec![0; count],
            free: HashMap::new(),
            removed: 0,
            unlike: 0,
            typed: HashMap::new(),
        })
    }

    /// The projected text, lines joined by `\n`.
    #[must_use]
    pub fn text(&self) -> String {
        self.projected.join("\n")
    }

    /// The entries as loaded.
    #[must_use]
    pub fn entries(&self) -> &[OutlineEntry<K>] {
        &self.entries
    }

    /// Where `entry` was loaded, as an index into [`Self::entries`].
    #[must_use]
    pub fn loaded_parent(&self, entry: usize) -> Option<usize> {
        self.parents.get(entry).copied().flatten()
    }

    /// How many lines the text has.
    #[must_use]
    pub const fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Follows the text to `lines` — every line, without endings — and
    /// reports whether anything changed.
    ///
    /// Re-reads the whole text; [`Self::edit`] is the same for a known span.
    pub fn track(&mut self, lines: Vec<String>) -> bool {
        self.edit(0, self.lines.len(), lines)
    }

    /// Follows an edit that replaced lines `start..old_end` with `lines`, and
    /// reports whether anything changed. The range is clamped to the lines
    /// there are.
    ///
    /// Lines the edit did not touch keep their entry; edited ones are paired
    /// with the old lines they replaced; and a line that comes back with an
    /// entry's label after the entry's own line went (a cut and a paste, an
    /// undo) is that entry again rather than a new one, however far apart the
    /// two are.
    pub fn edit(&mut self, start: usize, old_end: usize, lines: Vec<String>) -> bool {
        let old_end = old_end.min(self.lines.len());
        let start = start.min(old_end);
        if self
            .lines
            .range(start, old_end)
            .map(|line| line.text.as_str())
            .eq(lines.iter().map(String::as_str))
        {
            return false;
        }

        let old = self.lines.splice(start, old_end, Vec::new());
        let old_origins: Vec<Option<usize>> = old.iter().map(|line| self.origin_of(line)).collect();
        let origins = {
            let old_text: Vec<&str> = old.iter().map(|line| line.text.as_str()).collect();
            let new_text: Vec<&str> = lines.iter().map(String::as_str).collect();
            carry::carry(&old_text, &old_origins, &new_text)
        };

        // Whether each entry the edit touched was held before it.
        let mut was_held: HashMap<usize, bool> = HashMap::new();
        for (line, origin) in old.iter().zip(&old_origins) {
            if line.width.is_none() {
                continue;
            }
            self.count(line.state, LineState::Blank);
            match origin {
                Some(entry) => {
                    was_held.entry(*entry).or_insert(self.held[*entry]);
                    self.held[*entry] = false;
                },
                None => self.forget_typed(&line.text),
            }
        }
        let new: Vec<Line> = lines
            .into_iter()
            .zip(origins)
            .map(|(text, origin)| self.line(text, origin))
            .collect();
        for line in &new {
            if line.width.is_none() {
                continue;
            }
            match line.origin {
                Some(entry) => {
                    was_held.entry(entry).or_insert(self.held[entry]);
                    self.held[entry] = true;
                },
                None => *self.typed.entry(parse_line(&line.text).1).or_default() += 1,
            }
        }
        let end = start + new.len();
        self.lines.splice(start, start, new);

        let mut freed = Vec::new();
        for (entry, was) in was_held {
            match (was, self.held[entry]) {
                (true, false) => {
                    self.free_entry(entry);
                    freed.push(entry);
                },
                (false, true) => self.take_entry(entry),
                _ => {},
            }
        }

        let mut regions = vec![(start, end)];
        regions.extend(self.reattach(start, end, &freed));
        regions.sort_unstable();
        for (from, to) in regions {
            self.reread(from, to);
        }
        true
    }

    /// The tracked text read back as entries.
    ///
    /// Reads every line, so it is for when every line is wanted: planning.
    #[must_use]
    pub fn read(&self) -> Reading {
        let mut read: Vec<OutlineLine> = Vec::with_capacity(self.lines.len());
        // The lines still open above the one being read: (width, index).
        let mut open: Vec<(usize, usize)> = Vec::new();
        for (number, line) in self.lines.iter().enumerate() {
            let Some(width) = line.width else {
                continue;
            };
            let (_, label, branch) = self.shape(&line.text, line.origin);
            while open.last().is_some_and(|&(above, _)| above >= width) {
                open.pop();
            }
            let parent = open.last().map(|&(_, index)| index);
            let depth = parent.map_or(0, |parent| read[parent].depth + 1);
            open.push((width, read.len()));
            read.push(OutlineLine {
                line: number,
                depth,
                label,
                branch,
                parent,
                origin: line.origin,
            });
        }
        let removed = (0..self.entries.len())
            .filter(|&entry| !self.held[entry])
            .collect();
        Reading {
            lines: read,
            removed,
        }
    }

    /// Whether the text says anything other than what was loaded.
    ///
    /// Blank lines and a mark added to a leaf say nothing, so they do not
    /// count; a typed line, a missing entry, a rename or a move does.
    #[must_use]
    pub const fn is_edited(&self) -> bool {
        self.removed > 0 || self.unlike > 0
    }

    /// How many loaded entries no line holds any more.
    #[must_use]
    pub const fn removed_count(&self) -> usize {
        self.removed
    }

    /// How line `line` compares with what was loaded; blank past the end.
    #[must_use]
    pub fn line_state(&self, line: usize) -> LineState {
        self.lines
            .get(line)
            .map_or(LineState::Blank, |line| line.state)
    }

    /// How every tracked line compares with what was loaded, by line.
    #[must_use]
    pub fn line_states(&self) -> Vec<LineState> {
        self.lines.iter().map(|line| line.state).collect()
    }

    /// The entry `line` still is: its origin, unless a typed line has claimed
    /// that entry since.
    fn origin_of(&self, line: &Line) -> Option<usize> {
        line.origin
            .filter(|&entry| self.claims.get(entry) == Some(&line.epoch))
    }

    /// A line holding `text`, carried as `origin`, not yet read in place.
    fn line(&self, text: String, origin: Option<usize>) -> Line {
        let epoch = origin.map_or(0, |entry| self.claims[entry]);
        if text.trim().is_empty() {
            // A blank line still remembers the entry it was emptied from, so
            // the next characters typed there rename it.
            return Line {
                origin,
                epoch,
                ..blank_line(text)
            };
        }
        let (width, label, _) = self.shape(&text, origin);
        Line {
            matches: origin.is_some_and(|entry| self.entries[entry].label == label),
            epoch,
            text,
            origin,
            width: Some(width),
            parent: Parent::Top,
            state: LineState::Blank,
        }
    }

    /// A non-blank line's width, label and mark.
    ///
    /// An untouched line means what it was loaded as, even when its label
    /// would read back differently from the text alone.
    fn shape(&self, text: &str, origin: Option<usize>) -> (usize, String, bool) {
        match origin {
            Some(entry) if self.projected[entry] == text => {
                let loaded = &self.entries[entry];
                (
                    loaded.depth.saturating_mul(INDENT_WIDTH),
                    loaded.label.clone(),
                    loaded.branch,
                )
            },
            _ => parse_line(text),
        }
    }

    /// What a line with this origin is to the lines under it.
    const fn as_parent(origin: Option<usize>) -> Parent {
        match origin {
            Some(entry) => Parent::Entry(entry),
            None => Parent::Typed,
        }
    }

    /// What `entry` was loaded under.
    fn loaded_under(&self, entry: usize) -> Parent {
        self.parents[entry].map_or(Parent::Top, Parent::Entry)
    }

    /// What a line `width` wide placed at `index` sits under.
    fn parent_at(&self, index: usize, width: usize) -> Parent {
        self.lines
            .last_before(index, width)
            .and_then(|above| self.lines.get(above))
            .map_or(Parent::Top, |above| Self::as_parent(above.origin))
    }

    /// How `line`, under `parent`, compares with what was loaded.
    fn state_of(&self, line: &Line, parent: Parent) -> LineState {
        if line.width.is_none() {
            return LineState::Blank;
        }
        match line.origin {
            None => LineState::Typed,
            Some(entry) if line.matches && parent == self.loaded_under(entry) => LineState::Loaded,
            Some(_) => LineState::Changed,
        }
    }

    /// Keeps the count of unlike lines in step with a line going from `was`
    /// to `now`.
    const fn count(&mut self, was: LineState, now: LineState) {
        if matches!(was, LineState::Changed | LineState::Typed) {
            self.unlike -= 1;
        }
        if matches!(now, LineState::Changed | LineState::Typed) {
            self.unlike += 1;
        }
    }

    /// Notes that `entry` has no non-blank line now.
    fn free_entry(&mut self, entry: usize) {
        self.removed += 1;
        self.free
            .entry(self.entries[entry].label.clone())
            .or_default()
            .insert(entry);
    }

    /// Notes that `entry` has a non-blank line again.
    fn take_entry(&mut self, entry: usize) {
        self.removed -= 1;
        let label = &self.entries[entry].label;
        if let Some(set) = self.free.get_mut(label) {
            set.remove(&entry);
            if set.is_empty() {
                self.free.remove(label);
            }
        }
    }

    /// Notes that a typed line holding `text` has gone.
    fn forget_typed(&mut self, text: &str) {
        let label = parse_line(text).1;
        if let Some(count) = self.typed.get_mut(&label) {
            *count -= 1;
            if *count == 0 {
                self.typed.remove(&label);
            }
        }
    }

    /// Gives a typed line the entry it reads as, when that entry has no line,
    /// and returns the lines outside `start..end` that this changed.
    ///
    /// A label matching an entry whose line is gone is that entry: deleting
    /// and recreating a file under its own name is never what a cut and paste
    /// meant. One under the entry's own parent is preferred to one elsewhere,
    /// and typed lines are matched top to bottom.
    ///
    /// Only two kinds of line can match after an edit: one the edit typed,
    /// and one typed earlier carrying the label of an entry the edit freed.
    /// Every other typed line was matched, or found nothing to match, when it
    /// was typed.
    fn reattach(&mut self, start: usize, end: usize, freed: &[usize]) -> Vec<(usize, usize)> {
        if self.free.is_empty() {
            return Vec::new();
        }
        let mut candidates: Vec<usize> = (start..end)
            .zip(self.lines.range(start, end))
            .filter(|(_, line)| line.is_typed())
            .map(|(index, _)| index)
            .collect();
        let freed_labels: HashSet<&str> = freed
            .iter()
            .map(|&entry| self.entries[entry].label.as_str())
            .filter(|label| self.typed.contains_key(*label))
            .collect();
        if !freed_labels.is_empty() {
            candidates.extend(
                self.lines
                    .typed()
                    .filter(|&(index, line)| {
                        !(start..end).contains(&index)
                            && freed_labels.contains(parse_line(&line.text).1.as_str())
                    })
                    .map(|(index, _)| index),
            );
            candidates.sort_unstable();
        }

        let mut outside = Vec::new();
        for index in candidates {
            let Some((text, width)) = self
                .lines
                .get(index)
                .and_then(|line| Some((line.text.clone(), line.width?)))
            else {
                continue;
            };
            let label = parse_line(&text).1;
            let wanted = self.parent_at(index, width);
            let Some(entry) = self.free.get(&label).and_then(|set| {
                set.iter()
                    .copied()
                    .find(|&entry| self.loaded_under(entry) == wanted)
                    .or_else(|| set.first().copied())
            }) else {
                continue;
            };
            self.take_entry(entry);
            self.held[entry] = true;
            self.claims[entry] = self.claims[entry].wrapping_add(1);
            self.forget_typed(&text);
            let line = self.line(text, Some(entry));
            // The new line is unread; reading its region counts it again.
            if let Some(old) = self.lines.replace(index, line) {
                self.count(old.state, LineState::Blank);
            }
            if !(start..end).contains(&index) {
                outside.push((index, index + 1));
            }
        }
        outside
    }

    /// Works out again what lines `start..end` sit under and how they
    /// compare, then does the same for the lines below whose parent that
    /// could have moved.
    fn reread(&mut self, start: usize, end: usize) {
        // The lines still open above `start`, as (width, what the line is to
        // one under it): the nearest line above, then the nearest narrower
        // line above that, and so on out to the top level.
        let mut open: Vec<(usize, Parent)> = Vec::new();
        let (mut at, mut narrower) = (start, usize::MAX);
        while let Some(above) = self.lines.last_before(at, narrower) {
            let Some(line) = self.lines.get(above) else {
                break;
            };
            let width = line.width.unwrap_or(0);
            open.push((width, Self::as_parent(line.origin)));
            if width == 0 {
                break;
            }
            (at, narrower) = (above, width);
        }
        open.reverse();

        let mut readings = Vec::with_capacity(end.saturating_sub(start));
        for line in self.lines.range(start, end) {
            let Some(width) = line.width else {
                readings.push((line.parent, LineState::Blank, line.state));
                continue;
            };
            while open.last().is_some_and(|&(above, _)| above >= width) {
                open.pop();
            }
            let parent = open.last().map_or(Parent::Top, |&(_, parent)| parent);
            readings.push((parent, self.state_of(line, parent), line.state));
            open.push((width, Self::as_parent(line.origin)));
        }
        for (index, (parent, state, was)) in (start..end).zip(readings) {
            self.count(was, state);
            self.lines.set_reading(index, parent, state);
        }
        self.follow(end);
    }

    /// Re-reads the lines from `below` on whose search for a parent passes
    /// above `below`: the first non-blank line and every line as wide that
    /// the same search reaches, then each narrower one after them likewise.
    ///
    /// Every line of one width whose search passes above `below` has the same
    /// parent, before the edit and after, so one comparison settles the whole
    /// width. That is what keeps a rename inside a folder of thousands from
    /// visiting its siblings.
    fn follow(&mut self, below: usize) {
        let (mut from, mut widest) = (below, usize::MAX);
        while let Some(first) = self.lines.first_from(from, widest) {
            let Some(width) = self.lines.get(first).and_then(|line| line.width) else {
                break;
            };
            // A top-level line sits under nothing, whatever is above it.
            if width == 0 {
                break;
            }
            let parent = self.parent_at(below, width);
            let mut last = first;
            if self
                .lines
                .get(first)
                .is_some_and(|line| line.parent != parent)
            {
                let mut index = first;
                loop {
                    if let Some(line) = self.lines.get(index) {
                        let (was, state) = (line.state, self.state_of(line, parent));
                        self.count(was, state);
                        self.lines.set_reading(index, parent, state);
                    }
                    last = index;
                    match self.lines.first_from(index + 1, width) {
                        Some(next)
                            if self.lines.get(next).and_then(|line| line.width) == Some(width) =>
                        {
                            index = next;
                        },
                        _ => break,
                    }
                }
            }
            (from, widest) = (last + 1, width - 1);
        }
    }
}

/// A blank line holding `text`, with no entry.
const fn blank_line(text: String) -> Line {
    Line {
        text,
        origin: None,
        epoch: 0,
        width: None,
        matches: false,
        parent: Parent::Top,
        state: LineState::Blank,
    }
}

/// The line an entry is written as.
fn projected_line<K>(entry: &OutlineEntry<K>) -> String {
    let mut line = INDENT.repeat(entry.depth);
    line.push_str(&entry.label);
    if entry.branch {
        line.push(BRANCH_MARK);
    }
    line
}

/// A line's indentation width, label and mark, read from its text alone.
fn parse_line(text: &str) -> (usize, String, bool) {
    let mut width = 0;
    let mut body = text;
    for (offset, character) in text.char_indices() {
        match character {
            ' ' => width += 1,
            '\t' => width = (width / INDENT_WIDTH + 1) * INDENT_WIDTH,
            _ => {
                body = &text[offset..];
                break;
            },
        }
    }
    body.strip_suffix(BRANCH_MARK).map_or_else(
        || (width, body.to_owned(), false),
        |label| (width, label.to_owned(), true),
    )
}

/// Each item's parent from depths alone: the nearest earlier item that is
/// shallower, so a jump in depth attaches where the eye would put it.
fn parents_by_depth(depths: impl Iterator<Item = usize>) -> Vec<Option<usize>> {
    let mut open: Vec<(usize, usize)> = Vec::new();
    let mut parents = Vec::new();
    for (index, depth) in depths.enumerate() {
        while open.last().is_some_and(|&(above, _)| above >= depth) {
            open.pop();
        }
        parents.push(open.last().map(|&(_, above)| above));
        open.push((depth, index));
    }
    parents
}
