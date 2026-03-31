//! Output formatting for cq commands — framed, JSON, and raw modes.
//!
//! This module turns `Symbol` data from codequery-core into the three output
//! formats defined in SPECIFICATION.md section 9. It is pure formatting:
//! no I/O, no parsing, only string construction from typed symbol data.

use codequery_core::{
    Completeness, Diagnostic, DiagnosticSeverity, QueryResult, Reference, Resolution, Symbol,
    Visibility,
};
use codequery_index::FileSymbols;
use codequery_parse::{ImportInfo, SearchMatch};
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
pub fn format_def_results(symbols: &[Symbol]) -> String {
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
        None,
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n");
        }
        content.push_str(&format_frame_header(symbol));
    }
    prepend_meta_framed(&meta, &content)
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
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
            "{}:{}:{} {} {}",
            symbol.file.display(),
            symbol.line,
            symbol.column,
            symbol.kind,
            symbol.name,
        );
    }
    prepend_meta_raw(&meta, &content)
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
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
        None,
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n");
        }
        content.push_str(&format_frame_header(symbol));
        if let Some(body) = &symbol.body {
            content.push('\n');
            content.push_str(body);
        }
    }
    prepend_meta_framed(&meta, &content)
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
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n");
        }
        if let Some(body) = &symbol.body {
            content.push_str(body);
        }
    }
    prepend_meta_raw(&meta, &content)
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
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
        None,
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n");
        }
        content.push_str(&format_frame_header(symbol));
        if let Some(ref sig) = symbol.signature {
            content.push('\n');
            content.push_str(sig);
        }
    }
    prepend_meta_framed(&meta, &content)
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
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n");
        }
        if let Some(ref sig) = symbol.signature {
            content.push_str(sig);
        }
    }
    prepend_meta_raw(&meta, &content)
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
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
        None,
    );
    let mut content = format!("@@ {} @@", file.display());
    for symbol in symbols {
        content.push('\n');
        format_outline_symbol(symbol, 1, &mut content);
    }
    prepend_meta_framed(&meta, &content)
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
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        format_outline_symbol(symbol, 0, &mut content);
    }
    prepend_meta_raw(&meta, &content)
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
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        imports.len(),
        None,
    );
    let mut content = format!("@@ {} @@", file.display());
    for import in imports {
        content.push('\n');
        let _ = write!(
            content,
            "  @@ {}:{} {} {} @@",
            file.display(),
            import.line,
            import.kind,
            import.source,
        );
    }
    prepend_meta_framed(&meta, &content)
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
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        imports.len(),
    );
    let mut content = String::new();
    for (i, import) in imports.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
            ":{} {} {}",
            import.line, import.kind, import.source
        );
    }
    prepend_meta_raw(&meta, &content)
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
    let total = usize::from(symbol.is_some());
    let meta = format_meta_header(Resolution::Syntactic, Completeness::Exhaustive, total, None);

    let content = if let Some(sym) = symbol {
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
    } else {
        format!(
            "@@ {}:{} (no enclosing symbol) @@",
            file.display(),
            target_line,
        )
    };

    prepend_meta_framed(&meta, &content)
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
    let total = usize::from(symbol.is_some());
    let meta = format_meta_comment(Resolution::Syntactic, Completeness::Exhaustive, total);

    let content = if let Some(sym) = symbol {
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
    } else {
        String::new()
    };

    prepend_meta_raw(&meta, &content)
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

/// Format a `@@ meta ... @@` header line for framed output.
///
/// Produces: `@@ meta resolution=X completeness=Y total=N [note="..."] @@`
fn format_meta_header(
    resolution: Resolution,
    completeness: Completeness,
    total: usize,
    note: Option<&str>,
) -> String {
    use std::fmt::Write;
    let mut header =
        format!("@@ meta resolution={resolution} completeness={completeness} total={total}");
    if let Some(n) = note {
        let _ = write!(header, " note=\"{n}\"");
    }
    header.push_str(" @@");
    header
}

/// Format a `# meta ...` comment line for raw output.
///
/// Produces: `# meta resolution=X completeness=Y total=N`
fn format_meta_comment(resolution: Resolution, completeness: Completeness, total: usize) -> String {
    format!("# meta resolution={resolution} completeness={completeness} total={total}")
}

/// Prepend a meta header to framed output, with a blank line separator.
fn prepend_meta_framed(meta: &str, content: &str) -> String {
    if content.is_empty() {
        meta.to_string()
    } else {
        format!("{meta}\n\n{content}")
    }
}

