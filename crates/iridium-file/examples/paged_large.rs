//! Reproducible disk-backed storage exercise; this does not launch the editor.
//! Run with a byte count, e.g. `cargo run -p iridium-file --release --example
//! paged_large -- 2147549184`. All fixture files are removed on exit.
use iridium_file::paged::{Cancellation, PagedText};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::time::Instant;

const BLOCK: usize = 64 * 1024;
const RECORD: &[u8] = "{\"message\":\"🦀e\u{301}\",\"ok\":true}\r\n".as_bytes();

fn main() -> io::Result<()> {
    let requested = std::env::args()
        .nth(1)
        .ok_or_else(|| io::Error::other("supply fixture byte count"))?
        .parse::<u64>()
        .map_err(io::Error::other)?;
    let record_len = RECORD.len() as u64;
    let records = requested.div_ceil(record_len).max(4);
    let len = records
        .checked_mul(record_len)
        .ok_or_else(|| io::Error::other("fixture too large"))?;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("fixture.jsonl");
    let mut source = File::create(&path)?;
    let block = RECORD.repeat(BLOCK / RECORD.len());
    let mut remaining = len;
    while remaining > 0 {
        let count = usize::try_from(remaining.min(block.len() as u64)).map_err(io::Error::other)?;
        source.write_all(&block[..count])?;
        remaining -= count as u64;
    }
    source.sync_all()?;
    drop(source);
    let cancel = Cancellation::default();
    let start = Instant::now();
    let mut text = PagedText::capture(&path, &cancel, |_| {})?;
    let capture_ms = start.elapsed().as_millis();
    let start = Instant::now();
    let points = [0, (records / 2) * record_len, (records - 1) * record_len];
    let mut edits = Vec::new();
    for point in points {
        // Replace the first ASCII character of a key with one ASCII character.
        let offset = point + 2;
        text.replace(offset..offset + 1, "M")?;
        edits.push(offset);
        if text.read(point..point + record_len)?
            != String::from_utf8(RECORD.to_vec())
                .map_err(io::Error::other)?
                .replacen("message", "Message", 1)
        {
            return Err(io::Error::other("edited window differs"));
        }
    }
    let edit_us = start.elapsed().as_micros();
    for point in points.into_iter().rev() {
        if !text.undo()? || text.read(point..point + record_len)?.as_bytes() != RECORD {
            return Err(io::Error::other("undo did not restore original record"));
        }
    }
    if !text.is_at_save_point() {
        return Err(io::Error::other("undo did not restore save point"));
    }
    for _ in points {
        if !text.redo()? {
            return Err(io::Error::other("missing redo"));
        }
    }
    let output_path = dir.path().join("output.jsonl");
    let mut output = File::create(&output_path)?;
    let start = Instant::now();
    text.write_to(&mut output, &cancel)?;
    output.sync_all()?;
    let write_ms = start.elapsed().as_millis();
    drop(output);
    // Check every emitted byte against the repeating fixture plus exactly three edits.
    let start = Instant::now();
    let mut output = File::open(output_path)?;
    let mut buf = vec![0; BLOCK];
    let mut offset = 0_u64;
    loop {
        let count = output.read(&mut buf)?;
        if count == 0 {
            break;
        }
        for (index, &byte) in buf[..count].iter().enumerate() {
            let at = offset + index as u64;
            let expected = if edits.contains(&at) {
                b'M'
            } else {
                RECORD[usize::try_from(at % record_len).map_err(io::Error::other)?]
            };
            if byte != expected {
                return Err(io::Error::other(format!("output mismatch at {at}")));
            }
        }
        offset += count as u64;
    }
    if offset != len {
        return Err(io::Error::other("output length differs"));
    }
    // Confirm publication was never performed over the original fixture.
    let mut original = File::open(path)?;
    for at in edits {
        original.seek(SeekFrom::Start(at))?;
        let mut byte = [0];
        original.read_exact(&mut byte)?;
        if byte != *b"m" {
            return Err(io::Error::other("original was modified"));
        }
    }
    let verify_ms = start.elapsed().as_millis();
    writeln!(
        io::stdout().lock(),
        "{{\"bytes\":{len},\"capture_ms\":{capture_ms},\"three_edits_us\":{edit_us},\"stream_and_sync_ms\":{write_ms},\"verify_ms\":{verify_ms},\"pieces\":{},\"verified_every_output_byte\":true,\"editor_integrated\":false}}",
        text.piece_count()
    )
}
