//! Turning an edited list of rows into a validated plan of filesystem
//! operations.
//!
//! Pure: this module never touches the disk. It reads the tree to learn what
//! each row *was*, compares that to what the row now says, and produces an
//! ordered list of operations — or a list of reasons it will not. Executing
//! the plan is [`super::apply`]'s job, and it re-checks everything.
//!
//! # Rows are identified by their origin, never by name
//!
//! This is the rule the whole feature rests on, and
//! `iridium_explorer`'s own documentation was written for it: match rows by
//! name and a rename reads as a delete plus a create, which for a large file
//! means destroying its contents and writing them back rather than moving it.
//! A row that came from the tree carries its id; a row the user typed carries
//! `None`, and that is the only thing that makes it a creation.
//!
//! # Why a rename is decided by the NAME and not by the PATH
//!
//! A row's path changes when any directory above it is renamed. Renaming
//! `src` to `source` moves `src/main.rs` to `source/main.rs` as a consequence
//! — the filesystem does it, and emitting a second operation for the child
//! would look for a file that the first operation already moved.
//!
//! So a row needs a rename **only when its own name changed**, and the paths
//! either side of that rename are computed against where its parent will be
//! *by then*. Parents are therefore always ordered before their children.
//!
//! # Ordering is not a tidiness question
//!
//! Renaming `a` to `b` and `b` to `c` in the order typed destroys the original
//! `b`. Siblings are topologically sorted so that a name is vacated before
//! anything moves into it, and a genuine cycle — `a` to `b` and `b` to `a` —
//! is broken by routing one of them through a temporary name.
//!
//! Deletions run last, so a rename out of a directory that is being deleted
//! still finds its source.

// Step 1 of #58. Nothing calls this yet: the panel's edit mode, the
// confirmation view and `apply` are the callers, and they are the next
// commits. `expect` rather than `allow` deliberately — the moment a caller
// exists this attribute becomes an unfulfilled expectation and warns, so it
// removes itself rather than quietly outliving its reason.
// Scoped to the non-test build because the tests below DO use every item, so
// under `--all-targets` the lint never fires and a bare `expect` would itself
// be an unfulfilled expectation.
#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "step 1 of the editable file list; wired up by the panel's edit mode"
    )
)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

/// What a row was when the buffer loaded.
///
/// **Captured from the node once, when the buffer is built, and carried by the
/// row from then on.** This is how the by-id rule is kept: the pairing between
/// a row and the thing it came from is established by the panel while it still
/// holds the `NodeId`, and is never re-derived afterwards by comparing
/// names. A row that carries its own past cannot be mismatched with somebody
/// else's.
///
/// It is also why this module needs no access to the tree at all, and so
/// cannot consult one at the wrong moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowOrigin {
    /// The name the node had when the buffer loaded.
    pub name: String,
    /// Where the node was when the buffer loaded.
    pub path: PathBuf,
}

/// The name a rename is routed through when siblings form a cycle.
///
/// A leading dot and a name nothing would choose, so that a crash between the
/// two halves of a cycle leaves something identifiable rather than something
/// that looks like the user's own file.
const TEMPORARY: &str = ".iridium-oil-swap";

/// One row of the edited buffer, as the panel holds it.
///
/// `parent` indexes back into the same slice. The panel derives it from row
/// indentation; the plan does not care how, only that a parent appears before
/// its children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditedRow {
    /// What this row was, or `None` for a row the user typed.
    pub origin: Option<RowOrigin>,
    /// The name as it now reads, without any trailing `/`.
    pub name: String,
    /// The row holding this one, indexed into the same slice.
    pub parent: Option<usize>,
    /// Whether the row is struck through for deletion.
    pub deleted: bool,
    /// Whether a *created* row should be a directory. Meaningless for a row
    /// that already exists, whose kind is whatever it is on disk.
    pub directory: bool,
}

/// One thing that will be done to the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Make a file or directory that does not exist.
    Create {
        /// Where.
        path: PathBuf,
        /// A directory rather than an empty file.
        directory: bool,
    },
    /// Move a path to another name in the same directory.
    Rename {
        /// Where it is now.
        from: PathBuf,
        /// Where it goes.
        to: PathBuf,
    },
    /// Remove a path, and everything under it if it is a directory.
    Delete {
        /// What goes.
        path: PathBuf,
    },
}

impl Operation {
    /// How this operation reads in the confirmation, which is the only place
    /// a user sees it before it happens.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Create { path, directory } => format!(
                "create {}{}",
                display(path),
                if *directory { "/" } else { "" }
            ),
            Self::Rename { from, to } => {
                format!("rename {} → {}", display(from), display(to))
            },
            Self::Delete { path } => format!("delete {}", display(path)),
        }
    }
}

/// A row the plan will not act on, and why.
///
/// Carries the row so the panel can put the cursor on it. Refusals are
/// collected rather than returned one at a time: someone who mistyped two
/// names should see both, not fix one and be told about the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Which row, indexed into the slice handed in.
    pub row: usize,
    /// What is wrong with it, in a sentence.
    pub reason: String,
}

/// An ordered, validated list of operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// What will be done, in the order it must be done.
    pub operations: Vec<Operation>,
}

impl Plan {
    /// Whether there is nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }

