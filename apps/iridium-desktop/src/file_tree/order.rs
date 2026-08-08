//! Turning validated rows into an ordered list of operations.
//!
//! [`super::plan`] decided *what* each row means and refused everything it
//! could not answer for; this decides **in what order**, which is not a
//! tidiness question. Renaming `a` to `b` and `b` to `c` in the order typed
//! destroys the original `b`, and a genuine cycle — `a` to `b`, `b` to `a` —
//! has no order at all and has to be routed through a temporary name.
//!
//! Split from the validation because the two fail differently. A defect here
//! is an operation that runs at the wrong moment and destroys something that
//! was still needed; a defect there is an operation that should never have
//! been produced. Nothing in this module refuses anything: by the time it runs
//! every row has already been resolved to a target.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::plan::{EditedRow, Operation, Plan, RowOrigin};

/// The name a rename is routed through when siblings form a cycle.
///
/// A leading dot and a name nothing would choose, so that a crash between the
/// two halves of a cycle leaves something identifiable rather than something
/// that looks like the user's own file.
const TEMPORARY: &str = ".iridium-oil-swap";

/// Turns resolved rows into ordered operations, everything having validated.
pub(super) fn assemble(rows: &[EditedRow], targets: &[Option<PathBuf>]) -> Plan {
    let mut creates = Vec::new();
    let mut deletes = Vec::new();
    // Renames grouped by the directory they happen in, because that is the
    // only scope in which two of them can collide.
    let mut by_directory: BTreeMap<PathBuf, Vec<(String, String)>> = BTreeMap::new();

    for (index, row) in rows.iter().enumerate() {
        let Some(target) = targets[index].as_ref() else {
            continue;
        };

        if row.deleted {
            // Only something that exists can be deleted. A typed row that was
            // then struck through never existed, and is simply dropped.
            if let Some(origin) = row.origin.as_ref() {
                deletes.push(Operation::Delete {
                    path: deleted_path(rows, targets, index, origin),
                });
            }
            continue;
        }

        let Some(original) = row.origin.as_ref().map(|origin| origin.name.as_str()) else {
            creates.push(Operation::Create {
                path: target.clone(),
                directory: row.directory,
            });
            continue;
        };

        if original == row.name {
            continue;
        }

        // The directory the rename happens *in* is the parent's target, which
        // is this row's target with the last component removed. Using the
        // parent's future location is what makes a rename inside a renamed
        // folder correct.
        if let Some(directory) = target.parent() {
            by_directory
                .entry(directory.to_path_buf())
                .or_default()
                .push((original.to_owned(), row.name.clone()));
        }
    }

    let mut operations = Vec::new();
    // Shallow directories before deep ones, so a folder is renamed before the
    // renames inside it are expressed against its new path. `BTreeMap` orders
    // by path, and a parent path sorts before every path beneath it.
    for (directory, renames) in by_directory {
        operations.extend(order_renames(&directory, renames));
    }
    operations.extend(creates);
    operations.extend(deletes);

    Plan { operations }
}

/// Where a struck row will be **by the time the deletes run**.
///
/// ⚠️ Not `origin.path`, and the difference is the sharpest edge in this
/// module. Deletes are emitted last, after every rename, so by the time one
/// executes the folders above it have already moved. A row's original path
/// therefore names where it *was*, not where it is when the removal happens.
///
/// Two failures come out of using the stale path, and the second is silent.
/// Rename `old` to `new` and strike `old/keep.rs`, and the delete looks for
/// `old/keep.rs`, which no longer exists — the run stops half-applied, which is
/// recoverable. Now swap two folders, `a` to `b` and `b` to `a`, and strike
/// `a/x`: the cycle resolves correctly, `a` is afterwards the directory that
/// was `b`, and a delete of `a/x` **removes a different, live file** while the
/// confirmation showed the perfectly reasonable line `delete a/x`.
///
/// The fix is the derivation renames already use: the row's **original name**
/// under its parent's **future** path. A parent that is not being renamed
/// resolves to exactly the original path, so nothing else changes.
///
/// The name is the origin's rather than the row's because striking a row and
/// retyping its name are independent — the row on disk is still called what it
/// was called, and that is what has to be removed.
fn deleted_path(
    rows: &[EditedRow],
    targets: &[Option<PathBuf>],
    index: usize,
    origin: &RowOrigin,
) -> PathBuf {
    // The root has no parent and cannot be struck through — that is refused
    // above — so this fallback is for a shape the validation already rejected.
    // Falling back to the original path keeps the answer honest rather than
    // inventing one.
    rows[index]
        .parent
        .and_then(|parent| targets[parent].as_ref())
        .map_or_else(|| origin.path.clone(), |holder| holder.join(&origin.name))
}

/// Orders one directory's renames so no name is occupied when something moves
/// into it, breaking any cycle with a temporary.
fn order_renames(directory: &Path, renames: Vec<(String, String)>) -> Vec<Operation> {
    // Both sides of every rename: a temporary must collide with neither a
    // name being vacated nor one being moved into.
    let taken: BTreeSet<String> = renames
        .iter()
        .flat_map(|(from, to)| [from.clone(), to.clone()])
        .collect();
    let mut pending: Vec<Option<(String, String)>> = renames.into_iter().map(Some).collect();
    let mut operations = Vec::new();

    // Repeatedly emit any rename whose destination is not still occupied by a
    // source waiting its turn. Each pass either emits something or proves the
    // rest is a cycle, so this terminates.
    loop {
        let occupied: BTreeSet<String> = pending
            .iter()
            .flatten()
            .map(|(from, _)| from.clone())
            .collect();

        let mut progressed = false;
        for slot in &mut pending {
            let Some((from, to)) = slot.as_ref() else {
                continue;
            };
            if occupied.contains(to) && from != to {
                continue;
            }
            operations.push(Operation::Rename {
                from: directory.join(from),
                to: directory.join(to),
            });
            *slot = None;
            progressed = true;
        }

        if pending.iter().all(Option::is_none) {
            return operations;
        }
        if progressed {
            continue;
        }

        // Everything left is in a cycle. Move one of them out of the way
        // through a temporary, which frees its name and lets the next pass
        // unwind the rest.
        //
        // Only the FIRST half is emitted here. The second half goes back into
        // `pending` with the temporary as its source, so it is emitted by the
        // ordinary rule once its destination is actually free. Emitting both
        // halves immediately is the obvious thing to do and it is wrong — it
        // moves the temporary onto a name the cycle has not vacated yet, and
        // destroys what is there.
        let Some(slot) = pending.iter_mut().find(|slot| slot.is_some()) else {
            return operations;
        };
        let Some((from, to)) = slot.take() else {
            return operations;
        };
        let temporary = temporary_name(&taken);
        operations.push(Operation::Rename {
            from: directory.join(&from),
            to: directory.join(&temporary),
        });
        *slot = Some((temporary, to));
    }
}

/// A name in `directory` that no rename is already using.
fn temporary_name(taken: &BTreeSet<String>) -> String {
    let mut candidate = TEMPORARY.to_owned();
    let mut suffix = 0_u32;
    while taken.contains(&candidate) {
        suffix += 1;
        candidate = format!("{TEMPORARY}-{suffix}");
    }
    candidate
}
