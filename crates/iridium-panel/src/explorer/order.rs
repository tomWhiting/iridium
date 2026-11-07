//! Turning validated rows into an ordered list of operations.
//!
//! [`super::plan`](mod@super::plan) decided *what* each row means and refused everything it
//! could tell was wrong from the rows alone; this decides **in what order**,
//! which is not a tidiness question. Renaming `a` to `b` and `b` to `c` in the
//! order typed destroys the original `b`; a file moved into a folder the same
//! plan creates has nowhere to go until the folder exists; and a genuine cycle
//! — `a` to `b`, `b` to `a` — has no order at all and has to be routed through
//! a temporary name.
//!
//! # The order is found by playing the plan out
//!
//! Every row is placed where it really is, inside the row really holding it,
//! and the operations are tried in a fixed preference — renames and moves in
//! row order, then creates, then deletes — each one emitted only once it can
//! run: the folder it goes into is where it will finally be, nothing already
//! has its name there, and for a delete, nothing inside still has to move out.
//! Children move with their folder, so a path is always worked out from where
//! everything above it is *at that moment*, never from where it was loaded.
//!
//! A pass that emits nothing is a cycle, broken by moving one of its rows to a
//! temporary name; one that cannot be broken is refused, row by row, rather
//! than run in an order that destroys something.
//!
//! # A name is checked against what is on disk, not just what is on screen
//!
//! A collapsed folder or a filter hides entries the rows know nothing about.
//! The caller supplies whether a path exists, and a destination that is a
//! hidden entry's original path is a name that will never come free.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::plan::{EditedRow, Operation, Plan, Refusal};

/// The name a rename is routed through when its destination is part of a
/// cycle.
///
/// A leading dot and a name nothing would choose, so that a crash between the
/// two halves of a cycle leaves something identifiable rather than something
/// that looks like the user's own file.
const TEMPORARY: &str = ".iridium-oil-swap";

/// What a row sits inside while the plan is played out.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Holder {
    /// Another row.
    Row(usize),
    /// A folder no row stands for, by its path.
    Dir(PathBuf),
    /// Nothing: the root, which is at its own path.
    Fixed,
}

/// One operation still to be emitted, by the row it is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Put an existing row under its drawn folder with its drawn name.
    Relocate(usize),
    /// Make a typed row.
    Create(usize),
    /// Remove a struck row.
    Delete(usize),
}

/// The rows as they stand part-way through the plan.
struct World<'a> {
    /// The rows being planned.
    rows: &'a [EditedRow],
    /// Where each row is, or `None` for one not on disk.
    holder: Vec<Option<Holder>>,
    /// What each row is called now.
    name: Vec<String>,
    /// What each holder contains, by name.
    contents: HashMap<Holder, HashMap<String, usize>>,
    /// Whether each row still has a relocation or a create to come.
    unsettled: Vec<bool>,
    /// Every path a row was loaded from, which the rows themselves track.
    loaded: HashSet<&'a Path>,
    /// Whether a path exists on disk now, before anything runs.
    exists: &'a dyn Fn(&Path) -> bool,
}

