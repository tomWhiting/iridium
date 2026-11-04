//! The tracked lines, stored in chunks so that an edit, and the questions
//! the structure asks after one, cost what the edit touched rather than what
//! the outline holds.
//!
//! Each chunk keeps the narrowest indentation in it and how many typed lines
//! it has, so a search for "the nearest line above narrower than this" or
//! "every typed line" passes over whole chunks that cannot answer it.

use super::LineState;

/// The lines a chunk is cut to. A splice moves at most a few chunks' worth,
/// and a search walks at most `len / CHUNK` chunk summaries.
const CHUNK: usize = 256;

/// What a line sits under, as far as comparing with what was loaded goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Parent {
    /// Nothing: it is at the top level.
    Top,
    /// The line holding loaded entry `.0`.
    Entry(usize),
    /// A line no entry was loaded as.
    Typed,
}

/// One tracked line and what it was last read as.
#[derive(Debug, Clone)]
pub(super) struct Line {
    /// The text, without its ending.
    pub(super) text: String,
    /// The entry it was loaded as, while [`Self::epoch`] still says so.
    pub(super) origin: Option<usize>,
    /// Which claim on [`Self::origin`] this line holds. An entry claimed by
    /// a typed line elsewhere moves on, which retires a blank line that
    /// still remembered it without having to find that line.
    pub(super) epoch: u32,
    /// The indentation's width, or `None` for a blank line.
    pub(super) width: Option<usize>,
    /// Whether it reads as its entry's own label.
    pub(super) matches: bool,
    /// What it sits under, as last worked out.
    pub(super) parent: Parent,
    /// How it compares with what was loaded, as last worked out.
    pub(super) state: LineState,
}

impl Line {
    /// Whether this is a non-blank line no entry was loaded as.
    pub(super) const fn is_typed(&self) -> bool {
        self.width.is_some() && self.origin.is_none()
    }
}

/// A run of consecutive lines and a summary of them.
#[derive(Debug, Clone)]
struct Chunk {
    /// The lines, in order.
    lines: Vec<Line>,
    /// The narrowest indentation of a non-blank line here, `usize::MAX` for
    /// none.
    narrowest: usize,
    /// How many of the lines are typed.
    typed: usize,
}

impl Chunk {
    /// A chunk of `lines`, summarised.
    fn new(lines: Vec<Line>) -> Self {
        let mut chunk = Self {
            lines,
            narrowest: usize::MAX,
            typed: 0,
        };
        chunk.summarise();
        chunk
    }

    /// Recomputes the summary after the lines changed.
    fn summarise(&mut self) {
        self.narrowest = self
            .lines
            .iter()
            .filter_map(|line| line.width)
            .min()
            .unwrap_or(usize::MAX);
        self.typed = self.lines.iter().filter(|line| line.is_typed()).count();
    }
}

/// Every tracked line, in chunks.
#[derive(Debug, Clone)]
pub(super) struct Lines {
    /// Never empty: an outline with no lines keeps one empty chunk.
    chunks: Vec<Chunk>,
    /// The index of each chunk's first line, so a line is found by halving.
    starts: Vec<usize>,
    /// The lines in all chunks.
    len: usize,
}

impl Lines {
    /// `lines`, chunked.
    pub(super) fn new(lines: Vec<Line>) -> Self {
        let len = lines.len();
        let mut lines = Self {
            chunks: cut(lines),
            starts: Vec::new(),
            len,
        };
        lines.recount();
        lines
    }

    /// Rebuilds [`Self::starts`] after chunks changed size: one pass over the
    /// chunk summaries, never over the lines.
    fn recount(&mut self) {
        self.starts.clear();
        let mut start = 0;
        for chunk in &self.chunks {
            self.starts.push(start);
            start += chunk.lines.len();
        }
    }

    /// How many lines there are.
    pub(super) const fn len(&self) -> usize {
        self.len
    }

    /// The chunk holding line `index`, its offset there, and the index of
    /// the chunk's first line. `index == len` lands past the last line.
    fn locate(&self, index: usize) -> (usize, usize, usize) {
        let last = self.chunks.len() - 1;
        if index >= self.len {
            let base = self.starts[last];
            return (last, self.chunks[last].lines.len(), base);
        }
        // The last chunk starting at or before `index`; chunks are never
        // empty unless there is only one, so that chunk holds it.
        let at = self.starts.partition_point(|&start| start <= index) - 1;
        (at, index - self.starts[at], self.starts[at])
    }

