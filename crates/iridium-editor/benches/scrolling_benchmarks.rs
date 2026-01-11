//! Scrolling and viewport performance benchmarks.
//!
//! Benchmarks for scrolling operations on large files (100k+ lines)
//! to ensure 120fps (< 8.33ms per frame) performance.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use iridium_editor::{EditorView, Viewport};

/// Generate test content with the specified number of lines.
fn generate_content(lines: usize) -> String {
    (0..lines)
        .map(|i| {
            if i < lines - 1 {
                format!(
                    "Line {:06}: This is a line of text with some content for testing purposes.\n",
                    i
                )
            } else {
                format!(
                    "Line {:06}: This is a line of text with some content for testing purposes.",
                    i
                )
            }
        })
        .collect()
}

/// Benchmark viewport visible_line_range calculation.
fn viewport_visible_range(c: &mut Criterion) {
    let mut group = c.benchmark_group("viewport_visible_range");

    for lines in [1000, 10_000, 100_000] {
        let mut viewport = Viewport::new(800.0, 600.0);
        viewport.set_font_metrics(8.0, 20.0);
        viewport.set_total_lines(lines);
        viewport.scroll_to_line(lines / 2);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &viewport,
            |b, viewport| {
                b.iter(|| {
                    black_box(viewport.visible_line_range());
                });
            },
        );
    }

    group.finish();
}

/// Benchmark viewport scrolling operations.
fn viewport_scroll(c: &mut Criterion) {
    let mut group = c.benchmark_group("viewport_scroll");

    for lines in [1000, 10_000, 100_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines_scroll_by")),
            &lines,
            |b, &lines| {
                b.iter_batched(
                    || {
                        let mut viewport = Viewport::new(800.0, 600.0);
                        viewport.set_font_metrics(8.0, 20.0);
                        viewport.set_total_lines(lines);
                        viewport
                    },
                    |mut viewport| {
                        viewport.scroll_by(0.0, 100.0);
                        viewport
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines_scroll_to_line")),
            &lines,
            |b, &lines| {
                b.iter_batched(
                    || {
                        let mut viewport = Viewport::new(800.0, 600.0);
                        viewport.set_font_metrics(8.0, 20.0);
                        viewport.set_total_lines(lines);
                        viewport
                    },
                    |mut viewport| {
                        viewport.scroll_to_line(lines / 2);
                        viewport
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark visible_text extraction (viewport culling).
fn visible_text_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("visible_text");

    for lines in [1000, 10_000, 100_000] {
        let content = generate_content(lines);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &content,
            |b, content| {
                b.iter_batched(
                    || {
                        let mut view = EditorView::with_content(800.0, 600.0, content);
                        view.set_font_metrics(8.0, 20.0);
                        view.update();
                        // Scroll to middle of document
                        view.viewport_mut().scroll_to_line(lines / 2);
                        view.update();
                        view
                    },
                    |view| {
                        black_box(view.visible_text());
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark full frame update cycle.
fn frame_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame_update");

    // Target: < 8.33ms for 120fps
    for lines in [1000, 10_000, 100_000] {
        let content = generate_content(lines);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &content,
            |b, content| {
                b.iter_batched(
                    || {
                        let mut view = EditorView::with_content(800.0, 600.0, content);
                        view.set_font_metrics(8.0, 20.0);
                        view
                    },
                    |mut view| {
                        // Simulate frame update
                        view.update();
                        black_box(&view);
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark highlight rectangle generation.
fn highlight_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("highlight_generation");

    for lines in [1000, 10_000, 100_000] {
        let content = generate_content(lines);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &content,
            |b, content| {
                b.iter_batched(
                    || {
                        let mut view = EditorView::with_content(800.0, 600.0, content);
                        view.set_font_metrics(8.0, 20.0);
                        view.viewport_mut().scroll_to_line(lines / 2);
                        view.update();
                        view
                    },
                    |view| {
                        black_box(view.highlight_rects());
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark line cache update.
fn line_cache_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("line_cache_update");

    for lines in [1000, 10_000, 100_000] {
        let content = generate_content(lines);

        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines_scroll_update")),
            &content,
            |b, content| {
                b.iter_batched(
                    || {
                        let mut view = EditorView::with_content(800.0, 600.0, content);
                        view.set_font_metrics(8.0, 20.0);
                        view.update();
                        view
                    },
                    |mut view| {
                        // Scroll and update (triggers line cache update)
                        view.viewport_mut().scroll_by(0.0, 500.0);
                        view.update();
                        view
                    },
                    criterion::BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

/// Benchmark momentum scrolling update.
fn momentum_update(c: &mut Criterion) {
    let mut group = c.benchmark_group("momentum_update");

    for lines in [1000, 10_000, 100_000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{lines}_lines")),
            &lines,
            |b, &lines| {
                b.iter_batched(
                    || {
                        let mut viewport = Viewport::new(800.0, 600.0);
                        viewport.set_font_metrics(8.0, 20.0);
                        viewport.set_total_lines(lines);
                        // Start momentum
                        viewport.handle_trackpad_scroll(0.0, 100.0, false);
                        viewport
                    },
                    |mut viewport| {
                        // Update momentum (simulates frame update)
                        viewport.update_momentum();
                        viewport
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    viewport_visible_range,
    viewport_scroll,
    visible_text_extraction,
    frame_update,
    highlight_generation,
    line_cache_update,
    momentum_update,
);
criterion_main!(benches);
