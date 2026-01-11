//! Buffer operation benchmarks.
//!
//! Benchmarks for text buffer operations including insert, delete, and
//! navigation on files of various sizes.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use iridium_editor::Buffer;

/// Generate sample text of approximately n characters.
fn generate_text(lines: usize, chars_per_line: usize) -> String {
    let line: String = (0..chars_per_line)
        .map(|i| ((i % 26) as u8 + b'a') as char)
        .collect();
    (0..lines)
        .map(|_| line.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn buffer_from_str(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_from_str");

    for lines in [100, 1000, 10000] {
        let text = generate_text(lines, 80);
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &text,
            |b, text| {
                b.iter(|| Buffer::from(black_box(text.as_str())));
            },
        );
    }

    group.finish();
}

fn buffer_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_insert");

    for lines in [100, 1000, 10000] {
        let text = generate_text(lines, 80);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines_at_end")),
            &text,
            |b, text| {
                b.iter_batched(
                    || Buffer::from(text.as_str()),
                    |mut buffer| {
                        let pos = buffer.len_chars();
                        buffer.insert(pos, "inserted text");
                        buffer
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines_at_middle")),
            &text,
            |b, text| {
                b.iter_batched(
                    || Buffer::from(text.as_str()),
                    |mut buffer| {
                        let pos = buffer.len_chars() / 2;
                        buffer.insert(pos, "inserted text");
                        buffer
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn buffer_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_remove");

    for lines in [100, 1000, 10000] {
        let text = generate_text(lines, 80);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &text,
            |b, text| {
                b.iter_batched(
                    || Buffer::from(text.as_str()),
                    |mut buffer| {
                        let mid = buffer.len_chars() / 2;
                        buffer.remove(mid, mid + 10);
                        buffer
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

fn buffer_line_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_line_access");

    for lines in [100, 1000, 10000] {
        let text = generate_text(lines, 80);
        let buffer = Buffer::from(text.as_str());

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &buffer,
            |b, buffer| {
                b.iter(|| {
                    let line_idx = buffer.len_lines() / 2;
                    black_box(buffer.line(line_idx));
                });
            },
        );
    }

    group.finish();
}

fn buffer_char_to_line(c: &mut Criterion) {
    let mut group = c.benchmark_group("buffer_char_to_line");

    for lines in [100, 1000, 10000] {
        let text = generate_text(lines, 80);
        let buffer = Buffer::from(text.as_str());

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &buffer,
            |b, buffer| {
                b.iter(|| {
                    let char_idx = buffer.len_chars() / 2;
                    black_box(buffer.char_to_line(char_idx));
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    buffer_from_str,
    buffer_insert,
    buffer_remove,
    buffer_line_access,
    buffer_char_to_line,
);
criterion_main!(benches);
