//! Applying one edit to a cache without rescanning the document.
//!
//! Split from the cache itself because the two are different jobs. `mod.rs`
//! holds what a brace-fold cache *is* — the regions, the per-line states, the
//! stacks, and the whole-document scan that fills all three. This holds the one
//! operation that keeps them without redoing them: resume, rescan, converge,
//! and merge. The argument for why the merge is exactly a full rescan's answer
//! is in this module's parent; the arithmetic that carries it out is here.

use super::{
    BraceFoldCache, CHECKPOINT_INTERVAL, Checkpoint, LineEdit, LineState, TrackedBrace,
    has_lone_carriage_return, lines_from, record_checkpoint, scan_line, shift_line, signed,
};

impl BraceFoldCache {
    /// Updates the regions for one edit, rescanning only what the edit reached.
    ///
    /// `source` is the document **after** the edit and `edit` describes how it
    /// got there. Returns whether the published regions changed.
    ///
    /// Falls back to [`BraceFoldCache::rebuild`] whenever the edit cannot be
    /// trusted to describe this cache's document — nothing has been scanned yet,
    /// the offsets do not lie inside `source`, the rows are inconsistent, a lone
    /// `\r` makes the two line numberings disagree, or no recorded stack sits at
    /// or before the first changed line. Every one of those is a slower answer
    /// rather than a different one.
    pub fn update(&mut self, source: &str, edit: &LineEdit) -> bool {
        let Some(rescan) = self.plan(source, edit) else {
            return self.rebuild(source);
        };

        self.lines_scanned += rescan.scanned;

        if self.absorb(rescan) {
            self.publish()
        } else {
            // A retained item could not be moved. The cache is half-written by
            // this point, which costs nothing: a rebuild discards all of it.
            self.rebuild(source)
        }
    }

    /// Rescans from the last recorded stack before the edit until the scan
    /// converges with the previous one, or declines the whole update.
    fn plan(&self, source: &str, edit: &LineEdit) -> Option<Rescan> {
        let resume = self.resume_point(source, edit)?;
        let row_delta = signed(edit.new_end_row)?.checked_sub(signed(edit.old_end_row)?)?;
        let byte_delta = signed(edit.new_end_byte)?.checked_sub(signed(edit.old_end_byte)?)?;

        let mut stack = resume.stack;
        let mut state = resume.state;
        let mut rescan = Rescan {
            scan_from: resume.line,
            states: Vec::new(),
            tracked: Vec::new(),
            checkpoints: Vec::new(),
            stack: Vec::new(),
            converged: None,
            scanned: 0,
        };
        let mut last_checkpoint = None;

        for (line_number, (offset, line)) in (resume.line..).zip(lines_from(source, resume.byte)) {
            // Convergence is only meaningful once the remaining text is known to
            // match the old document's: up to and including the row the new text
            // ends on, it does not.
            if line_number > edit.new_end_row {
                if let Some(old_line) = shift_line(line_number, -row_delta) {
                    if self.states.get(old_line) == Some(&state) {
                        rescan.converged = Some(Converged {
                            old_line,
                            row_delta,
                            byte_delta,
                        });
                        break;
                    }
                }
            }

            rescan.states.push(state);
            record_checkpoint(
                &mut rescan.checkpoints,
                &mut last_checkpoint,
                line_number,
                offset,
                &stack,
            );
            scan_line(
                line,
                line_number,
                &mut state,
                &mut stack,
                &mut rescan.tracked,
            );
            rescan.scanned += 1;
        }

        if rescan.converged.is_none() {
            // The scan reached the end of the document, so this is the state a
            // scan of the line after the last one would start from — the extra
            // entry `states` always carries.
            rescan.states.push(state);
        }

        rescan.stack = stack;
        Some(rescan)
    }

    /// Finds the line a rescan for `edit` can resume from, and the state there.
    ///
    /// Returns `None` for every reason an incremental update must not be
    /// attempted, which the caller answers with a full rebuild.
    fn resume_point(&self, source: &str, edit: &LineEdit) -> Option<Resume> {
        if !self.primed || self.lone_carriage_return {
            return None;
        }

        let bytes = source.as_bytes();
        if edit.start_byte > edit.new_end_byte || edit.new_end_byte > bytes.len() {
            return None;
        }
        let rows_describe_one_edit =
            edit.start_row <= edit.old_end_row && edit.start_row <= edit.new_end_row;
        if !rows_describe_one_edit {
            return None;
        }
        if edit.old_end_row > self.line_count {
            return None;
        }

        // A `\r` whose follower the edit changed, and one the edit inserted, are
        // the only ways a document that had none can acquire one — and every
        // such `\r` sits in this window.
        if has_lone_carriage_return(bytes, edit.start_byte.saturating_sub(1)..edit.new_end_byte) {
            return None;
        }

        let index = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.line <= edit.start_row)
            .checked_sub(1)?;
        let checkpoint = self.checkpoints.get(index)?;
        if !source.is_char_boundary(checkpoint.byte) {
            return None;
        }

