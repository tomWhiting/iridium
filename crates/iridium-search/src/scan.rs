//! Bounded logical-line scanner with explicit binary/UTF-8/long-line failures.
use crate::{Event, LINE_LIMIT, MATCH_LIMIT, Match, Origin, SNAPSHOT_LIMIT};
use regex::Regex;
use std::{
    io::{self, BufRead},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::SyncSender,
    },
};

pub struct Budget {
    pub matches: usize,
    pub bytes: usize,
}
pub fn scan(
    reader: &mut impl BufRead,
    path: &Arc<PathBuf>,
    origin: &Origin,
    regex: &Regex,
    sender: &SyncSender<Event>,
    cancel: &AtomicBool,
    budget: &mut Budget,
) -> Result<(), String> {
    let mut line = Vec::new();
    let mut number = 0usize;
    let mut trailing_empty = true;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let ending = read_line(reader, &mut line, cancel).map_err(|e| e.to_string())?;
        if ending.is_none() && !trailing_empty {
            return Ok(());
        }
        trailing_empty = ending == Some(true);
        if line.contains(&0) {
            return Err("binary file (NUL byte)".into());
        }
        let mut text = std::str::from_utf8(&line).map_err(|e| format!("invalid UTF-8: {e}"))?;
        if number == 0 {
            text = text.strip_prefix('\u{feff}').unwrap_or(text);
        }
        let mut shared = None;
        let mut column = 0usize;
        let mut previous_start = 0usize;
        for found in regex.find_iter(text) {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            if budget.matches >= MATCH_LIMIT {
                return Err("project search reached 10,000 matches; narrow the query".into());
            }
            if shared.is_none() {
                budget.bytes += text.len();
                if budget.bytes > SNAPSHOT_LIMIT {
                    return Err("project results reached 32 MiB; narrow the query".into());
                }
                shared = Some(Arc::<str>::from(text));
            }
            let Some(line_text) = shared.as_ref() else {
                return Err("missing source line".into());
            };
            column += text[previous_start..found.start()].chars().count();
            previous_start = found.start();
            let result = Match {
                path: path.clone(),
                origin: origin.clone(),
                line: number,
                column,
                range: found.range(),
                line_text: line_text.clone(),
            };
            if sender.send(Event::Match(result)).is_err() {
                return Err("cancelled".into());
            }
            budget.matches += 1;
        }
        if ending.is_none() {
            return Ok(());
        }
        number = number.checked_add(1).ok_or("line count overflow")?;
    }
}
fn read_line(
    reader: &mut impl BufRead,
    line: &mut Vec<u8>,
    cancel: &AtomicBool,
) -> io::Result<Option<bool>> {
    line.clear();
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "cancelled"));
        }
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            return Ok((!line.is_empty()).then_some(false));
        }
        let ending = bytes.iter().position(|b| matches!(b, b'\r' | b'\n'));
        let count = ending.unwrap_or(bytes.len());
        if line.len() + count > LINE_LIMIT {
            return Err(io::Error::other("line exceeds 1 MiB search limit"));
        }
        line.extend_from_slice(&bytes[..count]);
        let cr = ending.is_some_and(|index| bytes[index] == b'\r');
        reader.consume(count + usize::from(ending.is_some()));
        if ending.is_some() {
            if cr && reader.fill_buf()?.first() == Some(&b'\n') {
                reader.consume(1);
            }
            return Ok(Some(true));
        }
    }
}
