//! Project policy: ignore files, no symlinks, current buffers replace disk.
use crate::{
    Buffer, Event, MATCH_LIMIT, Origin, SNAPSHOT_LIMIT,
    scan::{self, Budget},
};
use ignore::{WalkBuilder, overrides::Override};
use regex::Regex;
use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
};

pub fn search(
    root: &Path,
    regex: &Regex,
    overrides: &Override,
    buffers: Vec<Buffer>,
    sender: &SyncSender<Event>,
    cancel: &AtomicBool,
) {
    let mut budget = Budget {
        matches: 0,
        bytes: 0,
    };
    let mut buffers: HashMap<_, _> = buffers.into_iter().map(|b| (b.path.clone(), b)).collect();
    let walker = WalkBuilder::new(root)
        .hidden(true)
        .follow_links(false)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(true)
        .parents(false)
        .require_git(false)
        .overrides(overrides.clone())
        .build();
    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                if sender.send(Event::Warning(error.to_string())).is_err() {
                    return;
                }
                continue;
            },
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let path = entry.into_path();
        let outcome = match buffers.remove(&path) {
            Some(buffer) => scan_buffer(buffer, regex, sender, cancel, &mut budget),
            None => scan_disk(&path, regex, sender, cancel, &mut budget),
        };
        if !report(&path, outcome, sender, cancel) {
            return;
        }
        if limited(&budget) {
            break;
        }
    }
    // New/ignored open buffers are explicit user work; include unless excluded
    // by project root or query globs. Named buffers outside root are not searched.
    for (path, buffer) in buffers {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if limited(&budget) {
            break;
        }
        if path.starts_with(root) && !overrides.matched(&path, false).is_ignore() {
            let outcome = scan_buffer(buffer, regex, sender, cancel, &mut budget);
            if !report(&path, outcome, sender, cancel) {
                return;
            }
        }
    }
    if limited(&budget)
        && sender
            .send(Event::Warning(
                "result limit reached; search is incomplete; narrow the query".into(),
            ))
            .is_err()
    {
        return;
    }
    if sender.send(Event::Finished).is_err() {
        cancel.store(true, Ordering::Relaxed);
    }
}
const fn limited(budget: &Budget) -> bool {
    budget.matches >= MATCH_LIMIT || budget.bytes > SNAPSHOT_LIMIT
}
fn scan_buffer(
    buffer: Buffer,
    regex: &Regex,
    sender: &SyncSender<Event>,
    cancel: &AtomicBool,
    budget: &mut Budget,
) -> Result<(), String> {
    // BufReader caps fill_buf even for an already captured large string.
    scan::scan(
        &mut BufReader::with_capacity(64 * 1024, buffer.text.as_bytes()),
        &Arc::new(buffer.path),
        &Origin::Buffer {
            id: buffer.id,
            revision: buffer.revision,
        },
        regex,
        sender,
        cancel,
        budget,
    )
}
fn scan_disk(
    path: &Path,
    regex: &Regex,
    sender: &SyncSender<Event>,
    cancel: &AtomicBool,
    budget: &mut Budget,
) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let before = file.metadata().map_err(|e| e.to_string())?;
    let stamp = crate::DiskStamp::from_metadata(&before).map_err(|e| e.to_string())?;
    let mut reader = BufReader::with_capacity(64 * 1024, file);
    scan::scan(
        &mut reader,
        &Arc::new(path.to_owned()),
        &Origin::Disk(stamp.clone()),
        regex,
        sender,
        cancel,
        budget,
    )?;
    if !stamp.agrees(path).map_err(|e| e.to_string())? {
        return Err("file changed during search; rerun query".into());
    }
    Ok(())
}
fn report(
    path: &Path,
    result: Result<(), String>,
    sender: &SyncSender<Event>,
    cancel: &AtomicBool,
) -> bool {
    if cancel.load(Ordering::Relaxed) {
        return false;
    }
    match result {
        Ok(()) => true,
        Err(reason) if reason.starts_with("project ") => {
            sender.send(Event::Warning(reason)).is_ok()
        },
        Err(reason) => sender
            .send(Event::Invalidated {
                path: PathBuf::from(path),
                reason,
            })
            .is_ok(),
    }
}