impl<'a> World<'a> {
    /// Every existing row where it really is: inside the row loaded from its
    /// folder, or that folder itself when no row stands for it.
    fn loaded(rows: &'a [EditedRow], exists: &'a dyn Fn(&Path) -> bool) -> Self {
        let mut world = World {
            rows,
            holder: vec![None; rows.len()],
            name: rows.iter().map(|row| row.name.clone()).collect(),
            contents: HashMap::new(),
            unsettled: vec![false; rows.len()],
            loaded: rows
                .iter()
                .filter_map(|row| row.origin.as_ref().map(|origin| origin.path.as_path()))
                .collect(),
            exists,
        };
        let by_path: HashMap<&Path, usize> = rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                row.origin
                    .as_ref()
                    .map(|origin| (origin.path.as_path(), index))
            })
            .collect();
        for (index, row) in rows.iter().enumerate() {
            let Some(origin) = row.origin.as_ref() else {
                continue;
            };
            let holder = if index == 0 {
                Holder::Fixed
            } else {
                origin.path.parent().map_or(Holder::Fixed, |folder| {
                    by_path
                        .get(folder)
                        .map_or_else(|| Holder::Dir(folder.to_path_buf()), |&at| Holder::Row(at))
                })
            };
            world.place(index, holder, origin.name.clone());
        }
        world
    }

    /// Where row `index` is now.
    fn path(&self, index: usize) -> PathBuf {
        match &self.holder[index] {
            Some(Holder::Row(above)) => self.path(*above).join(&self.name[index]),
            Some(Holder::Dir(directory)) => directory.join(&self.name[index]),
            Some(Holder::Fixed) | None => self.rows[index]
                .origin
                .as_ref()
                .map_or_else(PathBuf::new, |origin| origin.path.clone()),
        }
    }

    /// Whether row `index` is where it will finally be, and so is every row
    /// above it.
    fn settled(&self, index: usize) -> bool {
        let mut at = index;
        loop {
            if self.unsettled[at] {
                return false;
            }
            match &self.holder[at] {
                Some(Holder::Row(above)) => at = *above,
                Some(Holder::Dir(_) | Holder::Fixed) => return true,
                None => return false,
            }
        }
    }

    /// Whether `inner` is `outer` or anywhere inside it, now.
    fn within(&self, inner: usize, outer: usize) -> bool {
        let mut at = inner;
        loop {
            if at == outer {
                return true;
            }
            match &self.holder[at] {
                Some(Holder::Row(above)) => at = *above,
                _ => return false,
            }
        }
    }

    /// The row called `name` inside row `folder`, if any.
    fn occupant(&self, folder: usize, name: &str) -> Option<usize> {
        self.contents
            .get(&Holder::Row(folder))
            .and_then(|names| names.get(name))
            .copied()
    }

    /// Whether `name` inside `holder` is an entry on disk no row stands for:
    /// a name that will never come free.
    ///
    /// Asked of where the folder was **loaded** from, because a folder that
    /// moves takes its hidden entries with it. A folder the plan creates is
    /// empty.
    fn hidden(&self, holder: &Holder, name: &str) -> bool {
        let original = match holder {
            Holder::Row(folder) => match &self.rows[*folder].origin {
                Some(origin) => origin.path.join(name),
                None => return false,
            },
            Holder::Dir(directory) => directory.join(name),
            Holder::Fixed => return false,
        };
        !self.loaded.contains(original.as_path()) && (self.exists)(&original)
    }

    /// Moves row `index` under `holder` as `name`.
    fn place(&mut self, index: usize, holder: Holder, name: String) {
        if let Some(old) = self.holder[index].take()
            && let Some(names) = self.contents.get_mut(&old)
            && names.get(&self.name[index]) == Some(&index)
        {
            names.remove(&self.name[index]);
        }
        self.contents
            .entry(holder.clone())
            .or_default()
            .insert(name.clone(), index);
        self.holder[index] = Some(holder);
        self.name[index] = name;
    }

    /// Takes row `index` off the disk.
    fn remove(&mut self, index: usize) {
        if let Some(old) = self.holder[index].take()
            && let Some(names) = self.contents.get_mut(&old)
            && names.get(&self.name[index]) == Some(&index)
        {
            names.remove(&self.name[index]);
        }
    }
}