    /// Line `index`.
    pub(super) fn get(&self, index: usize) -> Option<&Line> {
        if index >= self.len {
            return None;
        }
        let (chunk, offset, _) = self.locate(index);
        self.chunks[chunk].lines.get(offset)
    }

    /// Records what line `index` sits under and how it compares. Neither is
    /// summarised, so no chunk needs remeasuring.
    pub(super) fn set_reading(&mut self, index: usize, parent: Parent, state: LineState) {
        if index >= self.len {
            return;
        }
        let (chunk, offset, _) = self.locate(index);
        let line = &mut self.chunks[chunk].lines[offset];
        line.parent = parent;
        line.state = state;
    }

    /// Puts `line` in place of line `index`, returning the one it replaced.
    pub(super) fn replace(&mut self, index: usize, line: Line) -> Option<Line> {
        if index >= self.len {
            return None;
        }
        let (chunk, offset, _) = self.locate(index);
        let old = std::mem::replace(&mut self.chunks[chunk].lines[offset], line);
        self.chunks[chunk].summarise();
        Some(old)
    }

    /// Replaces lines `start..end` with `new`, returning the ones removed.
    /// The range is clamped to the lines there are.
    pub(super) fn splice(&mut self, start: usize, end: usize, new: Vec<Line>) -> Vec<Line> {
        let end = end.min(self.len);
        let start = start.min(end);
        let (first, from, _) = self.locate(start);
        let (last, to, _) = self.locate(end);
        self.len = self.len - (end - start) + new.len();
        let removed = if first == last {
            self.chunks[first].lines.splice(from..to, new).collect()
        } else {
            let mut removed = self.chunks[first].lines.split_off(from);
            for chunk in self.chunks.drain(first + 1..last) {
                removed.extend(chunk.lines);
            }
            // `last` moved down to `first + 1` with the drain.
            let tail = self.chunks[first + 1].lines.split_off(to);
            removed.append(&mut self.chunks[first + 1].lines);
            self.chunks.remove(first + 1);
            let lines = &mut self.chunks[first].lines;
            lines.extend(new);
            lines.extend(tail);
            removed
        };
        self.settle(first);
        self.recount();
        removed
    }

    /// Keeps chunk `at` near [`CHUNK`] lines after a splice: a large one is
    /// cut, a small one joins the next, an empty one goes.
    fn settle(&mut self, at: usize) {
        let size = self.chunks[at].lines.len();
        if size == 0 && self.chunks.len() > 1 {
            self.chunks.remove(at);
            return;
        }
        if size < CHUNK / 4 && at + 1 < self.chunks.len() {
            let next = self.chunks.remove(at + 1);
            self.chunks[at].lines.extend(next.lines);
        }
        if self.chunks[at].lines.len() > 2 * CHUNK {
            let lines = std::mem::take(&mut self.chunks[at].lines);
            self.chunks.splice(at..=at, cut(lines));
        } else {
            self.chunks[at].summarise();
        }
    }

    /// Every line, top to bottom.
    pub(super) fn iter(&self) -> impl Iterator<Item = &Line> {
        self.chunks.iter().flat_map(|chunk| chunk.lines.iter())
    }

    /// Lines `start..end`, top to bottom.
    pub(super) fn range(&self, start: usize, end: usize) -> impl Iterator<Item = &Line> {
        let end = end.min(self.len);
        let start = start.min(end);
        let (chunk, offset, _) = self.locate(start);
        self.chunks[chunk..]
            .iter()
            .flat_map(|chunk| chunk.lines.iter())
            .skip(offset)
            .take(end - start)
    }

    /// The last line before `before` whose indentation is narrower than
    /// `narrower`.
    pub(super) fn last_before(&self, before: usize, narrower: usize) -> Option<usize> {
        let before = before.min(self.len);
        if before == 0 {
            return None;
        }
        let (start_chunk, offset, base) = self.locate(before - 1);
        let mut base = base;
        for at in (0..=start_chunk).rev() {
            let chunk = &self.chunks[at];
            if at != start_chunk {
                base -= chunk.lines.len();
            }
            if chunk.narrowest >= narrower {
                continue;
            }
            let end = if at == start_chunk {
                offset + 1
            } else {
                chunk.lines.len()
            };
            if let Some(found) = chunk.lines[..end]
                .iter()
                .rposition(|line| line.width.is_some_and(|width| width < narrower))
            {
                return Some(base + found);
            }
        }
        None
    }

