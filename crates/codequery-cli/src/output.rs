//! Output formatting for cq commands — framed, JSON, and raw modes.
//!
//! This module turns `Symbol` data from codequery-core into the three output
//! formats defined in SPECIFICATION.md section 9. It is pure formatting:
//! no I/O, no parsing, only string construction from typed symbol data.

use codequery_core::{Completeness, QueryResult, Reference, Resolution, Symbol};
use codequery_index::FileSymbols;
use codequery_parse::ImportInfo;
use serde::Serialize;
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::Path;

use crate::args::OutputMode;

// ---------------------------------------------------------------------------
// JSON data structures
// ---------------------------------------------------------------------------

/// JSON payload for the `def` command.
#[derive(Debug, Serialize)]
pub struct DefResults {
    /// The symbol name that was searched for.
    pub symbol: String,
    /// Matching definitions.
    pub definitions: Vec<Symbol>,
    /// Total number of matches.
    pub total: usize,
}

/// JSON payload for the `body` command.
#[derive(Debug, Serialize)]
pub struct BodyResults {
    /// The symbol name that was searched for.
    pub symbol: String,
    /// Matching definitions with body text.
    pub definitions: Vec<Symbol>,
    /// Total number of matches.
    pub total: usize,
}

/// JSON payload for the `sig` command.
#[derive(Debug, Serialize)]
pub struct SigResults {
    /// The symbol name that was searched for.
    pub symbol: String,
    /// Matching definitions with signatures.
    pub signatures: Vec<Symbol>,
    /// Total number of matches.
    pub total: usize,
}

/// JSON payload for the `outline` command.
#[derive(Debug, Serialize)]
pub struct OutlineResult {
    /// The file that was outlined.
    pub file: String,
    /// Top-level symbols in the file.
    pub symbols: Vec<Symbol>,
}

/// JSON payload for the `imports` command.
#[derive(Debug, Serialize)]
pub struct ImportsResult {
    /// The file that was analyzed.
    pub file: String,
    /// Import declarations found in the file.
    pub imports: Vec<ImportInfo>,
    /// Total number of imports.
    pub total: usize,
}

/// JSON payload for the `context` command.
#[derive(Debug, Serialize)]
pub struct ContextResult {
    /// The file being queried.
    pub file: String,
    /// The target line number.
    pub target_line: usize,
    /// The enclosing symbol, if found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<Symbol>,
}

/// JSON payload for the `refs` command.
#[derive(Debug, Serialize)]
pub struct RefsResult {
    /// The symbol name that was searched for.
    pub symbol: String,
    /// Definition locations for the symbol.
    pub definitions: Vec<Symbol>,
    /// All references found.
    pub references: Vec<Reference>,
    /// Total number of references.
    pub total: usize,
}

/// JSON payload for the `callers` command.
#[derive(Debug, Serialize)]
pub struct CallersResult {
    /// The symbol name that was searched for.
    pub symbol: String,
    /// Definition locations for the symbol.
    pub definitions: Vec<Symbol>,
    /// All call-site references found.
    pub callers: Vec<Reference>,
    /// Total number of callers.
    pub total: usize,
}

/// JSON payload for a single file in the `tree` command.
#[derive(Debug, Serialize)]
pub struct TreeFileEntry {
    /// Relative file path.
    pub file: String,
    /// Symbols in this file.
    pub symbols: Vec<Symbol>,
}

/// JSON payload for the `tree` command.
#[derive(Debug, Serialize)]
pub struct TreeResult {
    /// Optional scope path that was used to filter files.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Files with their symbols.
    pub files: Vec<TreeFileEntry>,
    /// Total number of files.
    pub total_files: usize,
    /// Total number of symbols across all files.
    pub total_symbols: usize,
}

/// A single dependency of a symbol.
#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    /// The name of the referenced symbol.
    pub name: String,
    /// The kind of reference (`call`, `type_reference`, `import`, `assignment`).
    pub kind: String,
    /// The file where the dependency is defined, or None if unresolvable.
    pub defined_in: Option<String>,
    /// How the dependency was resolved (resolved via stack graph, or syntactic fallback).
    pub resolution: Resolution,
}

/// JSON payload for the `deps` command.
#[derive(Debug, Serialize)]
pub struct DepsResult {
    /// The symbol being analyzed.
    pub symbol: String,
    /// The symbol definition info, if found.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<Symbol>,
    /// Dependencies found in the symbol body.
    pub dependencies: Vec<Dependency>,
    /// Total number of dependencies.
    pub total: usize,
}

// ---------------------------------------------------------------------------
// Def formatting
// ---------------------------------------------------------------------------

/// Format `def` results in the requested mode.
pub fn format_def(symbols: &[Symbol], symbol_name: &str, mode: OutputMode, pretty: bool) -> String {
    match mode {
        OutputMode::Framed => format_def_results(symbols),
        OutputMode::Json => format_def_json(symbols, symbol_name, pretty),
        OutputMode::Raw => format_def_raw(symbols),
    }
}

/// Format symbol definitions for the `def` command — framed output.
///
/// Each symbol produces one frame header line: `@@ file:line:column kind name @@`
/// Multiple results are separated by blank lines.
/// Returns empty string if symbols is empty.
pub fn format_def_results(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        output.push_str(&format_frame_header(symbol));
    }
    output
}

/// Format `def` results as JSON wrapped in `QueryResult`.
fn format_def_json(symbols: &[Symbol], symbol_name: &str, force_pretty: bool) -> String {
    let data = DefResults {
        symbol: symbol_name.to_string(),
        definitions: symbols.to_vec(),
        total: symbols.len(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `def` results as raw text (no `@@` delimiters).
fn format_def_raw(symbols: &[Symbol]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let _ = write!(
            output,
            "{}:{}:{} {} {}",
            symbol.file.display(),
            symbol.line,
            symbol.column,
            symbol.kind,
            symbol.name,
        );
    }
    output
}

// ---------------------------------------------------------------------------
// Body formatting
// ---------------------------------------------------------------------------

/// Format `body` results in the requested mode.
pub fn format_body(
    symbols: &[Symbol],
    symbol_name: &str,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_body_framed(symbols),
        OutputMode::Json => format_body_json(symbols, symbol_name, pretty),
        OutputMode::Raw => format_body_raw(symbols),
    }
}

/// Format symbol bodies — framed output.
///
/// Each symbol produces a frame header followed by its body text.
/// Multiple results are separated by blank lines.
fn format_body_framed(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        output.push_str(&format_frame_header(symbol));
        if let Some(body) = &symbol.body {
            output.push('\n');
            output.push_str(body);
        }
    }
    output
}

