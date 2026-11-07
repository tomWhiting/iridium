//! Turning an edited list of rows into a validated plan of filesystem
//! operations.
//!
//! Pure: this module never touches the disk. It reads the tree to learn what
//! each row *was*, compares that to what the row now says, and produces an
//! ordered list of operations — or a list of reasons it will not. Executing
//! the plan is [`super::apply`](mod@super::apply)'s job, and it re-checks everything.
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
//! # A row drawn under another folder moves there
//!
//! Every target here is computed from the parent chain the panel drew, and a
//! row that came from the tree also carries where it really is. When the two
//! differ — its line was cut and pasted under another folder, or re-indented
//! — the row **moves**: one [`Operation::Move`] from where it is to where it
//! is drawn, a new name included, because a rename across folders is the same
//! operation. It is never a delete and a create.
//!
//! The destination must be a folder, must not be struck through, and must not
//! be the row's own inside; a name already taken there, on screen or hidden on
//! disk, is refused rather than overwritten.
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
//! # Ordering is somebody else's job
//!
//! Renaming `a` to `b` and `b` to `c` in the order typed destroys the original
//! `b`, and a genuine cycle has no order at all. That is [`super::order`]'s
//! problem, and it is separated because the two fail differently: a defect
//! here is an operation that should never have been produced, and a defect
//! there is one that runs at the wrong moment. What this module guarantees to
//! it is that every row has been resolved to a target, so nothing over there
//! has to refuse anything.