/// Orders the operations the rows describe, or refuses the rows no order can
/// carry out.
///
/// `exists` says whether a path is on disk before anything runs.
///
/// # Errors
///
/// One [`Refusal`] per row whose operation could not be placed: a folder
/// moved inside itself, or a name that is taken and never comes free.
pub(super) fn assemble(
    rows: &[EditedRow],
    exists: &dyn Fn(&Path) -> bool,
) -> Result<Plan, Vec<Refusal>> {
    let mut world = World::loaded(rows, exists);
    let mut pending = steps(&mut world);
    let taken: BTreeSet<&str> = rows
        .iter()
        .flat_map(|row| {
            std::iter::once(row.name.as_str())
                .chain(row.origin.as_ref().map(|origin| origin.name.as_str()))
        })
        .collect();
    let mut detoured = vec![false; rows.len()];
    let mut operations = Vec::new();

    while !pending.is_empty() {
        // Rows something still has to move out of.
        let mut occupied = vec![false; rows.len()];
        for step in &pending {
            if let Step::Relocate(index) = *step {
                let mut at = index;
                while let Some(Holder::Row(above)) = &world.holder[at] {
                    occupied[*above] = true;
                    at = *above;
                }
            }
        }

        // Renames, moves and creates first, as many as can run.
        if run_ready(&mut world, &mut pending, &mut operations, &occupied, false) {
            continue;
        }

        // None could: a cycle, unless every row in it is stuck. One row of it
        // moves aside under a temporary name, freeing its own.
        let detour = pending.iter().find_map(|step| match *step {
            Step::Relocate(index) if !detoured[index] && blocked_by_a_mover(&world, index) => {
                Some(index)
            },
            _ => None,
        });
        if let Some(index) = detour
            && let Some(holder) = world.holder[index].clone()
        {
            detoured[index] = true;
            let temporary = temporary_name(&world, &holder, &taken);
            let from = world.path(index);
            world.place(index, holder, temporary);
            operations.push(Operation::Rename {
                from,
                to: world.path(index),
            });
            continue;
        }

        // Deletes last: by now nothing still to move is inside them, and a
        // struck row whose name something wants is out of the way.
        if run_ready(&mut world, &mut pending, &mut operations, &occupied, true) {
            continue;
        }

        return Err(pending
            .iter()
            .map(|&step| Refusal {
                row: row_of(step),
                reason: why_stuck(&world, step, &occupied),
            })
            .collect());
    }
    Ok(Plan { operations })
}

/// Every operation the rows ask for, in the order they are preferred:
/// renames and moves, then creates, then deletes, each in row order.
fn steps(world: &mut World<'_>) -> Vec<Step> {
    let rows = world.rows;
    let mut relocations = Vec::new();
    let mut creates = Vec::new();
    let mut deletes = Vec::new();
    for (index, row) in rows.iter().enumerate().skip(1) {
        let Some(parent) = row.parent else {
            continue;
        };
        match (&row.origin, row.deleted) {
            (Some(_), true) => {
                // One inside a folder that is itself deleted goes with the
                // folder; a second delete would fail on a path already gone.
                if !struck_above(world, index) {
                    deletes.push(Step::Delete(index));
                }
            },
            (Some(origin), false) => {
                if world.holder[index] != Some(Holder::Row(parent)) || origin.name != row.name {
                    world.unsettled[index] = true;
                    relocations.push(Step::Relocate(index));
                }
            },
            // A typed row struck through never existed, and is dropped.
            (None, true) => {},
            (None, false) => {
                world.unsettled[index] = true;
                creates.push(Step::Create(index));
            },
        }
    }
    relocations
        .into_iter()
        .chain(creates)
        .chain(deletes)
        .collect()
}

/// Emits every pending step that can run now, deletes or everything else as
/// `deletes` says, keeping the rest; reports whether any ran.
fn run_ready(
    world: &mut World<'_>,
    pending: &mut Vec<Step>,
    operations: &mut Vec<Operation>,
    occupied: &[bool],
    deletes: bool,
) -> bool {
    let before = pending.len();
    let mut waiting = Vec::with_capacity(before);
    for step in pending.drain(..) {
        let eligible = matches!(step, Step::Delete(_)) == deletes;
        if eligible && matches!(ready(world, step, occupied), Ready::Now) {
            operations.push(carry_out(world, step));
        } else {
            waiting.push(step);
        }
    }
    *pending = waiting;
    pending.len() < before
}

/// Whether a row's operation can run now.
enum Ready {
    /// Yes.
    Now,
    /// Not until something else has run.
    Later,
    /// Not ever, and why.
    Never(String),
}

