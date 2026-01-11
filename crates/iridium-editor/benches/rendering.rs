//! Benchmarks for rendering operations.

use criterion::{Criterion, criterion_group, criterion_main};

fn render_benchmark(c: &mut Criterion) {
    c.bench_function("render_frame", |b| {
        b.iter(|| {
            // Rendering benchmarks will be added when GPU pipeline is implemented
        });
    });
}

criterion_group!(benches, render_benchmark);
criterion_main!(benches);
