//! A hierarchy edited as indented text.
//!
//! Two halves, and they are the seam "edit it as text" is built on:
//!
//! - **Projection**: [`Outline::project`] writes entries as lines, one indent
//!   per level and a trailing [`BRANCH_MARK`] on an entry that can hold
//!   others.
//! - **Reading**: [`Outline::track`] follows the text through every edit so
//!   each line keeps knowing which entry it was loaded as, and
//!   [`Outline::read`] turns the lines back into entries with parents taken
//!   from the indentation.
//!
//! Nothing here knows what an entry is. The explorer's are files keyed by
//! where they were; another source supplies its own key and its own planner.

mod carry;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::fmt;

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
    /// The text as last tracked, one string per line.
    lines: Vec<String>,
    /// For each tracked line, the entry it still is.
    origins: Vec<Option<usize>>,
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
        // An empty document still has one line, and tracking compares line
        // for line with the editor holding it.
        let (lines, origins) = if projected.is_empty() {
            (vec![String::new()], vec![None])
        } else {
            (projected.clone(), (0..entries.len()).map(Some).collect())
        };
        Ok(Self {
            entries,
            parents,
            projected,
            lines,
            origins,
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

    /// Follows the text to `lines` — the editor's lines, without endings —
    /// and reports whether anything changed.
    ///
    /// Lines the edit did not touch keep their entry; edited ones are paired
    /// with the old lines they replaced; and a line that comes back with an
    /// entry's label after the entry's own line went (a cut and a paste, an
    /// undo) is that entry again rather than a new one.
    pub fn track(&mut self, lines: Vec<String>) -> bool {
        if lines == self.lines {
            return false;
        }
        let origins = carry::carry(&self.lines, &self.origins, &lines);
        self.lines = lines;
        self.origins = origins;
        self.reattach();
        true
    }

    /// The tracked text read back as entries.
    #[must_use]
    pub fn read(&self) -> Reading {
        let lines = self.structure();
        let mut held = vec![false; self.entries.len()];
        for line in &lines {
            if let Some(entry) = line.origin {
                held[entry] = true;
            }
        }
        let removed = (0..self.entries.len())
            .filter(|&entry| !held[entry])
            .collect();
        Reading { lines, removed }
    }

    /// Whether the text says anything other than what was loaded.
    ///
    /// Blank lines and a mark added to a leaf say nothing, so they do not
    /// count; a typed line, a missing entry, a rename or a move does.
    #[must_use]
    pub fn is_edited(&self) -> bool {
        let reading = self.read();
        !reading.removed.is_empty()
            || reading
                .lines
                .iter()
                .any(|line| self.state_of(&reading.lines, line) != LineState::Loaded)
    }

    /// How every tracked line compares with what was loaded, by line.
    #[must_use]
    pub fn line_states(&self) -> Vec<LineState> {
        let reading = self.read();
        let mut states = vec![LineState::Blank; self.lines.len()];
        for line in &reading.lines {
            if let Some(state) = states.get_mut(line.line) {
                *state = self.state_of(&reading.lines, line);
            }
        }
        states
    }

    /// The tracked lines.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// What one read line is, compared with what was loaded.
    fn state_of(&self, lines: &[OutlineLine], line: &OutlineLine) -> LineState {
        let Some(entry) = line.origin else {
            return LineState::Typed;
        };
        let parent_entry = line.parent.and_then(|parent| lines[parent].origin);
        let same_parent = match (line.parent, self.parents[entry]) {
            (None, None) => true,
            (Some(_), Some(loaded)) => parent_entry == Some(loaded),
            _ => false,
        };
        if same_parent && self.entries[entry].label == line.label {
            LineState::Loaded
        } else {
            LineState::Changed
        }
    }

    /// Gives a typed line the entry it reads as, when that entry has no line.
    ///
    /// A label matching an entry whose line is gone is that entry: deleting
    /// and recreating a file under its own name is never what a cut and paste
    /// meant. One under the entry's own parent is preferred to one elsewhere.
    fn reattach(&mut self) {
        // A blank line still remembers the entry it was emptied from, so the
        // next characters typed there rename it; but it says nothing, so the
        // entry is free to be claimed by a line that does.
        let mut held = vec![false; self.entries.len()];
        for (text, origin) in self.lines.iter().zip(&self.origins) {
            if let Some(entry) = origin
                && !text.trim().is_empty()
            {
                held[*entry] = true;
            }
        }
        if held.iter().all(|&held| held) {
            return;
        }
        let mut by_label: HashMap<&str, Vec<usize>> = HashMap::new();
        for (entry, item) in self.entries.iter().enumerate() {
            if !held[entry] {
                by_label.entry(item.label.as_str()).or_default().push(entry);
            }
        }
        let lines = self.structure();
        let mut found: Vec<(usize, usize)> = Vec::new();
        let mut origins_by_read: Vec<Option<usize>> =
            lines.iter().map(|line| line.origin).collect();
        for (index, line) in lines.iter().enumerate() {
            if line.origin.is_some() {
                continue;
            }
            let Some(candidates) = by_label.get(line.label.as_str()) else {
                continue;
            };
            // What a loaded parent would have to be for this line to sit where
            // the entry was; `None` under a typed line, which no entry was in.
            let wanted_parent = line
                .parent
                .map_or(Some(None), |parent| origins_by_read[parent].map(Some));
            let free = || candidates.iter().copied().filter(|&entry| !held[entry]);
            let chosen = free()
                .find(|&entry| wanted_parent == Some(self.parents[entry]))
                .or_else(|| free().next());
            if let Some(entry) = chosen {
                held[entry] = true;
                origins_by_read[index] = Some(entry);
                found.push((line.line, entry));
            }
        }
        if found.is_empty() {
            return;
        }
        let claimed: HashMap<usize, usize> =
            found.iter().map(|&(line, entry)| (entry, line)).collect();
        for (line, origin) in self.origins.iter_mut().enumerate() {
            if let Some(entry) = *origin
                && claimed.get(&entry).is_some_and(|&claimer| claimer != line)
            {
                *origin = None;
            }
        }
        for (line, entry) in found {
            self.origins[line] = Some(entry);
        }
    }

    /// The tracked lines read for structure, without reattaching anything.
    fn structure(&self) -> Vec<OutlineLine> {
        let mut read: Vec<OutlineLine> = Vec::with_capacity(self.lines.len());
        // The lines still open above the one being read: (width, index).
        let mut open: Vec<(usize, usize)> = Vec::new();
        for (number, text) in self.lines.iter().enumerate() {
            if text.trim().is_empty() {
                continue;
            }
            let origin = self.origins.get(number).copied().flatten();
            // An untouched line means what it was loaded as, even when its
            // label would read back differently from the text alone.
            let (width, label, branch) = match origin {
                Some(entry) if self.projected[entry] == *text => {
                    let loaded = &self.entries[entry];
                    (
                        loaded.depth.saturating_mul(INDENT_WIDTH),
                        loaded.label.clone(),
                        loaded.branch,
                    )
                },
                _ => parse_line(text),
            };
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
                origin,
            });
        }
        read
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