use std::collections::{BTreeMap, HashSet};
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
    /// Whether the node is a directory, which is what decides whether a row
    /// may be drawn inside it.
    pub directory: bool,
}

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
    /// Move a path into another directory, under its old name or a new one.
    ///
    /// Carried out exactly as a rename is — one call that refuses to replace
    /// what is there — so it is undone the same way: by editing the rows to
    /// put it back. The verb differs only so the confirmation says which.
    Move {
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
            Self::Move { from, to } => format!("move {} → {}", display(from), display(to)),
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
        let (mut creates, mut renames, mut moves, mut deletes) = (0_usize, 0, 0, 0);
        for operation in &self.operations {
            match operation {
                Operation::Create { .. } => creates += 1,
                Operation::Rename { .. } => renames += 1,
                Operation::Move { .. } => moves += 1,
                Operation::Delete { .. } => deletes += 1,
            }
        }

        let mut parts = Vec::new();
        for (count, singular) in [
            (renames, "rename"),
            (moves, "move"),
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

/// Builds a plan from the edited rows, or every reason it will not, knowing
/// nothing of the disk beyond the rows.
///
/// # Errors
///
/// As [`plan_against`].
pub fn plan(rows: &[EditedRow]) -> Result<Plan, Vec<Refusal>> {
    plan_against(rows, &|_| false)
}

/// Builds a plan from the edited rows, or every reason it will not.
///
/// `exists` says whether a path is on disk now, so that a name hidden by a
/// collapsed folder or a filter is not planned over. It is asked, never
/// written through.
///
/// # Errors
///
/// Returns one [`Refusal`] per row that cannot be acted on. Nothing is
/// planned when any row is refused — a half-applied rename of a directory
/// tree is worse than none of it, and the user can see every problem at once.
pub fn plan_against(
    rows: &[EditedRow],
    exists: &dyn Fn(&Path) -> bool,
) -> Result<Plan, Vec<Refusal>> {
    let mut refusals = Vec::new();

    // Where each row will live once everything above it has moved. Computed
    // in slice order, which is why a parent must precede its children — the
    // check below turns that requirement into a refusal rather than a panic
    // or a silently wrong path.
    let mut targets: Vec<Option<PathBuf>> = vec![None; rows.len()];
    // Every path a row was loaded from.
    let loaded: HashSet<&Path> = rows
        .iter()
        .filter_map(|row| row.origin.as_ref().map(|origin| origin.path.as_path()))
        .collect();

    for (index, row) in rows.iter().enumerate() {
        let Some(parent_index) = row.parent else {
            // **Exactly one row may have nothing above it, and it is the
            // first.** A later row with no folder is one whose nesting could
            // not be worked out, and treating it as a second root would
            // quietly make it unrenamable and undeletable while telling the
            // user it is "the root" — about a row they can see is not.
            if index > 0 {
                refusals.push(Refusal {
                    row: index,
                    reason: "the folder this row is in could not be worked out from how it is \
                             drawn"
                        .to_owned(),
                });
                continue;
            }

            // The root row. It cannot be renamed — its name is the mount point
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
                    reason: "the first row must be the folder the panel is showing, not one \
                             that was typed"
                        .to_owned(),
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

        // Only a folder can hold a row. A row drawn where it already lives is
        // in a folder by definition; a typed or moved one is checked, because
        // its operation would otherwise fail half-way through the run.
        let parent = &rows[parent_index];
        let parent_is_folder = parent
            .origin
            .as_ref()
            .map_or(parent.directory, |holder| holder.directory);
        if !row.deleted && arrives(rows, row) && !parent_is_folder {
            refusals.push(Refusal {
                row: index,
                reason: format!("{} is a file, so nothing can go inside it", parent.name),
            });
            continue;
        }

        // A move is only read from rows that show where the row came from:
        // its own folder is among them. Neither row source draws a row
        // without its folder, so one that does was drawn wrongly rather than
        // moved, and planning a move from it would move a file nobody touched.
        if let Some(origin) = row.origin.as_ref()
            && arrives(rows, row)
            && origin
                .path
                .parent()
                .is_none_or(|folder| !loaded.contains(folder))
        {
            refusals.push(Refusal {
                row: index,
                reason: format!(
                    "{} is drawn without the folder it lives in",
                    display(&origin.path)
                ),
            });
            continue;
        }

        targets[index] = Some(parent_target.join(&row.name));
    }

    collect_arrivals_under_deleted(rows, &mut refusals);
    collect_collisions(rows, &targets, &mut refusals);

    if !refusals.is_empty() {
        refusals.sort_by_key(|refusal| refusal.row);
        return Err(refusals);
    }

    super::order::assemble(rows, exists).map_err(|mut refusals| {
        refusals.sort_by_key(|refusal| refusal.row);
        refusals
    })
}

/// Whether `row` is drawn somewhere it does not already live: typed, or
/// moved under another folder.
fn arrives(rows: &[EditedRow], row: &EditedRow) -> bool {
    row.origin.as_ref().is_none_or(|origin| {
        let drawn_in = row
            .parent
            .and_then(|parent| rows.get(parent))
            .and_then(|holder| holder.origin.as_ref())
            .map(|holder| holder.path.as_path());
        drawn_in != origin.path.parent()
    })
}

/// A row typed, or moved, inside one that is struck through.
///
/// The struck folder is deleted after everything that goes into it, so the
/// row would be made or moved there and then removed again with the folder
/// around it — work done for nothing, and a confirmation whose line describes
/// something that will not survive the run. Worse when the folder above is
/// itself typed and struck: it is never created at all, so the operation
/// underneath it fails on a path with no parent and stops the run
/// half-applied.
///
/// Refused rather than quietly dropped along with the folder. The rows say to
/// make or move something, and a plan that silently declined would be a plan
/// that did not match what was on the screen.
///
/// Only arrivals. A rename *inside*, or a move *out of*, a struck folder is
/// legitimate and is exactly what "deletes run last" exists to allow.
fn collect_arrivals_under_deleted(rows: &[EditedRow], refusals: &mut Vec<Refusal>) {
    for (index, row) in rows.iter().enumerate() {
        if row.deleted || !arrives(rows, row) {
            continue;
        }
        // Upwards only: each step must name a row before the one it was
        // reached from, so a malformed parent chain ends the walk rather than
        // looping. The main pass refuses such a chain anyway; this does not
        // depend on having run first.
        let mut current = index;
        while let Some(above) = rows[current].parent {
            if above >= current {
                break;
            }
            if rows[above].deleted {
                refusals.push(Refusal {
                    row: index,
                    reason: "this row is inside one that is struck through, so there would be \
                             nowhere to put it"
                        .to_owned(),
                });
                break;
            }
            current = above;
        }
    }
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
    // Typed text can carry a tab or a pasted control character that no
    // listing would show the same way again.
    if name.chars().any(char::is_control) {
        return Some("a name containing a control character".to_owned());
    }
    // `Path::components` normalises away exactly the things that make a name
    // dangerous, so the check is that the name survives being one component.
    let mut components = Path::new(name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(single)), None) if single == name => None,
        _ => Some(format!(
            "{name:?} is a path rather than a name; to move a row, put its line \
             under the folder instead"
        )),
    }
}

/// A path as it reads in a message.
fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
