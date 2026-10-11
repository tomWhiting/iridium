//! Preflight every source line before publishing any result selections.
use iridium_editor::{CursorState, Document, Selection};
use iridium_search::Match;
use std::collections::HashSet;

pub fn prepare(document: &Document, hits: &[Match]) -> Result<CursorState, String> {
    let mut checked = HashSet::new();
    let mut selections = Vec::with_capacity(hits.len());
    for hit in hits {
        if checked.insert(hit.line) {
            let line = document
                .line(hit.line)
                .ok_or("source line disappeared; rerun search")?;
            if line != hit.line_text.as_ref() {
                return Err("source line changed; rerun search".into());
            }
        }
        let offset = document
            .line_to_byte_offset(hit.line)
            .ok_or("invalid source line")?;
        let start = document
            .offset_to_position(offset + hit.range.start)
            .ok_or("invalid match start")?;
        let end = document
            .offset_to_position(offset + hit.range.end)
            .ok_or("invalid match end")?;
        selections.push(Selection::new(start, end));
    }
    let mut selections = selections.into_iter();
    let primary = selections.next().ok_or("no matches selected")?;
    Ok(CursorState::from_selections(primary, selections.collect()))
}
