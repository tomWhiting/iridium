//! Re-scan around a single edit, then shift unaffected suffix checkpoints.
use super::{Cancellation, LineIndex, PagedText, Scan};
use crate::paged::{Change, pieces};
use std::{io, sync::Arc};

#[derive(Debug)]
pub(super) struct Repair {
    pub old_join: Scan,
    pub new_offset: u64,
    pub tail: Vec<Scan>,
}
impl LineIndex {
    /// Reuse a completed index after one edit, undo or redo. This binds the
    /// index to the new state and retains its unchanged prefix immediately.
    /// Call `advance` to repair around the edit in bounded blocks; unchanged
    /// suffix checkpoints are reused rather than rescanning the whole file.
    ///
    /// An incomplete index or multiple unobserved changes is refused. Callers
    /// must finish repair before reusing it for another edit, or start a new
    /// index explicitly. No file I/O happens in this method; checkpoint metadata
    /// is copied. Cancellation leaves the old index unchanged.
    pub fn refresh(&mut self, text: &PagedText, cancel: &Cancellation) -> io::Result<()> {
        cancel.check()?;
        if !Arc::ptr_eq(&self.identity, &text.identity) || !self.complete {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refresh requires a completed index of this document",
            ));
        }
        if self.state == text.state {
            return Ok(());
        }
        let (change, reverse) = text
            .undo
            .last()
            .filter(|change| change.before == self.state && change.after == text.state)
            .map(|change| (change, false))
            .or_else(|| {
                text.redo
                    .last()
                    .filter(|change| change.after == self.state && change.before == text.state)
                    .map(|change| (change, true))
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "index missed more than one content change",
                )
            })?;
        let (removed, inserted) = lengths(change, reverse)?;
        let old_end = change
            .at
            .checked_add(removed)
            .ok_or_else(|| io::Error::other("edit endpoint overflow"))?;
        let left = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.offset <= change.at.saturating_sub(1))
            .saturating_sub(1);
        let right = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.offset <= old_end.saturating_add(1))
            .min(self.checkpoints.len() - 1);
        let old_join = *self
            .checkpoints
            .get(right)
            .ok_or_else(|| io::Error::other("missing repair checkpoint"))?;
        let new_offset = shift(old_join.offset, removed, inserted)?;
        let checkpoints = self.checkpoints[..=left].to_vec();
        let frontier = *checkpoints
            .last()
            .ok_or_else(|| io::Error::other("missing repair prefix"))?;
        let repair = Repair {
            old_join,
            new_offset,
            tail: self.checkpoints[right..].to_vec(),
        };
        cancel.check()?;
        self.checkpoints = checkpoints;
        self.frontier = frontier;
        self.state = text.state;
        self.total = text.len();
        self.complete = false;
        self.repair = Some(repair);
        Ok(())
    }
    pub(super) fn repaired_tail(
        &self,
        next: Scan,
        cancel: &Cancellation,
    ) -> io::Result<Option<Vec<Scan>>> {
        let Some(repair) = &self.repair else {
            return Ok(None);
        };
        if next.offset != repair.new_offset {
            return Ok(None);
        }
        let mut tail = Vec::with_capacity(repair.tail.len());
        for &old in &repair.tail {
            cancel.check()?;
            tail.push(Scan {
                offset: shift(old.offset, repair.old_join.offset, next.offset)?,
                line: shift(old.line, repair.old_join.line, next.line)?,
                start: if old.start == repair.old_join.start {
                    next.start
                } else {
                    shift(old.start, repair.old_join.offset, next.offset)?
                },
                pending_cr: old.pending_cr,
            });
        }
        Ok(Some(tail))
    }
}
fn lengths(change: &Change, reverse: bool) -> io::Result<(u64, u64)> {
    let removed = pieces::length(&change.removed)?;
    let inserted = pieces::length(&change.inserted)?;
    Ok(if reverse {
        (inserted, removed)
    } else {
        (removed, inserted)
    })
}
fn shift(value: u64, old: u64, new: u64) -> io::Result<u64> {
    u64::try_from(i128::from(value) + i128::from(new) - i128::from(old)).map_err(io::Error::other)
}
