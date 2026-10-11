//! Pure, cancellable replacement preview. No source is edited or saved here.
use crate::{LINE_LIMIT, MATCH_LIMIT, Match, QUERY_LIMIT, Query, SNAPSHOT_LIMIT};
use std::collections::{BTreeMap, HashMap};

/// A validated original match and its exact proposed replacement bytes.
#[derive(Clone, Debug)]
pub struct Replacement {
    source: Match,
    text: String,
}
impl Replacement {
    /// Source/provenance used during preview, requiring host revalidation.
    #[must_use]
    pub const fn source(&self) -> &Match {
        &self.source
    }
    /// Original selected source text.
    #[must_use]
    pub fn original(&self) -> &str {
        &self.source.line_text[self.source.range.clone()]
    }
    /// Replacement bytes, after capture expansion when regex mode is enabled.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Produce an all-or-error preview for chosen project matches.
///
/// Literal search uses literal replacement. Regex replacement follows regex's
/// numbered/named capture and escaped-dollar syntax. Output is bounded to 1 MiB
/// per replacement and 32 MiB in total, with at most 10,000 input matches.
/// Cancellation is checked between captures; callers should use a worker for UI.
/// This validates query/source consistency, not the current live source revision.
///
/// # Errors
/// Invalid regex, stale/forged source ranges, inconsistent source lines, overlapping
/// or duplicate matches, cancellation or exceeded limits. No partial plan escapes.
pub fn preview_replacements(
    query: &Query,
    hits: &[Match],
    replacement: &str,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Vec<Replacement>, String> {
    if replacement.len() > QUERY_LIMIT {
        return Err("replacement field exceeds 16 KiB".into());
    }
    if hits.len() > MATCH_LIMIT {
        return Err("replacement exceeds 10,000 matches".into());
    }
    let regex = crate::compile(query)?;
    let names: HashMap<_, _> = regex
        .capture_names()
        .enumerate()
        .filter_map(|(index, name)| name.map(|name| (name, index)))
        .collect();
    let mut lines = BTreeMap::new();
    for (index, hit) in hits.iter().enumerate() {
        if hit.line_text.len() > LINE_LIMIT {
            return Err("replacement source line exceeds 1 MiB".into());
        }
        if hit.line_text.get(hit.range.clone()).is_none() {
            return Err("invalid replacement source range".into());
        }
        lines
            .entry((hit.path.clone(), hit.line))
            .or_insert_with(Vec::new)
            .push(index);
    }
    let mut output = vec![None; hits.len()];
    let mut bytes = 0usize;
    for indices in lines.values_mut() {
        if cancelled() {
            return Err("replacement preview cancelled".into());
        }
        indices.sort_by_key(|index| (hits[*index].range.start, hits[*index].range.end));
        let Some(first) = indices.first().map(|index| &hits[*index]) else {
            continue;
        };
        for pair in indices.windows(2) {
            let (left, right) = (&hits[pair[0]], &hits[pair[1]]);
            let overlaps = left.range.end > right.range.start;
            if overlaps || left.range == right.range {
                return Err("replacement matches overlap or duplicate a range".into());
            }
        }
        for index in indices.iter() {
            if hits[*index].line_text != first.line_text || hits[*index].origin != first.origin {
                return Err("replacement matches disagree about their source snapshot".into());
            }
        }
        // Scan each source line once, using the same non-overlapping matching
        // semantics as search. captures_at for each hit could accept a forged
        // empty match that find_iter deliberately suppresses after a nonempty hit.
        let mut pending = indices.iter();
        let mut current = pending.next();
        for captures in regex.captures_iter(&first.line_text) {
            if cancelled() {
                return Err("replacement preview cancelled".into());
            }
            let Some(index) = current else {
                break;
            };
            let found = captures.get(0).ok_or("regex returned no whole match")?;
            if found.start() < hits[*index].range.start {
                continue;
            }
            if found.range() != hits[*index].range {
                return Err("query no longer matches the preview source".into());
            }
            let text = if query.regex {
                expand(&captures, &names, replacement)?
            } else {
                replacement.to_owned()
            };
            bytes = bytes
                .checked_add(text.len())
                .ok_or("replacement size overflow")?;
            if bytes > SNAPSHOT_LIMIT {
                return Err("replacement preview exceeds 32 MiB".into());
            }
            output[*index] = Some(Replacement {
                source: hits[*index].clone(),
                text,
            });
            current = pending.next();
        }
        if current.is_some() {
            return Err("query no longer matches the preview source".into());
        }
    }
    output
        .into_iter()
        .map(|entry| entry.ok_or_else(|| "missing replacement preview".to_owned()))
        .collect()
}
fn expand(
    captures: &regex::Captures<'_>,
    names: &HashMap<&str, usize>,
    template: &str,
) -> Result<String, String> {
    // Reuse regex's interpolation parser for exact sizing without materializing
    // amplified capture text. Only literals (at most 16 KiB) are appended here.
    let mut literals = String::new();
    let mut captured_bytes = 0usize;
    regex_automata::util::interpolate::string(
        template,
        |index, _| {
            captured_bytes = captured_bytes
                .saturating_add(captures.get(index).map_or(0, |capture| capture.len()));
        },
        |name| names.get(name).copied(),
        &mut literals,
    );
    let size = literals.len().saturating_add(captured_bytes);
    if size > LINE_LIMIT {
        return Err("expanded replacement exceeds 1 MiB".into());
    }
    let mut text = String::with_capacity(size);
    captures.expand(template, &mut text);
    Ok(text)
}
#[cfg(test)]
mod tests;
