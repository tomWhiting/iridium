//! Measure progressive indexing and bounded line windows on actual large files.
//! Arguments: byte count, then `jsonl` or `single-line`. Fixtures self-clean.
use iridium_file::paged::{Cancellation, LineIndex, PagedText};
use std::{
    fs::File,
    io::{self, Write},
    time::Instant,
};

const JSONL: &str = "{\"text\":\"🦀e\u{301}\",\"value\":123}\r\n";
const SINGLE: &str = "🦀e\u{301} text ";

fn main() -> io::Result<()> {
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    let [bytes, shape] = arguments.as_slice() else {
        return Err(io::Error::other(
            "supply byte count and jsonl or single-line",
        ));
    };
    let bytes = bytes.parse::<u64>().map_err(io::Error::other)?;
    let pattern = match shape.as_str() {
        "jsonl" => JSONL,
        "single-line" => SINGLE,
        _ => return Err(io::Error::other("shape must be jsonl or single-line")),
    };
    let records = bytes.div_ceil(pattern.len() as u64).max(4);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("fixture.txt");
    let len = write_fixture(&mut File::create(&path)?, pattern, records)?;
    let cancel = Cancellation::default();
    let start = Instant::now();
    let mut text = PagedText::capture(&path, &cancel, |_| {})?;
    let capture_ms = start.elapsed().as_millis();
    let mut index = LineIndex::new(&text);
    let start = Instant::now();
    let mut max_step_us = 0;
    while !index.progress().complete {
        let step = Instant::now();
        index.advance(&mut text, &cancel)?;
        max_step_us = max_step_us.max(step.elapsed().as_micros());
    }
    let index_ms = start.elapsed().as_millis();
    let expected_lines = if shape == "jsonl" { records + 1 } else { 1 };
    if index.progress().known_lines != expected_lines || index.progress().indexed_bytes != len {
        return Err(io::Error::other("index frontier or line count differs"));
    }
    let (median_window_us, max_window_us) =
        measure_windows(&index, &mut text, pattern, records, shape)?;
    let (repair_us, repair_steps, undo_repair_us) = measure_repair(&mut index, &mut text, pattern)?;
    writeln!(
        io::stdout().lock(),
        "{{\"shape\":\"{shape}\",\"bytes\":{len},\"lines\":{expected_lines},\"capture_ms\":{capture_ms},\"index_ms\":{index_ms},\"max_step_us\":{max_step_us},\"median_window_us\":{median_window_us},\"max_window_us\":{max_window_us},\"repair_us\":{repair_us},\"repair_steps\":{repair_steps},\"undo_repair_us\":{undo_repair_us},\"checkpoints\":{},\"window_samples\":61,\"editor_integrated\":false}}",
        index.checkpoint_count()
    )
}

fn write_fixture(file: &mut File, pattern: &str, records: u64) -> io::Result<u64> {
    let len = records
        .checked_mul(pattern.len() as u64)
        .ok_or_else(|| io::Error::other("fixture too large"))?;
    let block = pattern.repeat(65536 / pattern.len());
    let mut remaining = len;
    while remaining > 0 {
        let count = usize::try_from(remaining.min(block.len() as u64)).map_err(io::Error::other)?;
        file.write_all(&block.as_bytes()[..count])?;
        remaining -= count as u64;
    }
    file.sync_all()?;
    Ok(len)
}

fn measure_windows(
    index: &LineIndex,
    text: &mut PagedText,
    pattern: &str,
    records: u64,
    shape: &str,
) -> io::Result<(u128, u128)> {
    let mut samples = Vec::new();
    for sample in 0..61 {
        let record = [0, records / 2, records - 1][sample % 3];
        let (line, column) = if shape == "jsonl" {
            (record, 0)
        } else {
            (0, record * pattern.len() as u64)
        };
        let start = Instant::now();
        let window = index
            .window(text, line, column, pattern.len(), &Cancellation::default())?
            .ok_or_else(|| io::Error::other("known line missing"))?;
        samples.push(start.elapsed().as_micros());
        let expected = pattern.trim_end_matches(['\r', '\n']);
        if window.text != expected || window.bytes.start != record * pattern.len() as u64 {
            return Err(io::Error::other(
                "window differs from source at requested record",
            ));
        }
    }
    samples.sort_unstable();
    let median = samples
        .get(samples.len() / 2)
        .copied()
        .ok_or_else(|| io::Error::other("no samples"))?;
    let maximum = samples
        .last()
        .copied()
        .ok_or_else(|| io::Error::other("no samples"))?;
    Ok((median, maximum))
}

fn measure_repair(
    index: &mut LineIndex,
    text: &mut PagedText,
    pattern: &str,
) -> io::Result<(u128, usize, u128)> {
    let cancel = Cancellation::default();
    let before = index.progress();
    let start = Instant::now();
    text.replace(0..0, "\r\n")?;
    index.refresh(text, &cancel)?;
    let mut steps = 0;
    while !index.progress().complete {
        index.advance(text, &cancel)?;
        steps += 1;
    }
    let repair_us = start.elapsed().as_micros();
    if steps > 2 || index.progress().known_lines != before.known_lines + 1 {
        return Err(io::Error::other(
            "repair did not reuse the unchanged suffix",
        ));
    }
    let first = index
        .window(text, 1, 0, pattern.len(), &cancel)?
        .ok_or_else(|| io::Error::other("first original line missing"))?;
    if first.text != pattern.trim_end_matches(['\r', '\n']) {
        return Err(io::Error::other("repair changed original line window"));
    }
    let start = Instant::now();
    if !text.undo()? {
        return Err(io::Error::other("edit undo missing"));
    }
    index.refresh(text, &cancel)?;
    while !index.progress().complete {
        index.advance(text, &cancel)?;
    }
    let undo_us = start.elapsed().as_micros();
    if index.progress() != before {
        return Err(io::Error::other("undo repair changed index totals"));
    }
    Ok((repair_us, steps, undo_us))
}