/// Prepend a meta comment to raw output, with a newline separator.
fn prepend_meta_raw(meta: &str, content: &str) -> String {
    if content.is_empty() {
        meta.to_string()
    } else {
        format!("{meta}\n{content}")
    }
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
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
        None,
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        content.push_str(&format_frame_header(symbol));
    }
    prepend_meta_framed(&meta, &content)
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
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        symbols.len(),
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
            "{}:{}:{} {} {}",
            symbol.file.display(),
            symbol.line,
            symbol.column,
            symbol.kind,
            symbol.name,
        );
    }
    prepend_meta_raw(&meta, &content)
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
    let total_symbols: usize = file_symbols
        .iter()
        .map(|fs| count_symbols_recursive(&fs.symbols))
        .sum();
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        total_symbols,
        None,
    );
    let mut content = String::new();

    // Scope header
    if let Some(scope) = scope {
        let _ = write!(content, "@@ {} @@", scope.display());
    } else {
        content.push_str("@@ . @@");
    }

    for fs in file_symbols {
        content.push('\n');
        let _ = write!(content, "{}", fs.file.display());
        for symbol in &fs.symbols {
            content.push('\n');
            format_tree_symbol(symbol, 1, &mut content);
        }
    }

    prepend_meta_framed(&meta, &content)
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
    let total_symbols: usize = file_symbols
        .iter()
        .map(|fs| count_symbols_recursive(&fs.symbols))
        .sum();
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        total_symbols,
    );
    let mut content = String::new();

    for (i, fs) in file_symbols.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        content.push_str(&fs.file.display().to_string());
        for symbol in &fs.symbols {
            content.push('\n');
            format_tree_symbol(symbol, 1, &mut content);
        }
    }

    prepend_meta_raw(&meta, &content)
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
        OutputMode::Raw => format_refs_raw(
            definitions,
            references,
            context_lines,
            source_map,
            resolution,
        ),
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

    let note = match resolution {
        Resolution::Resolved => None,
        _ => Some("name-based matching; may include false positives"),
    };
    let meta = format_meta_header(resolution, Completeness::BestEffort, references.len(), note);

    let mut content = String::new();

    // Show definitions first
    for (i, def) in definitions.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
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
        if !content.is_empty() {
            content.push('\n');
        }
        let _ = write!(
            content,
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
                    content.push('\n');
                    content.push_str(line);
                }
            }
        } else {
            // Show the single context line
            content.push('\n');
            let trimmed = r.context.trim_start();
            content.push_str("    ");
            content.push_str(trimmed);
        }
    }

    // Summary line — indicate resolution quality
    if !content.is_empty() {
        content.push('\n');
    }
    let summary = match resolution {
        Resolution::Resolved => "resolved",
        _ => "syntactic match \u{2014} may be incomplete",
    };
    let _ = write!(
        content,
        "\n{} reference{} ({summary})",
        references.len(),
        if references.len() == 1 { "" } else { "s" },
    );

    prepend_meta_framed(&meta, &content)
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
    resolution: Resolution,
) -> String {
    use crate::commands::refs::get_context_lines;
    use std::fmt::Write;

    let meta = format_meta_comment(resolution, Completeness::BestEffort, references.len());
    let mut content = String::new();

    // Show definitions
    for (i, def) in definitions.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
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
        if !content.is_empty() {
            content.push('\n');
        }
        let _ = write!(
            content,
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
                    content.push('\n');
                    content.push_str(line);
                }
            }
        }
    }

    prepend_meta_raw(&meta, &content)
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
    resolution: Resolution,
) -> String {
    match mode {
        OutputMode::Framed => {
            format_callers_framed(definitions, callers, context_lines, source_map, resolution)
        }
        OutputMode::Json => {
            format_callers_json(definitions, callers, symbol_name, pretty, resolution)
        }
        OutputMode::Raw => format_callers_raw(callers, context_lines, source_map, resolution),
    }
}

/// Format callers results as framed output.
///
/// Shows definition location(s) first, then each call site with caller info.
/// Ends with a summary count indicating resolution quality.
fn format_callers_framed(
    definitions: &[Symbol],
    callers: &[Reference],
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
    resolution: Resolution,
) -> String {
    use crate::commands::refs::get_context_lines;
    use std::fmt::Write;

    let note = match resolution {
        Resolution::Resolved => None,
        _ => Some("name-based matching; may include false positives"),
    };
    let meta = format_meta_header(resolution, Completeness::BestEffort, callers.len(), note);

    let mut content = String::new();

    // Show definitions first
    for (i, def) in definitions.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
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
        if !content.is_empty() {
            content.push('\n');
        }

        // Include caller function name if available
        let caller_info = match &r.caller {
            Some(name) => format!(" (in {name})"),
            None => String::new(),
        };

        let _ = write!(
            content,
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
                    content.push('\n');
                    content.push_str(line);
                }
            }
        } else {
            // Show the single context line
            content.push('\n');
            let trimmed = r.context.trim_start();
            content.push_str("    ");
            content.push_str(trimmed);
        }
    }

    // Summary line — indicate resolution quality
    if !content.is_empty() {
        content.push('\n');
    }
    let summary = match resolution {
        Resolution::Resolved => "resolved",
        _ => "syntactic match \u{2014} may be incomplete",
    };
    let _ = write!(
        content,
        "\n{} caller{} ({summary})",
        callers.len(),
        if callers.len() == 1 { "" } else { "s" },
    );

    prepend_meta_framed(&meta, &content)
}

