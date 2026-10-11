//! Move an object selection to a spatial target without changing its level.
use super::{Level, NavigationState, Snapshot, Target, offsets, step, target, text, tree};
#[cfg(not(feature = "syntax"))]
use crate::syntax_stubs::Tree;
use crate::{CursorState, Document, Position, Selection};
#[cfg(feature = "syntax")]
use iridium_syntax::Tree;

impl NavigationState {
    pub(in crate::editor::ast) fn retarget(
        &mut self,
        document: &Document,
        tree: Option<&Tree>,
        before: &CursorState,
        moved: &CursorState,
        down: bool,
    ) -> CursorState {
        self.resolve(
            document,
            tree,
            before,
            moved,
            |current, destination, selection| {
                if destination.head == selection.start() {
                    None
                } else {
                    at_position(document, tree, current, destination.head, down)
                }
            },
        )
    }

    pub(in crate::editor::ast) fn horizontal(
        &mut self,
        document: &Document,
        tree: Option<&Tree>,
        before: &CursorState,
        forward: bool,
        count: u32,
    ) -> CursorState {
        use crate::input::keyboard::{AstRequest, motions};
        let mut moved = before.clone();
        for selection in moved.all_selections_mut() {
            let mut head = selection.head;
            for _ in 0..count {
                let next = if forward {
                    motions::char_right(document, head)
                } else {
                    motions::char_left(document, head)
                };
                if next == head {
                    break;
                }
                head = next;
            }
            *selection = Selection::collapsed(head);
        }
        let request = if forward {
            AstRequest::NavigateNext
        } else {
            AstRequest::NavigatePrevious
        };
        self.resolve(document, tree, before, &moved, |current, _, _| {
            let mut next = current.clone();
            for _ in 0..count {
                let Some(target) = step(document, tree, &next, request) else {
                    break;
                };
                if target.range == next.range {
                    break;
                }
                next = target;
            }
            Some(next)
        })
    }

    fn resolve(
        &mut self,
        document: &Document,
        tree: Option<&Tree>,
        before: &CursorState,
        moved: &CursorState,
        motion: impl Fn(&Target, &Selection, &Selection) -> Option<Target>,
    ) -> CursorState {
        let retained = self
            .produced
            .as_ref()
            .filter(|last| last.cursor == *before && self.revision == document.revision());
        let mut selections = Vec::new();
        let mut targets = Vec::new();
        for (selection, destination) in before.all_selections().zip(moved.all_selections()) {
            let Some(range) = offsets(document, *selection).filter(|range| !range.is_empty())
            else {
                selections.push(*destination);
                continue;
            };
            let current = retained
                .and_then(|last| last.targets.iter().find(|t| t.range == range))
                .cloned()
                .or_else(|| infer(document, tree, &range).map(|level| target(range, level)));
            let Some(current) = current else {
                selections.push(*destination);
                continue;
            };
            let next = motion(&current, destination, selection).unwrap_or(current);
            let selected = document
                .offset_to_position(next.range.start)
                .zip(document.offset_to_position(next.range.end))
                .map_or(*selection, |(start, end)| {
                    if selection.is_backward() {
                        Selection::new(end, start)
                    } else {
                        Selection::new(start, end)
                    }
                });
            selections.push(selected);
            targets.push(next);
        }
        let mut selections = selections.into_iter();
        let next = selections.next().map_or_else(
            || before.clone(),
            |first| CursorState::from_selections(first, selections.collect()),
        );
        self.parents.clear();
        self.revision = document.revision();
        self.produced = Some(Snapshot {
            cursor: next.clone(),
            targets,
        });
        next
    }
}
fn infer(
    document: &Document,
    tree: Option<&Tree>,
    range: &std::ops::Range<usize>,
) -> Option<Level> {
    if tree::exact_kind(tree, range).is_some() {
        return Some(Level::Syntax);
    }
    if text::line(document, range.start).as_ref() == Some(range) {
        return Some(Level::Line);
    }
    if text::containing_word(document, range).as_ref() == Some(range) {
        return Some(Level::Word);
    }
    if text::character(document, range.start).as_ref() == Some(range) {
        return Some(Level::Character);
    }
    None
}
fn at_position(
    document: &Document,
    tree: Option<&Tree>,
    current: &Target,
    position: Position,
    down: bool,
) -> Option<Target> {
    let offset = document.position_to_offset(position)?;
    let range = match current.level {
        Level::Syntax => tree::spatial_peer(
            tree,
            &current.range,
            Position::new(
                position.line,
                offset.saturating_sub(document.line_to_byte_offset(position.line)?),
            ),
            down,
        )?,
        Level::Word => word_near(document, position, down)?,
        Level::Character => text::character(document, offset)?,
        Level::Line => text::line(document, offset)
            .filter(|range| !range.is_empty())
            .or_else(|| text::character(document, offset))?,
        Level::Document => current.range.clone(),
    };
    (!range.is_empty()).then(|| target(range, current.level))
}
fn word_near(
    document: &Document,
    position: Position,
    down: bool,
) -> Option<std::ops::Range<usize>> {
    use unicode_segmentation::UnicodeSegmentation as _;
    let mut line = position.line;
    loop {
        let text = document.line(line)?;
        let start = document.line_to_byte_offset(line)?;
        let column = position.column.min(text.chars().count());
        let offset = text
            .char_indices()
            .nth(column)
            .map_or(text.len(), |(offset, _)| offset);
        if let Some((word, _)) = text
            .unicode_word_indices()
            .map(|(at, word)| {
                let end = at + word.len();
                (
                    start + at..start + end,
                    if offset < at {
                        at - offset
                    } else {
                        offset.saturating_sub(end.saturating_sub(1))
                    },
                )
            })
            .min_by_key(|(_, distance)| *distance)
        {
            return Some(word);
        }
        line = if down {
            line.checked_add(1)?
        } else {
            line.checked_sub(1)?
        };
    }
}
