# Performance Benchmarks

## Running Benchmarks

```sh
just bench
```

Or run benchmarks for a specific crate:

```sh
cargo bench -p codequery-parse
cargo bench -p codequery-index
```

Criterion generates HTML reports in `target/criterion/`. Open `target/criterion/report/index.html` for a summary.

## Performance Targets

From the specification:

| Command type | Target | Context |
|---|---|---|
| Narrow (`def`, `body`, `sig`, `outline`, `imports`, `context`) | < 100ms | Any project size |
| Wide (`refs`, `callers`, `symbols`, `tree`) | < 2s | 400k lines, 8 cores |

## Benchmarks

### codequery-parse (`crates/codequery-parse/benches/parse_bench.rs`)

| Benchmark | What it measures |
|---|---|
| `parse_rust_file` | Parse a single Rust file from disk (fixture lib.rs) |
| `extract_symbols_rust` | Extract symbols from an already-parsed Rust file |
| `search_pattern_rust` | Structural pattern search against a Rust file |
| `search_raw_sexp_rust` | Raw S-expression query against a Rust file |

### codequery-index (`crates/codequery-index/benches/scan_bench.rs`)

| Benchmark | What it measures |
|---|---|
| `scan_project_rust_fixture` | Full project scan of the Rust fixture (discovery + parse + extract) |
| `scan_with_filter_greet` | Filtered scan with grep pre-filter for "greet" |
| `symbol_index_from_scan` | Build a SymbolIndex from pre-computed scan results |

## Baseline Results

Measured on the Rust fixture project (~116 lines across 6 files). These are reference points, not production targets. The spec targets apply to much larger codebases.

Run `just bench` to establish a baseline for your machine. Criterion will track regressions across runs automatically.