/// Format `callers` results as JSON wrapped in `QueryResult`.
fn format_callers_json(
    definitions: &[Symbol],
    callers: &[Reference],
    symbol_name: &str,
    force_pretty: bool,
    resolution: Resolution,
) -> String {
    let data = CallersResult {
        symbol: symbol_name.to_string(),
        definitions: definitions.to_vec(),
        callers: callers.to_vec(),
        total: callers.len(),
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

/// Format `callers` results as raw text (no `@@` delimiters).
fn format_callers_raw(
    callers: &[Reference],
    context_lines: usize,
    source_map: &HashMap<&Path, &str>,
    resolution: Resolution,
) -> String {
    use crate::commands::refs::get_context_lines;
    use std::fmt::Write;

    let meta = format_meta_comment(resolution, Completeness::BestEffort, callers.len());
    let mut content = String::new();

    for (i, r) in callers.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }

        let caller_info = match &r.caller {
            Some(name) => format!(" (in {name})"),
            None => String::new(),
        };

        let _ = write!(
            content,
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
                    content.push('\n');
                    content.push_str(line);
                }
            }
        }
    }

    prepend_meta_raw(&meta, &content)
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
    let overall_resolution = if deps.iter().any(|d| d.resolution == Resolution::Resolved) {
        Resolution::Resolved
    } else {
        Resolution::Syntactic
    };
    let meta = format_meta_header(
        overall_resolution,
        Completeness::BestEffort,
        deps.len(),
        None,
    );
    let mut content = String::new();

    if let Some(sym) = target {
        content.push_str(&format_frame_header(sym));
    }

    for dep in deps {
        content.push('\n');
        let defined = dep.defined_in.as_deref().unwrap_or("<unresolved>");
        let _ = write!(content, "  {} ({}) -> {}", dep.name, dep.kind, defined);
    }

    prepend_meta_framed(&meta, &content)
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
    let overall_resolution = if deps.iter().any(|d| d.resolution == Resolution::Resolved) {
        Resolution::Resolved
    } else {
        Resolution::Syntactic
    };
    let meta = format_meta_comment(overall_resolution, Completeness::BestEffort, deps.len());
    let mut content = String::new();
    for (i, dep) in deps.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let defined = dep.defined_in.as_deref().unwrap_or("<unresolved>");
        let _ = write!(content, "{} ({}) -> {}", dep.name, dep.kind, defined);
    }
    prepend_meta_raw(&meta, &content)
}

// ---------------------------------------------------------------------------
// Search formatting
// ---------------------------------------------------------------------------

/// JSON payload for a single search match.
#[derive(Debug, Serialize)]
pub struct SearchMatchEntry {
    /// Relative file path.
    pub file: String,
    /// Start line (0-indexed).
    pub line: usize,
    /// Start column (0-indexed).
    pub column: usize,
    /// End line (0-indexed).
    pub end_line: usize,
    /// End column (0-indexed).
    pub end_column: usize,
    /// The matched source text.
    pub matched_text: String,
}

/// JSON payload for the `search` command.
#[derive(Debug, Serialize)]
pub struct SearchResult {
    /// The pattern that was searched for.
    pub pattern: String,
    /// All matches found.
    pub matches: Vec<SearchMatchEntry>,
    /// Total number of matches.
    pub total: usize,
}

/// Format `search` results in the requested mode.
pub fn format_search(
    matches: &[SearchMatch],
    pattern: &str,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => format_search_framed(matches),
        OutputMode::Json => format_search_json(matches, pattern, pretty),
        OutputMode::Raw => format_search_raw(matches),
    }
}

/// Format search results as framed output: `@@ file:line:col @@` followed by matched text.
fn format_search_framed(matches: &[SearchMatch]) -> String {
    use std::fmt::Write;
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        matches.len(),
        None,
    );
    let mut content = String::new();
    for (i, m) in matches.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n");
        }
        let _ = write!(
            content,
            "@@ {}:{}:{} @@\n{}",
            m.file.display(),
            m.line,
            m.column,
            m.matched_text,
        );
    }
    prepend_meta_framed(&meta, &content)
}

/// Format `search` results as JSON wrapped in `QueryResult`.
fn format_search_json(matches: &[SearchMatch], pattern: &str, force_pretty: bool) -> String {
    let entries: Vec<SearchMatchEntry> = matches
        .iter()
        .map(|m| SearchMatchEntry {
            file: m.file.display().to_string(),
            line: m.line,
            column: m.column,
            end_line: m.end_line,
            end_column: m.end_column,
            matched_text: m.matched_text.clone(),
        })
        .collect();
    let data = SearchResult {
        pattern: pattern.to_string(),
        total: entries.len(),
        matches: entries,
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format `search` results as raw text — matched text only.
fn format_search_raw(matches: &[SearchMatch]) -> String {
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        matches.len(),
    );
    let mut content = String::new();
    for (i, m) in matches.iter().enumerate() {
        if i > 0 {
            content.push_str("\n\n");
        }
        content.push_str(&m.matched_text);
    }
    prepend_meta_raw(&meta, &content)
}

// ---------------------------------------------------------------------------
// Dead code formatting
// ---------------------------------------------------------------------------

/// JSON payload for the `dead` command.
#[derive(Debug, Serialize)]
pub struct DeadResult {
    /// Symbols with zero references.
    pub dead_symbols: Vec<Symbol>,
    /// Total number of dead symbols.
    pub total: usize,
}

/// Format dead code results in the requested mode.
pub fn format_dead(symbols: &[Symbol], has_public: bool, mode: OutputMode, pretty: bool) -> String {
    match mode {
        OutputMode::Framed => format_dead_framed(symbols, has_public),
        OutputMode::Json => format_dead_json(symbols, has_public, pretty),
        OutputMode::Raw => format_dead_raw(symbols, has_public),
    }
}

/// Format dead symbols as framed output.
fn format_dead_framed(symbols: &[Symbol], has_public: bool) -> String {
    use std::fmt::Write;
    let note = if has_public {
        Some("structural analysis; public symbols may have external callers not visible to cq")
    } else {
        None
    };
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::BestEffort,
        symbols.len(),
        note,
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let vis = if symbol.visibility == Visibility::Public {
            " (pub)"
        } else {
            ""
        };
        let _ = write!(
            content,
            "@@ {}:{}:{} {} {}{} — zero references @@",
            symbol.file.display(),
            symbol.line,
            symbol.column,
            symbol.kind,
            symbol.name,
            vis,
        );
    }
    prepend_meta_framed(&meta, &content)
}

