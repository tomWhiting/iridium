//! Wrapped movement uses exactly the renderer's measured grapheme rows.
use super::wrapping::measure;
use iridium_editor::editor::LineMapping;
use iridium_editor::input::keyboard::visual_navigation::{VisualMotion, VisualNavigation};
use iridium_editor::{Document, Position};

#[derive(Debug)]
pub(super) struct Navigation {
    pub width: usize,
    pub tab_width: usize,
}
impl VisualNavigation for Navigation {
    fn layout_key(&self) -> (usize, usize) {
        (self.width, self.tab_width)
    }
    fn move_rows(
        &self,
        document: &Document,
        folds: &LineMapping,
        motion: VisualMotion,
    ) -> Result<(Position, usize), String> {
        let mut line = motion.head.line;
        let (mut layout, mut ranges) = measure(document, self.tab_width, line, self.width)?;
        let cell = layout.column_to_cell(motion.head.column);
        let mut index = ranges
            .iter()
            .position(|row| row.contains(&cell))
            .ok_or("position is outside measured rows")?;
        let column = motion
            .preferred
            .unwrap_or_else(|| cell.saturating_sub(ranges[index].start));
        let mut remaining = motion.rows;
        while remaining > 0 {
            let available = if motion.down {
                ranges.len() - index - 1
            } else {
                index
            };
            let step = remaining.min(available);
            index = if motion.down {
                index + step
            } else {
                index - step
            };
            remaining -= step;
            if remaining == 0 {
                break;
            }
            let visual = folds.project_document(line);
            let next = if motion.down {
                visual.saturating_add(1)
            } else {
                visual.saturating_sub(1)
            };
            let target = folds.visual_to_document(next);
            if target == line || target >= document.line_count() {
                break;
            }
            line = target;
            (layout, ranges) = measure(document, self.tab_width, line, self.width)?;
            index = if motion.down { 0 } else { ranges.len() - 1 };
            remaining -= 1;
        }
        let row = &ranges[index];
        let target = row
            .start
            .saturating_add(column)
            .min(row.end.saturating_sub(1));
        Ok((Position::new(line, layout.cell_to_column(target)), column))
    }
}
