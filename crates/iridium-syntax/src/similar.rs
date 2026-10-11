//! Bounded, source-addressed matching from an exact named syntax-node seed.
use std::ops::Range;
use tree_sitter::{Node, TreeCursor};
const WORK_LIMIT: usize = 1_000_000;
const MATCH_LIMIT: usize = 10_000;
/// Find nonoverlapping nodes of the same kind, optionally requiring identical
/// subtree kinds/arity (identifier and literal contents may differ).
///
/// Ancestors overlapping the seed are excluded; other matching outer nodes
/// suppress their descendants. The seed is always retained.
///
/// # Errors
/// Rejects partial/error seeds and excessive work or matches without partial output.
pub fn ranges(
    root: Node<'_>,
    seed: Range<usize>,
    same_shape: bool,
) -> Result<Vec<Range<usize>>, String> {
    let node = crate::navigate::node_at(root, &seed).ok_or("Select a syntax node first")?;
    if node.byte_range() != seed || seed.is_empty() || node.has_error() || node.is_missing() {
        return Err("Select a complete, valid syntax node first".into());
    }
    let mut budget = WORK_LIMIT;
    let signature = if same_shape {
        shape(node, &mut budget)?
    } else {
        Vec::new()
    };
    let mut cursor = root.walk();
    let mut matches = Vec::new();
    loop {
        spend(&mut budget)?;
        let candidate = cursor.node();
        let range = candidate.byte_range();
        let overlaps_seed = range.start < seed.end && seed.start < range.end;
        let eligible = candidate.is_named()
            && candidate.kind_id() == node.kind_id()
            && !candidate.has_error()
            && !candidate.is_missing()
            && (!overlaps_seed || range == seed)
            && (!same_shape || shape(candidate, &mut budget)? == signature);
        if eligible {
            if matches.len() == MATCH_LIMIT {
                return Err(
                    "Similar-node matches exceed 10,000; choose a narrower node shape".into(),
                );
            }
            matches.push(range);
        }
        if !advance(&mut cursor, !eligible) {
            break;
        }
    }
    Ok(matches)
}
fn spend(budget: &mut usize) -> Result<(), String> {
    *budget = budget
        .checked_sub(1)
        .ok_or("Similar-node query exceeded its traversal budget")?;
    Ok(())
}
fn shape(node: Node<'_>, budget: &mut usize) -> Result<Vec<(u16, usize)>, String> {
    let mut result = Vec::new();
    let mut cursor = node.walk();
    loop {
        spend(budget)?;
        let node = cursor.node();
        result.push((node.kind_id(), node.child_count()));
        if !advance(&mut cursor, true) {
            break;
        }
    }
    Ok(result)
}
fn advance(cursor: &mut TreeCursor<'_>, descend: bool) -> bool {
    if descend && cursor.goto_first_child() {
        return true;
    }
    loop {
        if cursor.goto_next_sibling() {
            return true;
        }
        if !cursor.goto_parent() {
            return false;
        }
    }
}