/// Format dead symbols as JSON wrapped in `QueryResult`.
fn format_dead_json(symbols: &[Symbol], has_public: bool, force_pretty: bool) -> String {
    let data = DeadResult {
        total: symbols.len(),
        dead_symbols: symbols.to_vec(),
    };
    let note = if has_public {
        Some(
            "structural analysis; public symbols may have external callers not visible to cq"
                .to_string(),
        )
    } else {
        None
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::BestEffort,
        note,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format dead symbols as raw text.
fn format_dead_raw(symbols: &[Symbol], has_public: bool) -> String {
    use std::fmt::Write;
    let _ = has_public; // Note is only shown in framed/JSON modes
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::BestEffort,
        symbols.len(),
    );
    let mut content = String::new();
    for (i, symbol) in symbols.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
            "{}:{}:{} {} {}",
            symbol.file.display(),
            symbol.line,
            symbol.column,
            symbol.kind,
            symbol.name,
        );
    }
    prepend_meta_raw(&meta, &content)
}

// ---------------------------------------------------------------------------
// Diagnostics formatting
// ---------------------------------------------------------------------------

/// JSON payload for the `diagnostics` command.
#[derive(Debug, Serialize)]
pub struct DiagnosticsResult {
    /// All diagnostics collected.
    pub diagnostics: Vec<Diagnostic>,
    /// Total number of diagnostics.
    pub total: usize,
}

/// Format diagnostics results in the requested mode.
pub fn format_diagnostics(diagnostics: &[Diagnostic], mode: OutputMode, pretty: bool) -> String {
    match mode {
        OutputMode::Framed => format_diagnostics_framed(diagnostics),
        OutputMode::Json => format_diagnostics_json(diagnostics, pretty),
        OutputMode::Raw => format_diagnostics_raw(diagnostics),
    }
}

/// Format diagnostics as framed output.
///
/// Each diagnostic produces one line: `@@ file:line:col severity source message @@`
fn format_diagnostics_framed(diagnostics: &[Diagnostic]) -> String {
    use std::fmt::Write;
    let meta = format_meta_header(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        diagnostics.len(),
        None,
    );
    let mut content = String::new();
    for (i, diag) in diagnostics.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let severity = format_severity(diag.severity);
        let _ = write!(
            content,
            "@@ {}:{}:{} {} {} {} @@",
            diag.file.display(),
            diag.line,
            diag.column,
            severity,
            format_source(diag.source),
            diag.message,
        );
    }
    prepend_meta_framed(&meta, &content)
}

/// Format diagnostics as JSON wrapped in `QueryResult`.
fn format_diagnostics_json(diagnostics: &[Diagnostic], force_pretty: bool) -> String {
    let data = DiagnosticsResult {
        total: diagnostics.len(),
        diagnostics: diagnostics.to_vec(),
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::Exhaustive,
        note: None,
        data,
    };
    serialize_json(&result, force_pretty)
}

/// Format diagnostics as raw text: `file:line:col severity message`.
fn format_diagnostics_raw(diagnostics: &[Diagnostic]) -> String {
    use std::fmt::Write;
    let meta = format_meta_comment(
        Resolution::Syntactic,
        Completeness::Exhaustive,
        diagnostics.len(),
    );
    let mut content = String::new();
    for (i, diag) in diagnostics.iter().enumerate() {
        if i > 0 {
            content.push('\n');
        }
        let _ = write!(
            content,
            "{}:{}:{} {} {}",
            diag.file.display(),
            diag.line,
            diag.column,
            format_severity(diag.severity),
            diag.message,
        );
    }
    prepend_meta_raw(&meta, &content)
}

/// Convert a `DiagnosticSeverity` to its lowercase display string.
fn format_severity(severity: DiagnosticSeverity) -> &'static str {
    match severity {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Information => "info",
        DiagnosticSeverity::Hint => "hint",
    }
}

/// Convert a `DiagnosticSource` to its lowercase display string.
fn format_source(source: codequery_core::DiagnosticSource) -> &'static str {
    match source {
        codequery_core::DiagnosticSource::Syntax => "syntax",
        codequery_core::DiagnosticSource::Lsp => "lsp",
    }
}

// ---------------------------------------------------------------------------
// Callchain formatting
// ---------------------------------------------------------------------------

/// JSON payload for the `callchain` command.
#[derive(Debug, Serialize)]
pub struct CallchainResult {
    /// The root of the call chain tree.
    pub root: codequery_core::CallChainNode,
    /// Maximum depth searched.
    pub depth: usize,
}

