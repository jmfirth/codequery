//! Criterion benchmarks for codequery-index.
//!
//! Covers project scanning, filtered scanning, and symbol index construction.

use std::path::Path;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use codequery_index::{scan_project, scan_with_filter, SymbolIndex};

/// Path to the Rust fixture project root.
fn fixture_root() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/rust_project"
    ))
}

/// Benchmark: scan the entire Rust fixture project.
fn bench_scan_project(c: &mut Criterion) {
    let root = fixture_root();

    c.bench_function("scan_project_rust_fixture", |b| {
        b.iter(|| {
            let results = scan_project(black_box(root), None).expect("scan");
            black_box(results);
        });
    });
}

/// Benchmark: scan with a symbol name filter.
fn bench_scan_with_filter(c: &mut Criterion) {
    let root = fixture_root();

    c.bench_function("scan_with_filter_greet", |b| {
        b.iter(|| {
            let results =
                scan_with_filter(black_box(root), None, black_box("greet")).expect("scan");
            black_box(results);
        });
    });
}

/// Benchmark: build a SymbolIndex from scan results.
fn bench_symbol_index_from_scan(c: &mut Criterion) {
    let root = fixture_root();
    let scan = scan_project(root, None).expect("scan");

    c.bench_function("symbol_index_from_scan", |b| {
        b.iter(|| {
            let index = SymbolIndex::from_scan(black_box(&scan));
            black_box(index);
        });
    });
}

criterion_group!(
    benches,
    bench_scan_project,
    bench_scan_with_filter,
    bench_symbol_index_from_scan,
);
criterion_main!(benches);