    /// The first line at or after `from` whose indentation is at most
    /// `widest`.
    pub(super) fn first_from(&self, from: usize, widest: usize) -> Option<usize> {
        if from >= self.len {
            return None;
        }
        let (start_chunk, offset, mut base) = self.locate(from);
        for (at, chunk) in self.chunks.iter().enumerate().skip(start_chunk) {
            let skip = if at == start_chunk { offset } else { 0 };
            if chunk.narrowest <= widest
                && let Some(found) = chunk.lines[skip..]
                    .iter()
                    .position(|line| line.width.is_some_and(|width| width <= widest))
            {
                return Some(base + skip + found);
            }
            base += chunk.lines.len();
        }
        None
    }

    /// The index of every typed line, top to bottom, passing over chunks
    /// that hold none.
    pub(super) fn typed(&self) -> impl Iterator<Item = (usize, &Line)> {
        let mut base = 0;
        self.chunks
            .iter()
            .map(move |chunk| {
                let start = base;
                base += chunk.lines.len();
                (start, chunk)
            })
            .filter(|(_, chunk)| chunk.typed > 0)
            .flat_map(|(start, chunk)| {
                chunk
                    .lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.is_typed())
                    .map(move |(offset, line)| (start + offset, line))
            })
    }
}

/// `lines` cut into chunks of [`CHUNK`], or one empty chunk for none.
fn cut(lines: Vec<Line>) -> Vec<Chunk> {
    if lines.len() <= 2 * CHUNK {
        return vec![Chunk::new(lines)];
    }
    let mut chunks = Vec::with_capacity(lines.len().div_ceil(CHUNK));
    let mut rest = lines.into_iter();
    loop {
        let piece: Vec<Line> = rest.by_ref().take(CHUNK).collect();
        if piece.is_empty() {
            break;
        }
        chunks.push(Chunk::new(piece));
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::{CHUNK, Line, Lines, Parent};
    use crate::outline::LineState;

    /// A line of width `width`, or a blank one.
    fn line(width: Option<usize>, typed: bool) -> Line {
        Line {
            text: String::new(),
            origin: (!typed).then_some(0),
            epoch: 0,
            width,
            matches: true,
            parent: Parent::Top,
            state: LineState::Loaded,
        }
    }

    /// The widths, read back.
    fn widths(lines: &Lines) -> Vec<Option<usize>> {
        lines.iter().map(|line| line.width).collect()
    }

    /// The same queries answered by looking at every line.
    fn check(lines: &Lines) {
        let all = widths(lines);
        assert_eq!(all.len(), lines.len());
        for at in 0..=all.len() {
            for limit in 0..6 {
                let expected = all[..at]
                    .iter()
                    .rposition(|width| width.is_some_and(|width| width < limit));
                assert_eq!(
                    lines.last_before(at, limit),
                    expected,
                    "last_before({at}, {limit})"
                );
                let expected = all
                    .iter()
                    .enumerate()
                    .skip(at)
                    .find(|(_, width)| width.is_some_and(|width| width <= limit))
                    .map(|(index, _)| index);
                assert_eq!(
                    lines.first_from(at, limit),
                    expected,
                    "first_from({at}, {limit})"
                );
            }
        }
        let typed: Vec<usize> = lines.typed().map(|(index, _)| index).collect();
        let expected: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, line)| line.is_typed())
            .map(|(index, _)| index)
            .collect();
        assert_eq!(typed, expected);
    }

    #[test]
    fn splices_across_chunks_keep_every_line_and_every_answer() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = |bound: usize| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            usize::try_from(seed % u64::try_from(bound.max(1)).unwrap_or(1)).unwrap_or(0)
        };
        let mut mirror: Vec<Option<usize>> = Vec::new();
        let mut lines = Lines::new(Vec::new());
        for step in 0..300 {
            let start = next(mirror.len() + 1);
            let end = (start + next(3 * CHUNK)).min(mirror.len());
            let count = if step % 4 == 0 {
                next(4 * CHUNK)
            } else {
                next(8)
            };
            let new: Vec<Option<usize>> = (0..count)
                .map(|_| (next(6) != 0).then(|| next(5)))
                .collect();
            let made = new.iter().map(|&width| line(width, next(5) == 0)).collect();
            let removed = lines.splice(start, end, made);
            let gone: Vec<Option<usize>> = mirror.splice(start..end, new).collect();
            assert_eq!(
                removed.iter().map(|line| line.width).collect::<Vec<_>>(),
                gone
            );
            assert_eq!(widths(&lines), mirror, "after step {step}");
            if step % 25 == 0 {
                check(&lines);
            }
        }
        check(&lines);
    }
}