/// Format call chain results in the requested mode.
pub fn format_callchain(
    root: &codequery_core::CallChainNode,
    depth: usize,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => {
            let total = count_callchain_nodes(root);
            let meta = format_meta_header(
                Resolution::Syntactic,
                Completeness::BestEffort,
                total,
                Some("recursive caller analysis; may miss indirect calls"),
            );
            let content = format_callchain_framed(root, 0);
            prepend_meta_framed(&meta, &content)
        }
        OutputMode::Json => format_callchain_json(root, depth, pretty),
        OutputMode::Raw => {
            let total = count_callchain_nodes(root);
            let meta = format_meta_comment(Resolution::Syntactic, Completeness::BestEffort, total);
            let content = format_callchain_framed(root, 0);
            prepend_meta_raw(&meta, &content)
        }
    }
}

/// Count total nodes in a call chain tree (including root).
fn count_callchain_nodes(node: &codequery_core::CallChainNode) -> usize {
    1 + node
        .callers
        .iter()
        .map(count_callchain_nodes)
        .sum::<usize>()
}

fn format_callchain_framed(node: &codequery_core::CallChainNode, indent: usize) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    let prefix = "  ".repeat(indent);
    let arrow = if indent > 0 { "← " } else { "" };
    let _ = write!(
        output,
        "{prefix}{arrow}{} ({}) {}:{}",
        node.name,
        node.kind,
        node.file.display(),
        node.line,
    );
    for caller in &node.callers {
        output.push('\n');
        output.push_str(&format_callchain_framed(caller, indent + 1));
    }
    output
}

fn format_callchain_json(
    root: &codequery_core::CallChainNode,
    depth: usize,
    force_pretty: bool,
) -> String {
    let data = CallchainResult {
        root: root.clone(),
        depth,
    };
    let result = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::BestEffort,
        note: Some("recursive caller analysis; may miss indirect calls".to_string()),
        data,
    };
    serialize_json(&result, force_pretty)
}

// ---------------------------------------------------------------------------
// Hierarchy formatting
// ---------------------------------------------------------------------------

/// JSON payload for the `hierarchy` command.
#[derive(Debug, Serialize)]
pub struct HierarchyResult {
    /// The type hierarchy result.
    #[serde(flatten)]
    pub hierarchy: codequery_core::TypeHierarchyResult,
}

/// Format type hierarchy results in the requested mode.
pub fn format_hierarchy(
    result: &codequery_core::TypeHierarchyResult,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => {
            let total = 1 + result.supertypes.len() + result.subtypes.len();
            let meta =
                format_meta_header(Resolution::Syntactic, Completeness::BestEffort, total, None);
            let content = format_hierarchy_framed(result);
            prepend_meta_framed(&meta, &content)
        }
        OutputMode::Json => format_hierarchy_json(result, pretty),
        OutputMode::Raw => {
            let total = 1 + result.supertypes.len() + result.subtypes.len();
            let meta = format_meta_comment(Resolution::Syntactic, Completeness::BestEffort, total);
            let content = format_hierarchy_framed(result);
            prepend_meta_raw(&meta, &content)
        }
    }
}

fn format_hierarchy_framed(result: &codequery_core::TypeHierarchyResult) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    let _ = write!(
        output,
        "@@ {} ({}) {}:{} @@",
        result.target.name,
        result.target.kind,
        result.target.file.display(),
        result.target.line,
    );
    if !result.supertypes.is_empty() {
        output.push_str("\n\nSupertypes:");
        for st in &result.supertypes {
            let _ = write!(
                output,
                "\n  ↑ {} ({}) {}:{}",
                st.name,
                st.kind,
                st.file.display(),
                st.line
            );
        }
    }
    if !result.subtypes.is_empty() {
        output.push_str("\n\nSubtypes:");
        for st in &result.subtypes {
            let _ = write!(
                output,
                "\n  ↓ {} ({}) {}:{}",
                st.name,
                st.kind,
                st.file.display(),
                st.line
            );
        }
    }
    output
}

fn format_hierarchy_json(
    result: &codequery_core::TypeHierarchyResult,
    force_pretty: bool,
) -> String {
    let data = HierarchyResult {
        hierarchy: result.clone(),
    };
    let qr = QueryResult {
        resolution: Resolution::Syntactic,
        completeness: Completeness::BestEffort,
        note: Some("structural AST matching; may miss complex generic relationships".to_string()),
        data,
    };
    serialize_json(&qr, force_pretty)
}

// ---------------------------------------------------------------------------
// Rename formatting
// ---------------------------------------------------------------------------

/// Format rename results in the requested mode.
pub fn format_rename(
    result: &codequery_core::RenameResult,
    mode: OutputMode,
    pretty: bool,
) -> String {
    match mode {
        OutputMode::Framed => {
            let note = if result.resolution == Resolution::Syntactic {
                Some("syntactic name matching; may include false positives")
            } else {
                None
            };
            let meta = format_meta_header(
                result.resolution,
                Completeness::BestEffort,
                result.edits.len(),
                note,
            );
            let content = format_rename_framed(result);
            prepend_meta_framed(&meta, &content)
        }
        OutputMode::Json => format_rename_json(result, pretty),
        OutputMode::Raw => {
            let meta = format_meta_comment(
                result.resolution,
                Completeness::BestEffort,
                result.edits.len(),
            );
            let content = format_rename_framed(result);
            prepend_meta_raw(&meta, &content)
        }
    }
}

