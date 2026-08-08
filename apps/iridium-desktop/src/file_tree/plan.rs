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
//! # The drawn nesting is checked against the real one
//!
//! Every path here is computed from the parent chain the panel drew. A row
//! that came from the tree also carries where it really is, and the two must
//! agree: a row drawn inside a folder it does not live in would produce
//! operations naming paths that belong to other files. Such a row is refused
//! rather than trusted, so nothing downstream has to be careful about it.
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

use std::collections::BTreeMap;
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

        // A target is built by joining the *drawn* parent chain, and a row
        // that came from the tree separately knows where it *really* is.
        // Nothing so far has checked those agree, and every operation for this
        // row is computed from the drawn chain alone — so if they disagreed,
        // a rename would name a path this row has nothing to do with, and
        // would rename whatever happened to be sitting there while leaving the
        // row's own file untouched.
        //
        // They do agree for both row sources: the tree draws an expansion and
        // the filter keeps every ancestor of a hit, so a row's drawn parent is
        // its real one. This is what turns that from an assumption held
        // somewhere else into something checked here, where the damage would
        // be done.
        if let Some(origin) = row.origin.as_ref() {
            let drawn_in = rows[parent_index]
                .origin
                .as_ref()
                .map(|holder| holder.path.as_path());
            if drawn_in != origin.path.parent() {
                refusals.push(Refusal {
                    row: index,
                    reason: format!(
                        "{} is drawn inside a folder it does not live in",
                        display(&origin.path)
                    ),
                });
                continue;
            }
        }

        targets[index] = Some(parent_target.join(&row.name));
    }

    collect_creates_under_deleted(rows, &mut refusals);
    collect_collisions(rows, &targets, &mut refusals);

    if !refusals.is_empty() {
        refusals.sort_by_key(|refusal| refusal.row);
        return Err(refusals);
    }

    Ok(super::order::assemble(rows, &targets))
}

/// A row typed inside one that is struck through.
///
/// Creates run *before* deletes, so the file would be made and then removed
/// again with the folder around it — work done for nothing, and a
/// confirmation whose `create` line describes something that will not survive
/// the run. Worse when the folder above is itself typed and struck: it is
/// never created at all, so the create underneath it fails on a path with no
/// parent and stops the run half-applied.
///
/// Refused rather than quietly dropped along with the folder. The rows say to
/// make something, and a plan that silently declined would be a plan that did
/// not match what was on the screen.
///
/// Only creates. A *rename* out of a struck folder is legitimate and is
/// exactly what "deletes run last" exists to allow — the file is moved
/// somewhere else before its old folder goes.
fn collect_creates_under_deleted(rows: &[EditedRow], refusals: &mut Vec<Refusal>) {
    for (index, row) in rows.iter().enumerate() {
        if row.origin.is_some() || row.deleted {
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
                             nowhere to create it"
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