        Some(Resume {
            line: checkpoint.line,
            byte: checkpoint.byte,
            stack: checkpoint.stack.clone(),
            state: *self.states.get(checkpoint.line)?,
        })
    }

    /// Merges a rescan with the parts of the document it did not touch.
    ///
    /// Returns false when a retained pair or stack cannot be moved into post-edit
    /// coordinates, which only a full rebuild can answer.
    fn absorb(&mut self, rescan: Rescan) -> bool {
        let Rescan {
            scan_from,
            states,
            tracked,
            checkpoints,
            stack,
            converged,
            ..
        } = rescan;

        // Everything closing before the resume point is untouched: it opened and
        // closed in text the edit did not reach.
        let kept = self
            .tracked
            .partition_point(|brace| brace.end_line < scan_from);
        let (tracked_end, states_end) = match converged {
            Some(converged) => {
                let suffix = self
                    .tracked
                    .partition_point(|brace| brace.end_line < converged.old_line);
                for brace in &mut self.tracked[suffix..] {
                    let Some(moved) = shift_brace(brace, &converged, &stack) else {
                        return false;
                    };
                    *brace = moved;
                }
                (suffix, converged.old_line)
            },
            None => (self.tracked.len(), self.states.len()),
        };

        let kept_checkpoints = self
            .checkpoints
            .partition_point(|checkpoint| checkpoint.line < scan_from);
        let mut merged = self.checkpoints[..kept_checkpoints].to_vec();
        merged.extend(checkpoints);
        if let Some(converged) = converged {
            if !append_moved_checkpoints(&mut merged, &self.checkpoints, &converged, &stack) {
                return false;
            }
        }
        self.checkpoints = merged;

        self.tracked.splice(kept..tracked_end, tracked);
        self.states.splice(scan_from..states_end, states);
        self.line_count = self.states.len().saturating_sub(1);
        true
    }
}

/// Where a rescan starts, and everything it needs to start there.
struct Resume {
    /// The line the scan resumes at.
    line: usize,
    /// The byte offset that line begins at, valid in both documents.
    byte: usize,
    /// The open-brace stack at that line.
    stack: Vec<usize>,
    /// The scanner state at that line.
    state: LineState,
}

/// Everything a rescan produced, before it is merged with what was kept.
struct Rescan {
    /// The line the rescan resumed from.
    scan_from: usize,
    /// States for the lines the rescan covered.
    states: Vec<LineState>,
    /// Pairs the rescan closed.
    tracked: Vec<TrackedBrace>,
    /// Stacks the rescan recorded.
    checkpoints: Vec<Checkpoint>,
    /// The open-brace stack where the rescan stopped.
    stack: Vec<usize>,
    /// Where the rescan met the previous scan, if it did.
    converged: Option<Converged>,
    /// How many lines the rescan read.
    scanned: u64,
}

/// Where a rescan met the previous scan, and by how much the document moved.
#[derive(Debug, Clone, Copy)]
struct Converged {
    /// The pre-edit line whose state the rescan matched.
    old_line: usize,
    /// Rows the edit added, or removed if negative.
    row_delta: isize,
    /// Bytes the edit added, or removed if negative.
    byte_delta: isize,
}

/// Moves one retained pair into post-edit coordinates.
///
/// The closing line is past the convergence point, so it moves by the edit's row
/// delta. The opening line either does too, or belongs to a brace already open at
/// the convergence point — in which case its new line is read out of the live
/// stack at the depth the pair recorded, because no arithmetic can describe a
/// line the edit ran through.
fn shift_brace(
    brace: &TrackedBrace,
    converged: &Converged,
    stack: &[usize],
) -> Option<TrackedBrace> {
    let open_line = if brace.open_line >= converged.old_line {
        shift_line(brace.open_line, converged.row_delta)?
    } else {
        *stack.get(brace.open_depth)?
    };

    Some(TrackedBrace {
        open_line,
        open_depth: brace.open_depth,
        end_line: shift_line(brace.end_line, converged.row_delta)?,
    })
}

/// Appends the stacks recorded past the convergence point, moved.
///
/// Returns false if one of them cannot be moved, which only a full rebuild can
/// answer. A moved stack closer than [`CHECKPOINT_INTERVAL`] to the one before it
/// is dropped, so that the join between regenerated and retained stacks cannot
/// leave them packed tighter than the interval — which, over many edits, is how
/// a fixed-size index turns into an unbounded one.
fn append_moved_checkpoints(
    into: &mut Vec<Checkpoint>,
    previous: &[Checkpoint],
    converged: &Converged,
    stack: &[usize],
) -> bool {
    let suffix = previous.partition_point(|checkpoint| checkpoint.line < converged.old_line);
    for checkpoint in &previous[suffix..] {
        let Some(moved) = shift_checkpoint(checkpoint, converged, stack) else {
            return false;
        };
        if into
            .last()
            .is_some_and(|last| moved.line < last.line + CHECKPOINT_INTERVAL)
        {
            continue;
        }
        into.push(moved);
    }

    true
}

/// Moves one retained stack into post-edit coordinates.
///
/// Its entries divide the way a retained pair's opening line does: those
/// recorded at or after the convergence point moved with the document, and those
/// before it name braces open at the convergence point, whose new lines the live
/// stack holds at the same depths.
fn shift_checkpoint(
    checkpoint: &Checkpoint,
    converged: &Converged,
    stack: &[usize],
) -> Option<Checkpoint> {
    let mut shifted = Vec::with_capacity(checkpoint.stack.len());
    for (depth, open_line) in checkpoint.stack.iter().enumerate() {
        shifted.push(if *open_line >= converged.old_line {
            shift_line(*open_line, converged.row_delta)?
        } else {
            *stack.get(depth)?
        });
    }

    Some(Checkpoint {
        line: shift_line(checkpoint.line, converged.row_delta)?,
        byte: shift_line(checkpoint.byte, converged.byte_delta)?,
        stack: shifted,
    })
}