fn format_rename_framed(result: &codequery_core::RenameResult) -> String {
    use std::fmt::Write;
    let mut output = String::new();
    if result.applied {
        let _ = write!(
            output,
            "Renamed {} → {} across {} files ({} edits) [{}]",
            result.old_name,
            result.new_name,
            result.files_affected,
            result.edits.len(),
            result.resolution,
        );
    } else {
        let _ = write!(
            output,
            "Rename {} → {}: {} edits across {} files [{} — preview only]\n\
             Run with --apply or use a higher precision tier (daemon) to apply.",
            result.old_name,
            result.new_name,
            result.edits.len(),
            result.files_affected,
            result.resolution,
        );
        // Show a simple diff
        let mut current_file: Option<&Path> = None;
        for edit in &result.edits {
            if current_file != Some(edit.file.as_path()) {
                let _ = write!(
                    output,
                    "\n\n--- {}\n+++ {}",
                    edit.file.display(),
                    edit.file.display()
                );
                current_file = Some(&edit.file);
            }
            let _ = write!(
                output,
                "\n@@ -{}:{} @@\n-{}\n+{}",
                edit.line, edit.column, result.old_name, edit.new_text,
            );
        }
    }
    output
}

fn format_rename_json(result: &codequery_core::RenameResult, force_pretty: bool) -> String {
    let qr = QueryResult {
        resolution: result.resolution,
        completeness: Completeness::BestEffort,
        note: if result.resolution == Resolution::Syntactic {
            Some("syntactic name matching; may include false positives".to_string())
        } else {
            None
        },
        data: result,
    };
    serialize_json(&qr, force_pretty)
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
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=1 @@\n\n"));
        assert!(output.contains("@@ src/lib.rs:1:0 function foo @@"));
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
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=2 @@\n\n"));
        assert!(output
            .contains("@@ src/lib.rs:1:0 function foo @@\n\n@@ src/main.rs:10:4 function bar @@"));
    }

    #[test]
    fn test_def_empty_results_returns_meta_only() {
        let output = format_def_results(&[]);
        assert_eq!(
            output,
            "@@ meta resolution=syntactic completeness=exhaustive total=0 @@"
        );
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
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=2 @@\n\n"));
        assert!(output.contains(
            "@@ src/lib.rs @@\n  greet (function, pub) :10\n  MAX_RETRIES (const, pub) :20"
        ));
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
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=2 @@\n\n"));
        assert!(output.contains("@@ src/lib.rs @@\n  greet (function, pub) :10\n  Router (impl, priv) :20\n    new (method, pub) :22"));
    }

    #[test]
    fn test_outline_file_header_format() {
        let output = format_outline(Path::new("src/api/routes.rs"), &[]);
        assert!(
            output.starts_with("@@ meta resolution=syntactic completeness=exhaustive total=0 @@")
        );
        assert!(output.contains("@@ src/api/routes.rs @@"));
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
    fn test_outline_no_symbols_shows_meta_and_file_header() {
        let output = format_outline(Path::new("src/empty.rs"), &[]);
        assert!(
            output.starts_with("@@ meta resolution=syntactic completeness=exhaustive total=0 @@")
        );
        assert!(output.contains("@@ src/empty.rs @@"));
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
        assert!(output.contains("@@ src/lib.rs:5:8 function indented @@"));
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
    fn test_def_raw_has_meta_comment() {
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
        assert!(output.starts_with("# meta resolution=syntactic completeness=exhaustive total=1\n"));
        assert!(output.contains("src/lib.rs:1:0 function foo"));
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
        assert_eq!(lines.len(), 3); // meta + 2 results
        assert!(lines[0].starts_with("# meta"));
        assert_eq!(lines[1], "src/lib.rs:1:0 function foo");
        assert_eq!(lines[2], "src/main.rs:10:0 function foo");
    }

    #[test]
    fn test_outline_raw_has_meta_comment() {
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
        assert!(output.starts_with("# meta resolution=syntactic completeness=exhaustive total=1\n"));
        assert!(output.contains("greet (function, pub) :10"));
    }

    #[test]
    fn test_outline_raw_empty_symbols_has_meta_only() {
        let output = format_outline_output(Path::new("src/lib.rs"), &[], OutputMode::Raw, false);
        assert_eq!(
            output,
            "# meta resolution=syntactic completeness=exhaustive total=0"
        );
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
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=1 @@\n\n"));
        assert!(output.contains("@@ src/lib.rs:9:0 function greet @@\n"));
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
    fn test_body_framed_empty_results_returns_meta_only() {
        let output = format_body(&[], "missing", OutputMode::Framed, false);
        assert_eq!(
            output,
            "@@ meta resolution=syntactic completeness=exhaustive total=0 @@"
        );
    }

    #[test]
    fn test_body_framed_symbol_without_body_shows_meta_and_header() {
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
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=1 @@\n\n"));
        assert!(output.contains("@@ src/lib.rs:1:0 function foo @@"));
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
    fn test_body_raw_has_meta_and_body_text() {
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
        assert!(output.starts_with("# meta resolution=syntactic completeness=exhaustive total=1\n"));
        assert!(output.contains("pub fn greet() {\n    println!(\"hello\");\n}"));
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
        assert!(output.starts_with("# meta resolution=syntactic completeness=exhaustive total=2\n"));
        assert!(output.contains("fn foo() {}\n\nfn foo() { 42 }"));
    }

    #[test]
    fn test_body_raw_empty_results_returns_meta_only() {
        let output = format_body(&[], "missing", OutputMode::Raw, false);
        assert_eq!(
            output,
            "# meta resolution=syntactic completeness=exhaustive total=0"
        );
    }

    #[test]
    fn test_body_raw_symbol_without_body_returns_meta_only() {
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
        // Body is None, so content is empty, but meta still present
        assert!(output.starts_with("# meta resolution=syntactic completeness=exhaustive total=1"));
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
    fn test_imports_framed_produces_meta_and_file_header_and_entries() {
        let imports = vec![
            make_import("std::collections::HashMap", "use", 1, true),
            make_import("crate::models::User", "use", 2, false),
        ];
        let output =
            format_imports_output(Path::new("src/lib.rs"), &imports, OutputMode::Framed, false);
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=2 @@\n\n"));
        assert!(output.contains("@@ src/lib.rs @@"));
        assert!(output.contains("@@ src/lib.rs:1 use std::collections::HashMap @@"));
        assert!(output.contains("@@ src/lib.rs:2 use crate::models::User @@"));
    }

    #[test]
    fn test_imports_framed_empty_shows_meta_and_file_header() {
        let output =
            format_imports_output(Path::new("src/empty.rs"), &[], OutputMode::Framed, false);
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=0 @@\n\n"));
        assert!(output.contains("@@ src/empty.rs @@"));
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
    fn test_imports_raw_has_meta_comment() {
        let imports = vec![
            make_import("std::io", "use", 1, true),
            make_import("crate::models", "use", 2, false),
        ];
        let output =
            format_imports_output(Path::new("src/lib.rs"), &imports, OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 3); // meta + 2 imports
        assert!(lines[0].starts_with("# meta"));
        assert_eq!(lines[1], ":1 use std::io");
        assert_eq!(lines[2], ":2 use crate::models");
    }

    #[test]
    fn test_imports_raw_empty_has_meta_only() {
        let output = format_imports_output(Path::new("src/lib.rs"), &[], OutputMode::Raw, false);
        assert_eq!(
            output,
            "# meta resolution=syntactic completeness=exhaustive total=0"
        );
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
    fn test_context_framed_without_body_shows_meta_and_header() {
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
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=1 @@\n\n"));
        assert!(output.contains("@@ src/lib.rs:10:0 function foo (contains line 12) @@"));
        // Meta line + blank line + header = 3 lines, no body
        assert_eq!(output.lines().count(), 3);
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
    fn test_context_raw_no_symbol_has_meta_only() {
        let output =
            format_context_output(None, 5, Path::new("src/lib.rs"), OutputMode::Raw, false);
        assert_eq!(
            output,
            "# meta resolution=syntactic completeness=exhaustive total=0"
        );
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

    // -----------------------------------------------------------------------
    // Search formatting
    // -----------------------------------------------------------------------

    fn make_search_match(
        file: &str,
        line: usize,
        column: usize,
        matched_text: &str,
    ) -> SearchMatch {
        SearchMatch {
            file: PathBuf::from(file),
            line,
            column,
            end_line: line,
            end_column: column + matched_text.len(),
            matched_text: matched_text.to_string(),
            pattern: "test_pattern".to_string(),
        }
    }

    #[test]
    fn test_search_framed_single_match() {
        let matches = vec![make_search_match("src/main.rs", 5, 3, "fn main() {}")];
        let output = format_search(&matches, "fn $NAME() {}", OutputMode::Framed, false);
        assert!(output.contains("@@ src/main.rs:5:3 @@"));
        assert!(output.contains("fn main() {}"));
    }

    #[test]
    fn test_search_framed_multiple_matches() {
        let matches = vec![
            make_search_match("src/a.rs", 0, 0, "fn alpha() {}"),
            make_search_match("src/b.rs", 2, 4, "fn beta() {}"),
        ];
        let output = format_search(&matches, "fn $NAME() {}", OutputMode::Framed, false);
        assert!(output.contains("@@ src/a.rs:0:0 @@"));
        assert!(output.contains("fn alpha() {}"));
        assert!(output.contains("@@ src/b.rs:2:4 @@"));
        assert!(output.contains("fn beta() {}"));
    }

    #[test]
    fn test_search_framed_empty_matches_has_meta_only() {
        let matches: Vec<SearchMatch> = vec![];
        let output = format_search(&matches, "fn $NAME() {}", OutputMode::Framed, false);
        assert_eq!(
            output,
            "@@ meta resolution=syntactic completeness=exhaustive total=0 @@"
        );
    }

    #[test]
    fn test_search_raw_has_meta_and_matched_text() {
        let matches = vec![
            make_search_match("src/a.rs", 0, 0, "fn alpha() {}"),
            make_search_match("src/b.rs", 2, 4, "fn beta() {}"),
        ];
        let output = format_search(&matches, "fn $NAME() {}", OutputMode::Raw, false);
        assert!(output.starts_with("# meta resolution=syntactic completeness=exhaustive total=2\n"));
        assert!(output.contains("fn alpha() {}\n\nfn beta() {}"));
    }

    #[test]
    fn test_search_json_mode() {
        let matches = vec![make_search_match("src/main.rs", 5, 3, "fn main() {}")];
        let output = format_search(&matches, "fn $NAME() {}", OutputMode::Json, true);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        // QueryResult flattens data fields to top level
        assert_eq!(parsed["pattern"], "fn $NAME() {}");
        assert_eq!(parsed["total"], 1);
        assert_eq!(parsed["matches"][0]["file"], "src/main.rs");
        assert_eq!(parsed["matches"][0]["line"], 5);
        assert_eq!(parsed["matches"][0]["column"], 3);
        assert_eq!(parsed["matches"][0]["matched_text"], "fn main() {}");
        assert_eq!(parsed["resolution"], "syntactic");
        assert_eq!(parsed["completeness"], "exhaustive");
    }

    #[test]
    fn test_search_json_empty_matches() {
        let matches: Vec<SearchMatch> = vec![];
        let output = format_search(&matches, "fn $NAME() {}", OutputMode::Json, true);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["total"], 0);
        assert!(parsed["matches"].as_array().unwrap().is_empty());
    }

    // -----------------------------------------------------------------------
    // Diagnostics formatting
    // -----------------------------------------------------------------------

    fn make_diagnostic(file: &str, line: usize, column: usize, message: &str) -> Diagnostic {
        use codequery_core::DiagnosticSource;
        Diagnostic {
            file: PathBuf::from(file),
            line,
            column,
            end_line: line,
            end_column: column + 5,
            severity: DiagnosticSeverity::Error,
            message: message.to_string(),
            source: DiagnosticSource::Syntax,
            code: None,
        }
    }

    #[test]
    fn test_format_diagnostics_framed_empty_returns_meta_only() {
        let output = format_diagnostics(&[], OutputMode::Framed, false);
        assert_eq!(
            output,
            "@@ meta resolution=syntactic completeness=exhaustive total=0 @@"
        );
    }

    #[test]
    fn test_format_diagnostics_framed_single_diagnostic() {
        let diag = make_diagnostic("src/main.rs", 10, 4, "unexpected syntax");
        let output = format_diagnostics(&[diag], OutputMode::Framed, false);
        assert!(output
            .starts_with("@@ meta resolution=syntactic completeness=exhaustive total=1 @@\n\n"));
        assert!(output.contains("src/main.rs:10:4"));
        assert!(output.contains("error"));
        assert!(output.contains("syntax"));
        assert!(output.contains("unexpected syntax"));
        assert!(output.ends_with("@@"));
    }

    #[test]
    fn test_format_diagnostics_framed_multiple_separated_by_newline() {
        let diags = vec![
            make_diagnostic("src/a.rs", 1, 0, "unexpected syntax"),
            make_diagnostic("src/b.rs", 5, 2, "missing }"),
        ];
        let output = format_diagnostics(&diags, OutputMode::Framed, false);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 4); // meta + blank + 2 diagnostics
        assert!(lines[0].starts_with("@@ meta"));
        assert!(lines[2].contains("src/a.rs"));
        assert!(lines[3].contains("src/b.rs"));
    }

    #[test]
    fn test_format_diagnostics_raw_empty_returns_meta_only() {
        let output = format_diagnostics(&[], OutputMode::Raw, false);
        assert_eq!(
            output,
            "# meta resolution=syntactic completeness=exhaustive total=0"
        );
    }

    #[test]
    fn test_format_diagnostics_raw_no_framing_delimiters() {
        let diag = make_diagnostic("src/main.rs", 3, 0, "unexpected syntax");
        let output = format_diagnostics(&[diag], OutputMode::Raw, false);
        assert!(!output.contains("@@"));
        assert!(output.contains("src/main.rs:3:0"));
        assert!(output.contains("error"));
        assert!(output.contains("unexpected syntax"));
    }

    #[test]
    fn test_format_diagnostics_json_empty_produces_valid_json() {
        let output = format_diagnostics(&[], OutputMode::Json, true);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["total"], 0);
        assert_eq!(parsed["resolution"], "syntactic");
        assert_eq!(parsed["completeness"], "exhaustive");
        assert!(parsed["diagnostics"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_format_diagnostics_json_with_diagnostics() {
        let diag = make_diagnostic("src/main.rs", 7, 2, "missing ;");
        let output = format_diagnostics(&[diag], OutputMode::Json, true);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["total"], 1);
        assert_eq!(parsed["diagnostics"][0]["file"], "src/main.rs");
        assert_eq!(parsed["diagnostics"][0]["line"], 7);
        assert_eq!(parsed["diagnostics"][0]["column"], 2);
        assert_eq!(parsed["diagnostics"][0]["severity"], "error");
        assert_eq!(parsed["diagnostics"][0]["source"], "syntax");
        assert_eq!(parsed["diagnostics"][0]["message"], "missing ;");
    }
}
