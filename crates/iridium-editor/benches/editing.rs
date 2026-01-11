//! Benchmarks for editing operations.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use iridium_editor::{Document, Position};

fn insert_benchmark(c: &mut Criterion) {
    let mut doc = Document::new("Hello, world!");

    c.bench_function("insert_char", |b| {
        b.iter(|| {
            let result = doc.insert(black_box(Position::new(0, 5)), black_box("x"));
            let _ = black_box(result);
        });
    });
}

fn delete_benchmark(c: &mut Criterion) {
    let mut doc = Document::new("Hello, world! This is a test.");

    c.bench_function("delete_range", |b| {
        b.iter(|| {
            let range = iridium_editor::Range::new(Position::new(0, 5), Position::new(0, 10));
            let _ = doc.delete(black_box(range));
            // Reset for next iteration
            doc = Document::new("Hello, world! This is a test.");
        });
    });
}

criterion_group!(benches, insert_benchmark, delete_benchmark);
criterion_main!(benches);
