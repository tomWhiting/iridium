//! Which old line each new line still is, across one edit.
//!
//! An edit leaves most lines alone, so the lines both versions start and end
//! with are the same lines. In what is left, a line whose text appears exactly
//! once on each side is the same line moved; the rest are paired in order,
//! most alike first, within the gaps those unique lines leave — which is what
//! typing into, splitting or joining lines does to them.

use std::collections::HashMap;

/// The most cells the order-keeping alignment may cost before falling back to
/// pairing in order. Reached only by an edit rewriting thousands of lines at
/// once, where the two give the same answer for lines edited in place.
const ALIGN_LIMIT: usize = 250_000;

/// The entry each line of `new` still is, given what each line of `old` was.
pub(super) fn carry(
    old: &[&str],
    old_origins: &[Option<usize>],
    new: &[&str],
) -> Vec<Option<usize>> {
    let origin = |index: usize| old_origins.get(index).copied().flatten();
    let mut origins = vec![None; new.len()];

    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(before, after)| before == after)
        .count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(room)
        .take_while(|(before, after)| before == after)
        .count();
    for (index, slot) in origins.iter_mut().enumerate().take(prefix) {
        *slot = origin(index);
    }
    for offset in 1..=suffix {
        origins[new.len() - offset] = origin(old.len() - offset);
    }

    let old_middle = prefix..old.len() - suffix;
    let new_middle = prefix..new.len() - suffix;
    if old_middle.is_empty() || new_middle.is_empty() {
        return origins;
    }

    // Text seen exactly once on each side is the same line, wherever it went.
    let mut seen: HashMap<&str, [(usize, usize); 2]> = HashMap::new();
    for index in old_middle.clone() {
        let slot = &mut seen.entry(old[index]).or_default()[0];
        *slot = (slot.0 + 1, index);
    }
    for index in new_middle.clone() {
        let slot = &mut seen.entry(new[index]).or_default()[1];
        *slot = (slot.0 + 1, index);
    }
    let mut old_taken = vec![false; old.len()];
    let mut new_taken = vec![false; new.len()];
    let mut anchors = Vec::new();
    for (text, [(old_count, old_index), (new_count, new_index)]) in seen {
        if old_count == 1 && new_count == 1 && !text.trim().is_empty() {
            origins[new_index] = origin(old_index);
            old_taken[old_index] = true;
            new_taken[new_index] = true;
            anchors.push((old_index, new_index));
        }
    }

    // What is left pairs in order, within the gaps between the unique lines
    // that kept their order: a line typed over stays the line it was typed
    // over, not one alike elsewhere. Only old lines that were an entry matter.
    // A line emptied on the way to a new name stays its entry, so blank new
    // lines take part too.
    anchors.sort_unstable_by_key(|&(_, new_index)| new_index);
    let ends = (old_middle.end, new_middle.end);
    let (mut old_from, mut new_from) = (old_middle.start, new_middle.start);
    for (old_to, new_to) in in_order(&anchors).into_iter().chain([ends]) {
        let old_left: Vec<usize> = (old_from..old_to)
            .filter(|&index| !old_taken[index] && origin(index).is_some())
            .collect();
        let new_left: Vec<usize> = (new_from..new_to)
            .filter(|&index| !new_taken[index])
            .collect();
        for (old_at, new_at) in pairs(old, &old_left, new, &new_left) {
            origins[new_left[new_at]] = origin(old_left[old_at]);
        }
        (old_from, new_from) = (old_to + 1, new_to + 1);
    }
    origins
}

/// The longest run of `anchors`, already in new-line order, whose old lines
/// are in order too.
fn in_order(anchors: &[(usize, usize)]) -> Vec<(usize, usize)> {
    // tails[k]: the anchor ending the best run of length k + 1 found so far.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; anchors.len()];
    for (at, &(old_index, _)) in anchors.iter().enumerate() {
        let length = tails.partition_point(|&tail| anchors[tail].0 < old_index);
        previous[at] = length.checked_sub(1).map(|before| tails[before]);
        if length == tails.len() {
            tails.push(at);
        } else {
            tails[length] = at;
        }
    }
    let mut run = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(index) = at {
        run.push(anchors[index]);
        at = previous[index];
    }
    run.reverse();
    run
}

/// Pairs positions of `old_left` with positions of `new_left`, keeping order.
fn pairs(
    old: &[&str],
    old_left: &[usize],
    new: &[&str],
    new_left: &[usize],
) -> Vec<(usize, usize)> {
    let (rows, columns) = (old_left.len(), new_left.len());
    if rows == columns || rows.saturating_mul(columns) > ALIGN_LIMIT {
        return (0..rows.min(columns)).map(|at| (at, at)).collect();
    }
    let old_text: Vec<&str> = old_left
        .iter()
        .map(|&index| old[index].trim_start())
        .collect();
    let new_text: Vec<&str> = new_left
        .iter()
        .map(|&index| new[index].trim_start())
        .collect();

    // best[i][j]: the most alike the first i old and first j new lines can be
    // paired, flattened row by row.
    let width = columns + 1;
    let mut best = vec![0_usize; (rows + 1) * width];
    for i in 1..=rows {
        for j in 1..=columns {
            let paired = best[(i - 1) * width + j - 1] + likeness(old_text[i - 1], new_text[j - 1]);
            best[i * width + j] = paired
                .max(best[(i - 1) * width + j])
                .max(best[i * width + j - 1]);
        }
    }

    let mut chosen = Vec::with_capacity(rows.min(columns));
    let (mut i, mut j) = (rows, columns);
    while i > 0 && j > 0 {
        let here = best[i * width + j];
        if here == best[(i - 1) * width + j - 1] + likeness(old_text[i - 1], new_text[j - 1]) {
            chosen.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if here == best[(i - 1) * width + j] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    chosen.reverse();
    chosen
}

/// How alike two lines are: one for pairing at all, plus the characters they
/// share at either end.
fn likeness(before: &str, after: &str) -> usize {
    let shortest = before.chars().count().min(after.chars().count());
    let front = before
        .chars()
        .zip(after.chars())
        .take_while(|(left, right)| left == right)
        .count();
    let back = before
        .chars()
        .rev()
        .zip(after.chars().rev())
        .take(shortest - front)
        .take_while(|(left, right)| left == right)
        .count();
    1 + front + back
}