/// Whether `step` can run in `world` as it stands.
fn ready(world: &World<'_>, step: Step, occupied: &[bool]) -> Ready {
    match step {
        Step::Relocate(index) | Step::Create(index) => {
            let Some(folder) = world.rows[index].parent else {
                return Ready::Never(
                    "the folder this row is in could not be worked out".to_owned(),
                );
            };
            if matches!(step, Step::Relocate(_)) && world.within(folder, index) {
                if world.settled(folder) {
                    return Ready::Never("a folder cannot be moved inside itself".to_owned());
                }
                return Ready::Later;
            }
            if !world.settled(folder) {
                return Ready::Later;
            }
            let name = &world.rows[index].name;
            if world.hidden(&Holder::Row(folder), name) {
                return Ready::Never(format!(
                    "{} already exists",
                    world.path(folder).join(name).display()
                ));
            }
            match world.occupant(folder, name) {
                Some(other) if other != index => Ready::Later,
                _ => Ready::Now,
            }
        },
        Step::Delete(index) => {
            if occupied[index] {
                Ready::Later
            } else {
                Ready::Now
            }
        },
    }
}

/// Emits `step`, which is ready, and plays it out.
fn carry_out(world: &mut World<'_>, step: Step) -> Operation {
    match step {
        Step::Relocate(index) => {
            let folder = world.rows[index].parent.unwrap_or(0);
            let same_folder = world.holder[index] == Some(Holder::Row(folder));
            let from = world.path(index);
            world.place(index, Holder::Row(folder), world.rows[index].name.clone());
            world.unsettled[index] = false;
            let to = world.path(index);
            if same_folder {
                Operation::Rename { from, to }
            } else {
                Operation::Move { from, to }
            }
        },
        Step::Create(index) => {
            let folder = world.rows[index].parent.unwrap_or(0);
            world.place(index, Holder::Row(folder), world.rows[index].name.clone());
            world.unsettled[index] = false;
            Operation::Create {
                path: world.path(index),
                directory: world.rows[index].directory,
            }
        },
        Step::Delete(index) => {
            let path = world.path(index);
            world.remove(index);
            Operation::Delete { path }
        },
    }
}

/// Whether a struck row sits inside another struck row, now.
fn struck_above(world: &World<'_>, index: usize) -> bool {
    let mut at = index;
    while let Some(Holder::Row(above)) = &world.holder[at] {
        if world.rows[*above].deleted {
            return true;
        }
        at = *above;
    }
    false
}

/// Whether row `index` waits only for a row that is itself moving away from
/// the name it wants.
fn blocked_by_a_mover(world: &World<'_>, index: usize) -> bool {
    let Some(folder) = world.rows[index].parent else {
        return false;
    };
    if !world.settled(folder) || world.within(folder, index) {
        return false;
    }
    world
        .occupant(folder, &world.rows[index].name)
        .is_some_and(|other| other != index && world.unsettled[other])
}

/// The row a step is for.
const fn row_of(step: Step) -> usize {
    match step {
        Step::Relocate(index) | Step::Create(index) | Step::Delete(index) => index,
    }
}

/// Why `step` never became ready, in a sentence for its row.
fn why_stuck(world: &World<'_>, step: Step, occupied: &[bool]) -> String {
    if let Ready::Never(reason) = ready(world, step, occupied) {
        return reason;
    }
    match step {
        Step::Relocate(index) | Step::Create(index) => {
            let folder = world.rows[index].parent.unwrap_or(0);
            if world.settled(folder) {
                format!(
                    "{} is taken by a row that cannot move out of the way first",
                    world.path(folder).join(&world.rows[index].name).display()
                )
            } else {
                "the folder it goes into could not be put in place first".to_owned()
            }
        },
        Step::Delete(_) => "something inside it could not be moved out first".to_owned(),
    }
}

/// A name inside `holder` that nothing is using: no row's old or new name,
/// nothing there now, and nothing hidden on disk.
fn temporary_name(world: &World<'_>, holder: &Holder, taken: &BTreeSet<&str>) -> String {
    let mut candidate = TEMPORARY.to_owned();
    let mut suffix = 0_u32;
    while taken.contains(candidate.as_str())
        || world
            .contents
            .get(holder)
            .is_some_and(|names| names.contains_key(&candidate))
        || world.hidden(holder, &candidate)
    {
        suffix += 1;
        candidate = format!("{TEMPORARY}-{suffix}");
    }
    candidate
}
