//! A continuous selection scale: syntax, prose words and grapheme clusters.
mod retarget;
mod text;
mod tree;

use crate::document::{CursorState, Document, Selection};
use crate::input::keyboard::AstRequest;
#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::Tree;
#[cfg(feature = "syntax")]
use iridium_syntax::Tree;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Level {
    Syntax,
    Word,
    Character,
    Line,
    Document,
}

#[derive(Debug, Clone)]
struct Target {
    range: Range<usize>,
    level: Level,
}

#[derive(Debug, Clone)]
struct Snapshot {
    cursor: CursorState,
    targets: Vec<Target>,
}

#[derive(Debug, Default, Clone)]
pub(super) struct NavigationState {
    revision: u64,
    produced: Option<Snapshot>,
    parents: Vec<Snapshot>,
}

pub(super) const fn is_navigation(request: AstRequest) -> bool {
    matches!(
        request,
        AstRequest::NavigateFiner
            | AstRequest::NavigateCoarser
            | AstRequest::NavigateNext
            | AstRequest::NavigatePrevious
    )
}

impl NavigationState {
    pub(super) fn clear(&mut self) {
        self.produced = None;
        self.parents.clear();
    }

    pub(super) fn apply(
        &mut self,
        document: &Document,
        cursor: &CursorState,
        tree: Option<&Tree>,
        request: AstRequest,
    ) -> Option<CursorState> {
        if self.revision != document.revision()
            || self
                .produced
                .as_ref()
                .is_none_or(|last| last.cursor != *cursor)
        {
            self.clear();
        }
        self.revision = document.revision();
        if request == AstRequest::NavigateFiner {
            if let Some(previous) = self.parents.pop() {
                let result = previous.cursor.clone();
                self.produced = Some(previous);
                return Some(result);
            }
        } else if request != AstRequest::NavigateCoarser {
            self.parents.clear();
        }
        let before = Snapshot {
            cursor: cursor.clone(),
            targets: cursor
                .all_selections()
                .filter_map(|selection| {
                    let range = offsets(document, *selection)?;
                    let level = self
                        .produced
                        .as_ref()
                        .and_then(|last| last.targets.iter().find(|target| target.range == range))
                        .map_or(Level::Syntax, |target| target.level);
                    Some(Target { range, level })
                })
                .collect(),
        };
        let mut targets = Vec::new();
        let mut selections = Vec::new();
        for selection in cursor.all_selections() {
            let Some(range) = offsets(document, *selection) else {
                selections.push(*selection);
                continue;
            };
            let current = before.targets.iter().find(|target| target.range == range)?;
            let target = step(document, tree, current, request).unwrap_or_else(|| current.clone());
            let from = document.offset_to_position(target.range.start)?;
            let to = document.offset_to_position(target.range.end)?;
            selections.push(if selection.is_backward() {
                Selection::new(to, from)
            } else {
                Selection::new(from, to)
            });
            targets.push(target);
        }
        let mut selections = selections.into_iter();
        let next = CursorState::from_selections(selections.next()?, selections.collect());
        if request == AstRequest::NavigateCoarser && next != *cursor {
            self.parents.push(before);
        }
        self.produced = Some(Snapshot {
            cursor: next.clone(),
            targets,
        });
        (next != *cursor).then_some(next)
    }

    pub(super) fn label(&self, document: &Document, cursor: &CursorState) -> Option<&'static str> {
        let state = self
            .produced
            .as_ref()
            .filter(|state| state.cursor == *cursor && self.revision == document.revision())?;
        let range = offsets(document, cursor.primary)?;
        match state
            .targets
            .iter()
            .find(|target| target.range == range)?
            .level
        {
            Level::Syntax => None,
            Level::Word => Some("word"),
            Level::Character => Some("character"),
            Level::Line => Some("line"),
            Level::Document => Some("document"),
        }
    }
}

fn offsets(document: &Document, selection: Selection) -> Option<Range<usize>> {
    Some(
        document.position_to_offset(selection.start())?
            ..document.position_to_offset(selection.end())?,
    )
}

const fn target(range: Range<usize>, level: Level) -> Target {
    Target { range, level }
}

fn step(
    document: &Document,
    tree: Option<&Tree>,
    current: &Target,
    request: AstRequest,
) -> Option<Target> {
    match request {
        AstRequest::NavigateFiner => finer(document, tree, current),
        AstRequest::NavigateCoarser => coarser(document, tree, current),
        AstRequest::NavigateNext | AstRequest::NavigatePrevious => {
            let forward = request == AstRequest::NavigateNext;
            match current.level {
                Level::Character => text::adjacent_character(document, &current.range, forward)
                    .map(|range| target(range, Level::Character)),
                Level::Word => text::adjacent_word(document, &current.range, forward)
                    .map(|range| target(range, Level::Word)),
                Level::Line => text::adjacent_line(document, &current.range, forward)
                    .map(|range| target(range, Level::Line)),
                Level::Document => None,
                Level::Syntax => tree::sibling(tree, &current.range, forward)
                    .map(|range| target(range, Level::Syntax))
                    .or_else(|| {
                        if tree.is_none() {
                            text::adjacent_line(document, &current.range, forward)
                                .map(|range| target(range, Level::Line))
                        } else {
                            None
                        }
                    }),
            }
        },
        _ => None,
    }
}

fn finer(document: &Document, tree: Option<&Tree>, current: &Target) -> Option<Target> {
    let range = &current.range;
    if current.level == Level::Character {
        return text::adjacent_character(document, range, true)
            .map(|range| target(range, Level::Character));
    }
    if matches!(current.level, Level::Syntax | Level::Document) {
        if let Some(child) = tree::child(tree, range) {
            return Some(target(child, Level::Syntax));
        }
        if tree.is_none() {
            let line = text::line(document, range.start)?;
            if line != *range && !line.is_empty() {
                return Some(target(line, Level::Line));
            }
        }
    }
    let extent = if range.is_empty() {
        text::line(document, range.start)?
    } else {
        range.clone()
    };
    if current.level != Level::Word {
        if let Some(word) = text::first_word(document, &extent) {
            if word != *range {
                return Some(target(word, Level::Word));
            }
        }
    }
    let character = text::character(document, extent.start)?;
    if character != *range {
        return Some(target(character, Level::Character));
    }
    text::adjacent_character(document, range, true).map(|range| target(range, Level::Character))
}

fn coarser(document: &Document, tree: Option<&Tree>, current: &Target) -> Option<Target> {
    if current.level == Level::Character {
        if let Some(word) = text::containing_word(document, &current.range) {
            if word != current.range {
                return Some(target(word, Level::Word));
            }
        }
    }
    if let Some(parent) = tree::parent(tree, &current.range) {
        return Some(target(parent, Level::Syntax));
    }
    if tree.is_none() && current.level != Level::Line {
        let line = text::line(document, current.range.start)?;
        if line.start <= current.range.start
            && line.end >= current.range.end
            && line != current.range
        {
            return Some(target(line, Level::Line));
        }
    }
    let whole = 0..document.byte_count();
    (whole != current.range && !whole.is_empty()).then_some(target(whole, Level::Document))
}