/// Format `body` results as JSON wrapped in `QueryResult`.
fn format_body_json(symbols: &[Symbol], symbol_name: &str, force_pretty: bool) -> String {
    let data = BodyResults {
        symbol: symbol_name.to_string(),
        definitions: symbols.to_vec(),
        total: symbols.len(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `body` results as raw text — body text only, no framing.
fn format_body_raw(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        if let Some(body) = &symbol.body {
            output.push_str(body);
        }
    }
    output
}

// ---------------------------------------------------------------------------
// Sig formatting
// ---------------------------------------------------------------------------

/// Format `sig` results in the requested mode.
pub fn format_sig(symbols: &[Symbol], symbol_name: &str, mode: OutputMode, pretty: bool) -> String {
    match mode {
        OutputMode::Framed => format_sig_framed(symbols),
        OutputMode::Json => format_sig_json(symbols, symbol_name, pretty),
        OutputMode::Raw => format_sig_raw(symbols),
    }
}

/// Format symbol signatures — framed output.
///
/// Each symbol produces a frame header followed by its signature text.
/// Multiple results are separated by blank lines.
fn format_sig_framed(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        output.push_str(&format_frame_header(symbol));
        if let Some(ref sig) = symbol.signature {
            output.push('\n');
            output.push_str(sig);
        }
    }
    output
}

/// Format `sig` results as JSON wrapped in `QueryResult`.
fn format_sig_json(symbols: &[Symbol], symbol_name: &str, force_pretty: bool) -> String {
    let data = SigResults {
        symbol: symbol_name.to_string(),
        signatures: symbols.to_vec(),
        total: symbols.len(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `sig` results as raw text — just the signature, no framing.
fn format_sig_raw(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push_str("\n\n");
        }
        if let Some(ref sig) = symbol.signature {
            output.push_str(sig);
        }
    }
    output
}

// ---------------------------------------------------------------------------
// Outline formatting
// ---------------------------------------------------------------------------

/// Format `outline` results in the requested mode.
pub fn format_outline_output(
    file: &Path,
    symbols: &[Symbol],
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_outline(file, symbols),
        OutputMode::Json => format_outline_json(file, symbols, pretty),
        OutputMode::Raw => format_outline_raw(symbols),
    }
}

/// Format a file's symbol outline — framed output.
///
/// Produces a file-level header followed by an indented symbol list
/// with nesting for children (e.g., methods inside impl blocks).
pub fn format_outline(file: &Path, symbols: &[Symbol]) -> String {
    let mut output = format!("@@ {} @@", file.display());
    for symbol in symbols {
        output.push('\n');
        format_outline_symbol(symbol, 1, &mut output);
    }
    output
}

/// Format `outline` results as JSON wrapped in `QueryResult`.
fn format_outline_json(file: &Path, symbols: &[Symbol], force_pretty: bool) -> String {
    let data = OutlineResult {
        file: file.display().to_string(),
        symbols: symbols.to_vec(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `outline` results as raw text (no `@@` header).
fn format_outline_raw(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        format_outline_symbol(symbol, 0, &mut output);
    }
    output
}

// ---------------------------------------------------------------------------
// Imports formatting
// ---------------------------------------------------------------------------

/// Format `imports` results in the requested mode.
pub fn format_imports_output(
    file: &Path,
    imports: &[ImportInfo],
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_imports_framed(file, imports),
        OutputMode::Json => format_imports_json(file, imports, pretty),
        OutputMode::Raw => format_imports_raw(imports),
    }
}

/// Format imports as framed output: `@@ file:line import source @@`.
fn format_imports_framed(file: &Path, imports: &[ImportInfo]) -> String {
    use std::fmt::Write;
    let mut output = format!("@@ {} @@", file.display());
    for import in imports {
        output.push('\n');
        let _ = write!(
            output,
            "  @@ {}:{} {} {} @@",
            file.display(),
            import.line,
            import.kind,
            import.source,
        );
    }
    output
}

/// Format `imports` results as JSON wrapped in `QueryResult`.
fn format_imports_json(file: &Path, imports: &[ImportInfo], force_pretty: bool) -> String {
    let data = ImportsResult {
        file: file.display().to_string(),
        imports: imports.to_vec(),
        total: imports.len(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `imports` results as raw text (no `@@` delimiters).
fn format_imports_raw(imports: &[ImportInfo]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (i, import) in imports.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let _ = write!(output, ":{} {} {}", import.line, import.kind, import.source);
    }
    output
}

// ---------------------------------------------------------------------------
// Context formatting
// ---------------------------------------------------------------------------

/// Format `context` results in the requested mode.
pub fn format_context_output(
    symbol: Option<&Symbol>,
    target_line: usize,
    file: &Path,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_context_framed(symbol, target_line, file),
        OutputMode::Json => format_context_json(symbol, target_line, file, pretty),
        OutputMode::Raw => format_context_raw(symbol, target_line),
    }
}

/// Format context result as framed output.
fn format_context_framed(symbol: Option<&Symbol>, target_line: usize, file: &Path) -> String {
    let Some(sym) = symbol else {
        return format!(
            "@@ {}:{} (no enclosing symbol) @@",
            file.display(),
            target_line,
        );
    };

    let header = format!(
        "@@ {}:{}:{} {} {} (contains line {}) @@",
        sym.file.display(),
        sym.line,
        sym.column,
        sym.kind,
        sym.name,
        target_line,
    );

    if let Some(body) = &sym.body {
        let body_with_marker = insert_line_marker(body, sym.line, target_line);
        format!("{header}\n{body_with_marker}")
    } else {
        header
    }
}

/// Format context result as JSON wrapped in `QueryResult`.
fn format_context_json(
    symbol: Option<&Symbol>,
    target_line: usize,
    file: &Path,
    force_pretty: bool,
) -> String {
    let data = ContextResult {
        file: file.display().to_string(),
        target_line,
        symbol: symbol.cloned(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format context result as raw text (body with line marker, no framing).
fn format_context_raw(symbol: Option<&Symbol>, target_line: usize) -> String {
    let Some(sym) = symbol else {
        return String::new();
    };

    if let Some(body) = &sym.body {
        insert_line_marker(body, sym.line, target_line)
    } else {
        format!(
            "{}:{}:{} {} {}",
            sym.file.display(),
            sym.line,
            sym.column,
            sym.kind,
            sym.name,
        )
    }
}

/// Insert a `// <- line N` marker on the appropriate line of the body text.
///
/// The body starts at `body_start_line` (1-based). The marker is inserted
/// at the end of the line corresponding to `target_line`.
fn insert_line_marker(body: &str, body_start_line: usize, target_line: usize) -> String {
    let target_offset = target_line.saturating_sub(body_start_line);
    let mut result = String::new();
    for (i, line) in body.lines().enumerate() {
        if i > 0 {
            result.push('\n');
        }
        result.push_str(line);
        if i == target_offset {
            result.push_str("    // <- line ");
            result.push_str(&target_line.to_string());
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Format a single frame header line.
fn format_frame_header(symbol: &Symbol) -> String {
    format!(
        "@@ {}:{}:{} {} {} @@",
        symbol.file.display(),
        symbol.line,
        symbol.column,
        symbol.kind,
        symbol.name,
    )
}

/// Format a symbol for the outline, at a given indent level.
fn format_outline_entry(symbol: &Symbol, indent: usize) -> String {
    let spaces = " ".repeat(indent * 2);
    format!(
        "{spaces}{} ({}, {}) :{}",
        symbol.name, symbol.kind, symbol.visibility, symbol.line,
    )
}

/// Recursively format a symbol and its children.
fn format_outline_symbol(symbol: &Symbol, indent: usize, output: &mut String) {
    output.push_str(&format_outline_entry(symbol, indent));
    for child in &symbol.children {
        output.push('\n');
        format_outline_symbol(child, indent + 1, output);
    }
}

// ---------------------------------------------------------------------------
// Symbols formatting
// ---------------------------------------------------------------------------

/// JSON payload for the `symbols` command.
#[derive(Debug, Serialize)]
pub struct SymbolsResult {
    /// All symbols found in the project.
    pub symbols: Vec<Symbol>,
    /// Total number of symbols.
    pub total: usize,
}

/// Format `symbols` results in the requested mode.
pub fn format_symbols(symbols: &[Symbol], mode: OutputMode, pretty: bool) -> String {
    match mode {
        OutputMode::Framed => format_symbols_framed(symbols),
        OutputMode::Json => format_symbols_json(symbols, pretty),
        OutputMode::Raw => format_symbols_raw(symbols),
    }
}

/// Format symbols as framed output: `@@ file:line:col kind name @@` per symbol.
fn format_symbols_framed(symbols: &[Symbol]) -> String {
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        output.push_str(&format_frame_header(symbol));
    }
    output
}

/// Format `symbols` results as JSON wrapped in `QueryResult`.
fn format_symbols_json(symbols: &[Symbol], force_pretty: bool) -> String {
    let data = SymbolsResult {
        total: symbols.len(),
        symbols: symbols.to_vec(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `symbols` results as raw text: `file:line:col kind name` per symbol.
fn format_symbols_raw(symbols: &[Symbol]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let _ = write!(
            output,
            "{}:{}:{} {} {}",
            symbol.file.display(),
            symbol.line,
            symbol.column,
            symbol.kind,
            symbol.name,
        );
    }
    output
}

// ---------------------------------------------------------------------------
// Tree formatting
// ---------------------------------------------------------------------------

/// Format `tree` results in the requested mode.
pub fn format_tree_output(
    file_symbols: &[FileSymbols],
    scope: Option<&Path>,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_tree_framed(file_symbols, scope),
        OutputMode::Json => format_tree_json(file_symbols, scope, pretty),
        OutputMode::Raw => format_tree_raw(file_symbols),
    }
}

/// Format tree as framed output: `@@ scope @@` header, then files with indented symbols.
fn format_tree_framed(file_symbols: &[FileSymbols], scope: Option<&Path>) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    // Scope header
    if let Some(scope) = scope {
        let _ = write!(output, "@@ {} @@", scope.display());
    } else {
        output.push_str("@@ . @@");
    }

    for fs in file_symbols {
        output.push('\n');
        let _ = write!(output, "{}", fs.file.display());
        for symbol in &fs.symbols {
            output.push('\n');
            format_tree_symbol(symbol, 1, &mut output);
        }
    }

    output
}

/// Format `tree` results as JSON wrapped in `QueryResult`.
fn format_tree_json(
    file_symbols: &[FileSymbols],
    scope: Option<&Path>,
    force_pretty: bool,
) -> String {
    let total_symbols: usize = file_symbols
        .iter()
        .map(|fs| count_symbols_recursive(&fs.symbols))
        .sum();

    let files: Vec<TreeFileEntry> = file_symbols
        .iter()
        .map(|fs| TreeFileEntry {
            file: fs.file.display().to_string(),
            symbols: fs.symbols.clone(),
        })
        .collect();

    let data = TreeResult {
        scope: scope.map(|p| p.display().to_string()),
        total_files: files.len(),
        total_symbols,
        files,
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format tree as raw text (no `@@` header), files with indented symbols.
fn format_tree_raw(file_symbols: &[FileSymbols]) -> String {
    let mut output = String::new();

    for (i, fs) in file_symbols.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        output.push_str(&fs.file.display().to_string());
        for symbol in &fs.symbols {
            output.push('\n');
            format_tree_symbol(symbol, 1, &mut output);
        }
    }

    output
}

/// Recursively format a symbol in the tree, with indentation.
fn format_tree_symbol(symbol: &Symbol, indent: usize, output: &mut String) {
    use std::fmt::Write;
    let spaces = " ".repeat(indent * 2);
    let _ = write!(
        output,
        "{spaces}{} ({}, {}) :{}",
        symbol.name, symbol.kind, symbol.visibility, symbol.line,
    );
    for child in &symbol.children {
        output.push('\n');
        format_tree_symbol(child, indent + 1, output);
    }
}

/// Count symbols recursively (including children).
fn count_symbols_recursive(symbols: &[Symbol]) -> usize {
    symbols
        .iter()
        .fold(0, |acc, s| acc + 1 + count_symbols_recursive(&s.children))
}

// ---------------------------------------------------------------------------
// Refs formatting
// ---------------------------------------------------------------------------

/// Format `refs` results in the requested mode.
#[allow(clippy::too_many_arguments)]
// All parameters are needed to support definition, reference, context, and output mode options
pub fn format_refs(
    definitions: &[Symbol],
    references: &[Reference],
    symbol_name: &str,
    mode: OutputMode,
    pretty: bool,
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
    resolution: Resolution,
) -> String {
    match mode {
        OutputMode::Framed => format_refs_framed(
            definitions,
            references,
            context_lines,
            source_map,
            resolution,
        ),
        OutputMode::Json => {
            format_refs_json(definitions, references, symbol_name, pretty, resolution)
        }
        OutputMode::Raw => format_refs_raw(definitions, references, context_lines, source_map),
    }
}

/// Format refs results as framed output.
///
/// Shows definition location(s) first, then each reference with its context line.
/// Ends with a summary count indicating whether results are resolved or syntactic.
fn format_refs_framed(
    definitions: &[Symbol],
    references: &[Reference],
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
    resolution: Resolution,
) -> String {
    use crate::commands::refs::get_context_lines;
    use std::fmt::Write;

    let mut output = String::new();

    // Show definitions first
    for (i, def) in definitions.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let _ = write!(
            output,
            "@@ {}:{}:{} {} {} (definition) @@",
            def.file.display(),
            def.line,
            def.column,
            def.kind,
            def.name,
        );
    }

    // Show references
    for r in references {
        if !output.is_empty() {
            output.push('\n');
        }
        let _ = write!(
            output,
            "@@ {}:{}:{} {} @@",
            r.file.display(),
            r.line,
            r.column,
            r.kind,
        );

        if context_lines > 0 {
            if let Some(source) = source_map.get(r.file.as_path()) {
                let ctx = get_context_lines(source, r.line, context_lines);
                for line in &ctx {
                    output.push('\n');
                    output.push_str(line);
                }
            }
        } else {
            // Show the single context line
            output.push('\n');
            let trimmed = r.context.trim_start();
            output.push_str("    ");
            output.push_str(trimmed);
        }
    }

    // Summary line — indicate resolution quality
    if !output.is_empty() {
        output.push('\n');
    }
    let summary = match resolution {
        Resolution::Resolved => "resolved",
        _ => "syntactic match \u{2014} may be incomplete",
    };
    let _ = write!(
        output,
        "\n{} reference{} ({summary})",
        references.len(),
        if references.len() == 1 { "" } else { "s" },
    );

    output
}

/// Format `refs` results as JSON wrapped in `QueryResult`.
fn format_refs_json(
    definitions: &[Symbol],
    references: &[Reference],
    symbol_name: &str,
    force_pretty: bool,
    resolution: Resolution,
) -> String {
    let data = RefsResult {
        symbol: symbol_name.to_string(),
        definitions: definitions.to_vec(),
        references: references.to_vec(),
        total: references.len(),
    };
    let note = match resolution {
        Resolution::Resolved => None,
        _ => Some(
            "name-based matching; may include false positives or miss renamed symbols".to_string(),
        ),
    };
    let result = QueryResult {
        resolution,
        completeness: Completeness::BestEffort,
        note,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `refs` results as raw text (no `@@` delimiters).
fn format_refs_raw(
    definitions: &[Symbol],
    references: &[Reference],
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
) -> String {
    use crate::commands::refs::get_context_lines;
    use std::fmt::Write;

    let mut output = String::new();

    // Show definitions
    for (i, def) in definitions.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let _ = write!(
            output,
            "{}:{}:{} {} {} (definition)",
            def.file.display(),
            def.line,
            def.column,
            def.kind,
            def.name,
        );
    }

    // Show references
    for r in references {
        if !output.is_empty() {
            output.push('\n');
        }
        let _ = write!(
            output,
            "{}:{}:{} {}",
            r.file.display(),
            r.line,
            r.column,
            r.kind,
        );

        if context_lines > 0 {
            if let Some(source) = source_map.get(r.file.as_path()) {
                let ctx = get_context_lines(source, r.line, context_lines);
                for line in &ctx {
                    output.push('\n');
                    output.push_str(line);
                }
            }
        }
    }

    output
}

// ---------------------------------------------------------------------------
// Callers formatting
// ---------------------------------------------------------------------------

/// Format `callers` results in the requested mode.
#[allow(clippy::too_many_arguments)]
// All parameters are needed to support definition, reference, context, and output mode options
pub fn format_callers(
    definitions: &[Symbol],
    callers: &[Reference],
    symbol_name: &str,
    mode: OutputMode,
    pretty: bool,
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
) -> String {
    match mode {
        OutputMode::Framed => {
            format_callers_framed(definitions, callers, context_lines, source_map)
        }
        OutputMode::Json => format_callers_json(definitions, callers, symbol_name, pretty),
        OutputMode::Raw => format_callers_raw(callers, context_lines, source_map),
    }
}

/// Format callers results as framed output.
///
/// Shows definition location(s) first, then each call site with caller info.
/// Ends with a summary count.
fn format_callers_framed(
    definitions: &[Symbol],
    callers: &[Reference],
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
) -> String {
    use crate::commands::refs::get_context_lines;
    use std::fmt::Write;

    let mut output = String::new();

    // Show definitions first
    for (i, def) in definitions.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let _ = write!(
            output,
            "@@ {}:{}:{} {} {} (definition) @@",
            def.file.display(),
            def.line,
            def.column,
            def.kind,
            def.name,
        );
    }

    // Show call-site references with caller info
    for r in callers {
        if !output.is_empty() {
            output.push('\n');
        }

        // Include caller function name if available
        let caller_info = match &r.caller {
            Some(name) => format!(" (in {name})"),
            None => String::new(),
        };

        let _ = write!(
            output,
            "@@ {}:{}:{} call{} @@",
            r.file.display(),
            r.line,
            r.column,
            caller_info,
        );

        if context_lines > 0 {
            if let Some(source) = source_map.get(r.file.as_path()) {
                let ctx = get_context_lines(source, r.line, context_lines);
                for line in &ctx {
                    output.push('\n');
                    output.push_str(line);
                }
            }
        } else {
            // Show the single context line
            output.push('\n');
            let trimmed = r.context.trim_start();
            output.push_str("    ");
            output.push_str(trimmed);
        }
    }

    // Summary line
    if !output.is_empty() {
        output.push('\n');
    }
    let _ = write!(
        output,
        "\n{} caller{} (syntactic match \u{2014} may be incomplete)",
        callers.len(),
        if callers.len() == 1 { "" } else { "s" },
    );

    output
}

/// Format `callers` results as JSON wrapped in `QueryResult`.
fn format_callers_json(
    definitions: &[Symbol],
    callers: &[Reference],
    symbol_name: &str,
    force_pretty: bool,
) -> String {
    let data = CallersResult {
        symbol: symbol_name.to_string(),
        definitions: definitions.to_vec(),
        callers: callers.to_vec(),
        total: callers.len(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::BestEffort,
        note: Some(
            "name-based matching; may include false positives or miss renamed symbols".to_string(),
        ),
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `callers` results as raw text (no `@@` delimiters).
fn format_callers_raw(
    callers: &[Reference],
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
) -> String {
    use crate::commands::refs::get_context_lines;
    use std::fmt::Write;

    let mut output = String::new();

    for (i, r) in callers.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }

        let caller_info = match &r.caller {
            Some(name) => format!(" (in {name})"),
            None => String::new(),
        };

        let _ = write!(
            output,
            "{}:{}:{} call{}",
            r.file.display(),
            r.line,
            r.column,
            caller_info,
        );

        if context_lines > 0 {
            if let Some(source) = source_map.get(r.file.as_path()) {
                let ctx = get_context_lines(source, r.line, context_lines);
                for line in &ctx {
                    output.push('\n');
                    output.push_str(line);
                }
            }
        }
    }

    output
}

// ---------------------------------------------------------------------------
// Deps formatting
// ---------------------------------------------------------------------------

/// Format `deps` results in the requested mode.
pub fn format_deps(
    target: Option<&Symbol>,
    deps: &[Dependency],
    symbol_name: &str,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_deps_framed(target, deps),
        OutputMode::Json => format_deps_json(target, deps, symbol_name, pretty),
        OutputMode::Raw => format_deps_raw(deps),
    }
}

/// Format deps as framed output.
fn format_deps_framed(target: Option<&Symbol>, deps: &[Dependency]) -> String {
    use std::fmt::Write;
    let mut output = String::new();

    if let Some(sym) = target {
        output.push_str(&format_frame_header(sym));
    }

    for dep in deps {
        output.push('\n');
        let defined = dep.defined_in.as_deref().unwrap_or("<unresolved>");
        let _ = write!(output, "  {} ({}) -> {}", dep.name, dep.kind, defined);
    }

    output
}

/// Format `deps` results as JSON wrapped in `QueryResult`.
fn format_deps_json(
    target: Option<&Symbol>,
    deps: &[Dependency],
    symbol_name: &str,
    force_pretty: bool,
) -> String {
    let data = DepsResult {
        symbol: symbol_name.to_string(),
        definition: target.cloned(),
        dependencies: deps.to_vec(),
        total: deps.len(),
    };
    // Determine overall resolution: Resolved if any dependency was resolved,
    // otherwise Syntactic.
    let overall_resolution = if deps.iter().any(|d| d.resolution == Resolution::Resolved) {
        Resolution::Resolved
    } else {
        Resolution::Syntactic
    };
    let result = QueryResult {
        resolution: overall_resolution,
        completeness: Completeness::BestEffort,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `deps` results as raw text (no framing).
fn format_deps_raw(deps: &[Dependency]) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    for (i, dep) in deps.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        let defined = dep.defined_in.as_deref().unwrap_or("<unresolved>");
        let _ = write!(output, "{} ({}) -> {}", dep.name, dep.kind, defined);
    }
    output
}

/// Serialize a value to JSON, choosing pretty or compact based on TTY and flags.
fn serialize_json<T: Serialize>(value: &T, force_pretty: bool) -> String {
    let use_pretty = force_pretty || std::io::stdout().is_terminal();
    if use_pretty {
        serde_json::to_string_pretty(value).unwrap_or_default()
    } else {
        serde_json::to_string(value).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codequery_core::{SymbolKind, Visibility};
    use std::path::PathBuf;

    fn make_symbol(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        column: usize,
        visibility: Visibility,
        children: Vec<Symbol>,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column,
            end_line: line + 5,
            visibility,
            children,
            doc: None,
            body: None,
            signature: None,
        }
    }

    fn make_symbol_with_body(
        name: &str,
        kind: SymbolKind,
        file: &str,
        line: usize,
        column: usize,
        visibility: Visibility,
        body: &str,
    ) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from(file),
            line,
            column,
            end_line: line + 5,
            visibility,
            children: vec![],
            doc: Some("A doc comment.".to_string()),
            body: Some(body.to_string()),
            signature: Some(format!("fn {name}()")),
        }
    }

    // -----------------------------------------------------------------------
    // Framed output tests (regression)
    // -----------------------------------------------------------------------

    #[test]
    fn test_def_single_result_produces_correct_frame_header() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_def_results(&symbols);
        assert_eq!(output, "@@ src/lib.rs:1:0 function foo @@");
    }

    #[test]
    fn test_def_multiple_results_separated_by_blank_line() {
        let symbols = vec![
            make_symbol(
                "foo",
                SymbolKind::Function,
                "src/lib.rs",
                1,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "bar",
                SymbolKind::Function,
                "src/main.rs",
                10,
                4,
                Visibility::Private,
                vec![],
            ),
        ];
        let output = format_def_results(&symbols);
        assert_eq!(
            output,
            "@@ src/lib.rs:1:0 function foo @@\n\n@@ src/main.rs:10:4 function bar @@"
        );
    }

    #[test]
    fn test_def_empty_results_returns_empty_string() {
        let output = format_def_results(&[]);
        assert_eq!(output, "");
    }

    #[test]
    fn test_outline_flat_symbols_produces_correct_output() {
        let symbols = vec![
            make_symbol(
                "greet",
                SymbolKind::Function,
                "src/lib.rs",
                10,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "MAX_RETRIES",
                SymbolKind::Const,
                "src/lib.rs",
                20,
                0,
                Visibility::Public,
                vec![],
            ),
        ];
        let output = format_outline(Path::new("src/lib.rs"), &symbols);
        assert_eq!(
            output,
            "@@ src/lib.rs @@\n  greet (function, pub) :10\n  MAX_RETRIES (const, pub) :20"
        );
    }

    #[test]
    fn test_outline_nested_symbols_produces_correct_indentation() {
        let method = make_symbol(
            "new",
            SymbolKind::Method,
            "src/lib.rs",
            22,
            4,
            Visibility::Public,
            vec![],
        );
        let impl_block = make_symbol(
            "Router",
            SymbolKind::Impl,
            "src/lib.rs",
            20,
            0,
            Visibility::Private,
            vec![method],
        );
        let func = make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        );
        let symbols = vec![func, impl_block];
        let output = format_outline(Path::new("src/lib.rs"), &symbols);
        let expected = "@@ src/lib.rs @@\n  greet (function, pub) :10\n  Router (impl, priv) :20\n    new (method, pub) :22";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_outline_file_header_format() {
        let output = format_outline(Path::new("src/api/routes.rs"), &[]);
        assert!(output.starts_with("@@ src/api/routes.rs @@"));
    }

    #[test]
    fn test_outline_visibility_values_display_correctly() {
        let symbols = vec![
            make_symbol(
                "public_fn",
                SymbolKind::Function,
                "lib.rs",
                1,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "private_fn",
                SymbolKind::Function,
                "lib.rs",
                5,
                0,
                Visibility::Private,
                vec![],
            ),
            make_symbol(
                "crate_fn",
                SymbolKind::Function,
                "lib.rs",
                10,
                0,
                Visibility::Crate,
                vec![],
            ),
        ];
        let output = format_outline(Path::new("lib.rs"), &symbols);
        assert!(output.contains("(function, pub) :1"));
        assert!(output.contains("(function, priv) :5"));
        assert!(output.contains("(function, pub(crate)) :10"));
    }

    #[test]
    fn test_different_symbol_kinds_display_correctly() {
        let symbols = vec![
            make_symbol(
                "MyStruct",
                SymbolKind::Struct,
                "lib.rs",
                1,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "MyTrait",
                SymbolKind::Trait,
                "lib.rs",
                10,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "MyEnum",
                SymbolKind::Enum,
                "lib.rs",
                20,
                0,
                Visibility::Public,
                vec![],
            ),
        ];

        let def_output = format_def_results(&symbols);
        assert!(def_output.contains("struct MyStruct"));
        assert!(def_output.contains("trait MyTrait"));
        assert!(def_output.contains("enum MyEnum"));

        let outline_output = format_outline(Path::new("lib.rs"), &symbols);
        assert!(outline_output.contains("MyStruct (struct, pub)"));
        assert!(outline_output.contains("MyTrait (trait, pub)"));
        assert!(outline_output.contains("MyEnum (enum, pub)"));
    }

    #[test]
    fn test_outline_no_symbols_shows_just_file_header() {
        let output = format_outline(Path::new("src/empty.rs"), &[]);
        assert_eq!(output, "@@ src/empty.rs @@");
    }

    #[test]
    fn test_frame_header_uses_zero_based_column() {
        let symbols = vec![make_symbol(
            "indented",
            SymbolKind::Function,
            "src/lib.rs",
            5,
            8,
            Visibility::Private,
            vec![],
        )];
        let output = format_def_results(&symbols);
        assert_eq!(output, "@@ src/lib.rs:5:8 function indented @@");
    }

    #[test]
    fn test_file_paths_not_normalized() {
        let symbols = vec![make_symbol(
            "func",
            SymbolKind::Function,
            "./src/../src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let def_output = format_def_results(&symbols);
        assert!(def_output.contains("./src/../src/lib.rs:1:0"));

        let outline_output = format_outline(Path::new("./weird/path/../file.rs"), &[]);
        assert!(outline_output.contains("./weird/path/../file.rs"));
    }

    // -----------------------------------------------------------------------
    // JSON output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_def_json_produces_valid_json_with_metadata() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_def(&symbols, "greet", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "greet");
        assert_eq!(json["total"], 1);
        assert!(json["definitions"].is_array());
        assert_eq!(json["definitions"][0]["name"], "greet");
        assert_eq!(json["definitions"][0]["kind"], "function");
    }

    #[test]
    fn test_def_json_empty_results_has_metadata() {
        let output = format_def(&[], "missing", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "missing");
        assert_eq!(json["total"], 0);
        assert_eq!(json["definitions"], serde_json::json!([]));
    }

    #[test]
    fn test_outline_json_produces_valid_json_with_metadata() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        )];
        let output =
            format_outline_output(Path::new("src/lib.rs"), &symbols, OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["file"], "src/lib.rs");
        assert!(json["symbols"].is_array());
        assert_eq!(json["symbols"][0]["name"], "greet");
    }

    #[test]
    fn test_outline_json_empty_symbols_has_metadata() {
        let output = format_outline_output(Path::new("src/empty.rs"), &[], OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["file"], "src/empty.rs");
        assert_eq!(json["symbols"], serde_json::json!([]));
    }

    // -----------------------------------------------------------------------
    // Raw output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_def_raw_strips_frame_delimiters() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_def(&symbols, "foo", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert_eq!(output, "src/lib.rs:1:0 function foo");
    }

    #[test]
    fn test_def_raw_multiple_results_newline_separated() {
        let symbols = vec![
            make_symbol(
                "foo",
                SymbolKind::Function,
                "src/lib.rs",
                1,
                0,
                Visibility::Public,
                vec![],
            ),
            make_symbol(
                "foo",
                SymbolKind::Function,
                "src/main.rs",
                10,
                0,
                Visibility::Private,
                vec![],
            ),
        ];
        let output = format_def(&symbols, "foo", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "src/lib.rs:1:0 function foo");
        assert_eq!(lines[1], "src/main.rs:10:0 function foo");
    }

    #[test]
    fn test_outline_raw_strips_frame_header() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        )];
        let output =
            format_outline_output(Path::new("src/lib.rs"), &symbols, OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert!(output.contains("greet (function, pub) :10"));
    }

    #[test]
    fn test_outline_raw_empty_symbols_is_empty() {
        let output = format_outline_output(Path::new("src/lib.rs"), &[], OutputMode::Raw, false);
        assert!(output.is_empty());
    }

    // -----------------------------------------------------------------------
    // Framed mode via format_def / format_outline_output (regression)
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_def_framed_matches_format_def_results() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        assert_eq!(
            format_def(&symbols, "foo", OutputMode::Framed, false),
            format_def_results(&symbols)
        );
    }

    #[test]
    fn test_format_outline_output_framed_matches_format_outline() {
        let symbols = vec![make_symbol(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            10,
            0,
            Visibility::Public,
            vec![],
        )];
        assert_eq!(
            format_outline_output(Path::new("src/lib.rs"), &symbols, OutputMode::Framed, false),
            format_outline(Path::new("src/lib.rs"), &symbols)
        );
    }

    // -----------------------------------------------------------------------
    // Body framed output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_body_framed_single_result_with_body_text() {
        let symbols = vec![make_symbol_with_body(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            "pub fn greet(name: &str) -> String {\n    format!(\"Hello, {name}!\")\n}",
        )];
        let output = format_body(&symbols, "greet", OutputMode::Framed, false);
        assert!(output.starts_with("@@ src/lib.rs:9:0 function greet @@\n"));
        assert!(output.contains("pub fn greet(name: &str) -> String {"));
        assert!(output.contains("format!(\"Hello, {name}!\")"));
    }

    #[test]
    fn test_body_framed_multiple_results_separated_by_blank_line() {
        let symbols = vec![
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/lib.rs",
                1,
                0,
                Visibility::Public,
                "fn foo() {}",
            ),
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/main.rs",
                10,
                0,
                Visibility::Private,
                "fn foo() { 42 }",
            ),
        ];
        let output = format_body(&symbols, "foo", OutputMode::Framed, false);
        assert!(output.contains("@@ src/lib.rs:1:0 function foo @@\nfn foo() {}"));
        assert!(output.contains("\n\n@@ src/main.rs:10:0 function foo @@\nfn foo() { 42 }"));
    }

    #[test]
    fn test_body_framed_empty_results_returns_empty_string() {
        let output = format_body(&[], "missing", OutputMode::Framed, false);
        assert_eq!(output, "");
    }

    #[test]
    fn test_body_framed_symbol_without_body_shows_header_only() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_body(&symbols, "foo", OutputMode::Framed, false);
        assert_eq!(output, "@@ src/lib.rs:1:0 function foo @@");
    }

    // -----------------------------------------------------------------------
    // Body JSON output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_body_json_produces_valid_json_with_body_field() {
        let symbols = vec![make_symbol_with_body(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            "pub fn greet() {}",
        )];
        let output = format_body(&symbols, "greet", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "greet");
        assert_eq!(json["total"], 1);
        assert!(json["definitions"].is_array());
        assert_eq!(json["definitions"][0]["name"], "greet");
        assert_eq!(json["definitions"][0]["body"], "pub fn greet() {}");
        assert_eq!(json["definitions"][0]["signature"], "fn greet()");
        assert_eq!(json["definitions"][0]["doc"], "A doc comment.");
    }

    #[test]
    fn test_body_json_empty_results_has_metadata() {
        let output = format_body(&[], "missing", OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["symbol"], "missing");
        assert_eq!(json["total"], 0);
        assert_eq!(json["definitions"], serde_json::json!([]));
    }

    // -----------------------------------------------------------------------
    // Body raw output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_body_raw_outputs_body_text_only() {
        let symbols = vec![make_symbol_with_body(
            "greet",
            SymbolKind::Function,
            "src/lib.rs",
            9,
            0,
            Visibility::Public,
            "pub fn greet() {\n    println!(\"hello\");\n}",
        )];
        let output = format_body(&symbols, "greet", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert_eq!(output, "pub fn greet() {\n    println!(\"hello\");\n}");
    }

    #[test]
    fn test_body_raw_multiple_results_separated_by_blank_line() {
        let symbols = vec![
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/lib.rs",
                1,
                0,
                Visibility::Public,
                "fn foo() {}",
            ),
            make_symbol_with_body(
                "foo",
                SymbolKind::Function,
                "src/main.rs",
                10,
                0,
                Visibility::Private,
                "fn foo() { 42 }",
            ),
        ];
        let output = format_body(&symbols, "foo", OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert_eq!(output, "fn foo() {}\n\nfn foo() { 42 }");
    }

    #[test]
    fn test_body_raw_empty_results_returns_empty_string() {
        let output = format_body(&[], "missing", OutputMode::Raw, false);
        assert_eq!(output, "");
    }

    #[test]
    fn test_body_raw_symbol_without_body_returns_empty_string() {
        let symbols = vec![make_symbol(
            "foo",
            SymbolKind::Function,
            "src/lib.rs",
            1,
            0,
            Visibility::Public,
            vec![],
        )];
        let output = format_body(&symbols, "foo", OutputMode::Raw, false);
        assert_eq!(output, "");
    }

    // -----------------------------------------------------------------------
    // Imports output tests
    // -----------------------------------------------------------------------

    fn make_import(source: &str, kind: &str, line: usize, external: bool) -> ImportInfo {
        ImportInfo {
            source: source.to_string(),
            kind: kind.to_string(),
            line,
            external,
        }
    }

    #[test]
    fn test_imports_framed_produces_file_header_and_entries() {
        let imports = vec![
            make_import("std::collections::HashMap", "use", 1, true),
            make_import("crate::models::User", "use", 2, false),
        ];
        let output =
            format_imports_output(Path::new("src/lib.rs"), &imports, OutputMode::Framed, false);
        assert!(output.starts_with("@@ src/lib.rs @@"));
        assert!(output.contains("@@ src/lib.rs:1 use std::collections::HashMap @@"));
        assert!(output.contains("@@ src/lib.rs:2 use crate::models::User @@"));
    }

    #[test]
    fn test_imports_framed_empty_shows_just_file_header() {
        let output =
            format_imports_output(Path::new("src/empty.rs"), &[], OutputMode::Framed, false);
        assert_eq!(output, "@@ src/empty.rs @@");
    }

    #[test]
    fn test_imports_json_produces_valid_json_with_metadata() {
        let imports = vec![make_import("std::io", "use", 1, true)];
        let output =
            format_imports_output(Path::new("src/lib.rs"), &imports, OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["file"], "src/lib.rs");
        assert_eq!(json["total"], 1);
        assert!(json["imports"].is_array());
        assert_eq!(json["imports"][0]["source"], "std::io");
        assert_eq!(json["imports"][0]["kind"], "use");
        assert_eq!(json["imports"][0]["line"], 1);
        assert_eq!(json["imports"][0]["external"], true);
    }

    #[test]
    fn test_imports_json_empty_has_metadata() {
        let output = format_imports_output(Path::new("src/empty.rs"), &[], OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["total"], 0);
        assert_eq!(json["imports"], serde_json::json!([]));
    }

    #[test]
    fn test_imports_raw_strips_frame_delimiters() {
        let imports = vec![
            make_import("std::io", "use", 1, true),
            make_import("crate::models", "use", 2, false),
        ];
        let output =
            format_imports_output(Path::new("src/lib.rs"), &imports, OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], ":1 use std::io");
        assert_eq!(lines[1], ":2 use crate::models");
    }

    #[test]
    fn test_imports_raw_empty_is_empty() {
        let output = format_imports_output(Path::new("src/lib.rs"), &[], OutputMode::Raw, false);
        assert!(output.is_empty());
    }

    // -----------------------------------------------------------------------
    // Context output tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_context_framed_with_body_includes_line_marker() {
        let sym = Symbol {
            name: "handle_request".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/api/routes.rs"),
            line: 42,
            column: 4,
            end_line: 46,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: Some("pub async fn handle_request(req: Request) -> Response {\n    let auth = authenticate(&req).await?;\n    let data = parse_body(&req).await?;\n    process(auth, data).await\n}".to_string()),
            signature: None,
        };
        let output = format_context_output(
            Some(&sym),
            44,
            Path::new("src/api/routes.rs"),
            OutputMode::Framed,
            false,
        );
        assert!(output
            .contains("@@ src/api/routes.rs:42:4 function handle_request (contains line 44) @@"));
        assert!(output.contains("// <- line 44"));
    }

    #[test]
    fn test_context_framed_no_symbol_shows_no_enclosing() {
        let output =
            format_context_output(None, 5, Path::new("src/lib.rs"), OutputMode::Framed, false);
        assert!(output.contains("@@ src/lib.rs:5 (no enclosing symbol) @@"));
    }

    #[test]
    fn test_context_framed_without_body_shows_header_only() {
        let sym = Symbol {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/lib.rs"),
            line: 10,
            column: 0,
            end_line: 15,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: None,
            signature: None,
        };
        let output = format_context_output(
            Some(&sym),
            12,
            Path::new("src/lib.rs"),
            OutputMode::Framed,
            false,
        );
        assert!(output.contains("@@ src/lib.rs:10:0 function foo (contains line 12) @@"));
        // No body lines after header
        assert_eq!(output.lines().count(), 1);
    }

    #[test]
    fn test_context_json_produces_valid_json_with_metadata() {
        let sym = Symbol {
            name: "greet".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/lib.rs"),
            line: 9,
            column: 0,
            end_line: 11,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: Some("pub fn greet() {}".to_string()),
            signature: None,
        };
        let output = format_context_output(
            Some(&sym),
            10,
            Path::new("src/lib.rs"),
            OutputMode::Json,
            true,
        );
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["resolution"], "syntactic");
        assert_eq!(json["completeness"], "exhaustive");
        assert_eq!(json["file"], "src/lib.rs");
        assert_eq!(json["target_line"], 10);
        assert!(json["symbol"].is_object());
        assert_eq!(json["symbol"]["name"], "greet");
    }

    #[test]
    fn test_context_json_no_symbol_has_null_symbol() {
        let output =
            format_context_output(None, 5, Path::new("src/lib.rs"), OutputMode::Json, true);
        let json: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(json["file"], "src/lib.rs");
        assert_eq!(json["target_line"], 5);
        assert!(json.get("symbol").is_none()); // skip_serializing_if = None
    }

    #[test]
    fn test_context_raw_with_body_has_marker_no_framing() {
        let sym = Symbol {
            name: "foo".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/lib.rs"),
            line: 5,
            column: 0,
            end_line: 8,
            visibility: Visibility::Public,
            children: vec![],
            doc: None,
            body: Some("fn foo() {\n    bar();\n    baz();\n}".to_string()),
            signature: None,
        };
        let output = format_context_output(
            Some(&sym),
            7,
            Path::new("src/lib.rs"),
            OutputMode::Raw,
            false,
        );
        assert!(!output.contains("@@"));
        assert!(output.contains("// <- line 7"));
    }

    #[test]
    fn test_context_raw_no_symbol_is_empty() {
        let output =
            format_context_output(None, 5, Path::new("src/lib.rs"), OutputMode::Raw, false);
        assert!(output.is_empty());
    }

    #[test]
    fn test_insert_line_marker_first_line() {
        let body = "fn foo() {\n    bar();\n}";
        let result = insert_line_marker(body, 10, 10);
        assert!(result.starts_with("fn foo() {    // <- line 10"));
    }

    #[test]
    fn test_insert_line_marker_middle_line() {
        let body = "fn foo() {\n    bar();\n    baz();\n}";
        let result = insert_line_marker(body, 5, 7);
        // line 7 is offset 2 from start line 5
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines[2].contains("// <- line 7"));
        // Other lines should NOT have the marker
        assert!(!lines[0].contains("// <-"));
        assert!(!lines[1].contains("// <-"));
        assert!(!lines[3].contains("// <-"));
    }

    #[test]
    fn test_insert_line_marker_last_line() {
        let body = "fn foo() {\n    bar();\n}";
        let result = insert_line_marker(body, 10, 12);
        let lines: Vec<&str> = result.lines().collect();
        assert!(lines[2].contains("// <- line 12"));
    }
}
