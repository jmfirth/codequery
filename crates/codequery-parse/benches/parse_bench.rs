//! Criterion benchmarks for codequery-parse.
//!
//! Covers single-file parsing, symbol extraction, and S-expression search.

use std::path::Path;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use codequery_core::Language;
use codequery_parse::{extract_symbols, search_file, Parser};

/// Path to the Rust fixture project's lib.rs.
fn fixture_path() -> &'static Path {
    Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/rust_project/src/lib.rs"
    ))
}

/// Benchmark: parse a single Rust file from disk.
fn bench_parse_rust_file(c: &mut Criterion) {
    let path = fixture_path();

    c.bench_function("parse_rust_file", |b| {
        b.iter(|| {
            let mut parser = Parser::for_language(Language::Rust).expect("rust parser");
            let (_source, tree) = parser.parse_file(black_box(path)).expect("parse");
            black_box(tree);
        });
    });
}

/// Benchmark: extract symbols from an already-parsed Rust file.
fn bench_extract_symbols_rust(c: &mut Criterion) {
    let path = fixture_path();
    let mut parser = Parser::for_language(Language::Rust).expect("rust parser");
    let (source, tree) = parser.parse_file(path).expect("parse");

    c.bench_function("extract_symbols_rust", |b| {
        b.iter(|| {
            let symbols = extract_symbols(
                black_box(&source),
                black_box(&tree),
                black_box(path),
                Language::Rust,
            );
            black_box(symbols);
        });
    });
}

/// Benchmark: S-expression query search against a Rust file.
fn bench_search_sexp_rust(c: &mut Criterion) {
    let path = fixture_path();
    let mut parser = Parser::for_language(Language::Rust).expect("rust parser");
    let (source, tree) = parser.parse_file(path).expect("parse");

    c.bench_function("search_sexp_rust", |b| {
        b.iter(|| {
            let matches = search_file(
                black_box("(function_item name: (identifier) @name)"),
                black_box(&source),
                black_box(&tree),
                black_box(path),
            )
            .expect("search");
            black_box(matches);
        });
    });
}

criterion_group!(
    benches,
    bench_parse_rust_file,
    bench_extract_symbols_rust,
    bench_search_sexp_rust,
);
criterion_main!(benches);