    /// The counts, for the confirmation's first line.
    #[must_use]
    pub fn summary(&self) -> String {
        let (mut creates, mut renames, mut deletes) = (0_usize, 0_usize, 0_usize);
        for operation in &self.operations {
            match operation {
                Operation::Create { .. } => creates += 1,
                Operation::Rename { .. } => renames += 1,
                Operation::Delete { .. } => deletes += 1,
            }
        }

        let mut parts = Vec::new();
        for (count, singular) in [
            (renames, "rename"),
            (deletes, "delete"),
            (creates, "create"),
        ] {
            if count > 0 {
                parts.push(format!(
                    "{count} {singular}{}",
                    if count == 1 { "" } else { "s" }
                ));
            }
        }
        if parts.is_empty() {
            return "nothing to do".to_owned();
        }
        parts.join(", ")
    }
}

/// Builds a plan from the edited rows, or every reason it will not.
///
/// # Errors
///
/// Returns one [`Refusal`] per row that cannot be acted on. Nothing is
/// planned when any row is refused — a half-applied rename of a directory
/// tree is worse than none of it, and the user can see every problem at once.
pub fn plan(rows: &[EditedRow]) -> Result<Plan, Vec<Refusal>> {
    let mut refusals = Vec::new();

    // Where each row will live once everything above it has moved. Computed
    // in slice order, which is why a parent must precede its children — the
    // check below turns that requirement into a refusal rather than a panic
    // or a silently wrong path.
    let mut targets: Vec<Option<PathBuf>> = vec![None; rows.len()];

    for (index, row) in rows.iter().enumerate() {
        let Some(parent_index) = row.parent else {
            // A root row. It cannot be renamed — its name is the mount point
            // the panel was opened at, and changing it would rename a
            // directory nobody navigated into.
            match row.origin.as_ref() {
                Some(origin) => {
                    targets[index] = Some(origin.path.clone());
                    if origin.name != row.name {
                        refusals.push(Refusal {
                            row: index,
                            reason: "the root cannot be renamed from inside itself".to_owned(),
                        });
                    }
                    if row.deleted {
                        refusals.push(Refusal {
                            row: index,
                            reason: "the root cannot be deleted from inside itself".to_owned(),
                        });
                    }
                },
                None => refusals.push(Refusal {
                    row: index,
                    reason: "this row has no parent and is not the root".to_owned(),
                }),
            }
            continue;
        };

        if parent_index >= index {
            refusals.push(Refusal {
                row: index,
                reason: "this row is nested under one that comes after it".to_owned(),
            });
            continue;
        }

        if let Some(reason) = bad_name(&row.name) {
            refusals.push(Refusal { row: index, reason });
            continue;
        }

        let Some(parent_target) = targets[parent_index].clone() else {
            // The parent was itself refused. Say so plainly rather than
            // cascading a second, more confusing complaint about this row.
            refusals.push(Refusal {
                row: index,
                reason: "the folder holding this row could not be resolved".to_owned(),
            });
            continue;
        };
        targets[index] = Some(parent_target.join(&row.name));
    }

    collect_collisions(rows, &targets, &mut refusals);

    if !refusals.is_empty() {
        refusals.sort_by_key(|refusal| refusal.row);
        return Err(refusals);
    }

    Ok(assemble(rows, &targets))
}

/// Two rows resolving to one path, which no ordering can rescue.
fn collect_collisions(
    rows: &[EditedRow],
    targets: &[Option<PathBuf>],
    refusals: &mut Vec<Refusal>,
) {
    let mut seen: BTreeMap<&Path, usize> = BTreeMap::new();
    for (index, target) in targets.iter().enumerate() {
        // A deleted row is not going anywhere, so it cannot collide with
        // anything — and a delete-then-create-with-the-same-name is a
        // legitimate way to empty a file.
        if rows[index].deleted {
            continue;
        }
        let Some(target) = target.as_deref() else {
            continue;
        };
        if let Some(&first) = seen.get(target) {
            refusals.push(Refusal {
                row: index,
                reason: format!("this and row {first} would both be {}", display(target)),
            });
        } else {
            seen.insert(target, index);
        }
    }
}

/// Turns resolved rows into ordered operations, everything having validated.
fn assemble(rows: &[EditedRow], targets: &[Option<PathBuf>]) -> Plan {
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
                    path: origin.path.clone(),
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

/// Why a name cannot be used, if it cannot.
///
/// Checked here rather than left to the operating system because the message
/// a user needs is about the row they typed, and because a name containing a
/// separator would silently write outside the directory the row is in.
fn bad_name(name: &str) -> Option<String> {
    if name.trim().is_empty() {
        return Some("a row with no name".to_owned());
    }
    if name != name.trim() {
        return Some("a name with leading or trailing whitespace".to_owned());
    }
    if name == "." || name == ".." {
        return Some(format!("{name:?} is not a name"));
    }
    if name.contains('\0') {
        return Some("a name containing a null byte".to_owned());
    }
    // `Path::components` normalises away exactly the things that make a name
    // dangerous, so the check is that the name survives being one component.
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(single)), None) if single == name => None,
        _ => Some(format!(
            "{name:?} is a path rather than a name; a row cannot move between \
             folders by typing a separator"
        )),
    }
}

/// A path as it reads in a message.
fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
