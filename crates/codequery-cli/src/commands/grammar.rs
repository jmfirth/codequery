//! Grammar package management commands: list, install, update, remove, info.
//!
//! Manages installable language grammar packages that extend cq beyond its
//! 16 built-in languages. Packages are stored in `~/.local/share/cq/languages/`.

use std::path::Path;

use crate::args::ExitCode;
use serde::Deserialize;

/// The baked-in language registry, compiled into the binary.
const REGISTRY_JSON: &str = include_str!("../../../../languages/registry.json");

/// Remote registry URL (CORS-enabled, auto-updates on push to main).
/// Used by `cq grammar update` to fetch the latest registry, and available
/// for browser/WASM integrations that need CORS access.
const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/jmfirth/codequery/main/languages/registry.json";

/// Base URL for GitHub release artifacts (grammar packages, binaries).
/// CORS-enabled — works from browsers.
const RELEASE_BASE_URL: &str = "https://github.com/jmfirth/codequery/releases/download";

/// Languages compiled into the binary with the `common` feature preset.
///
/// This list reflects the default `common` feature. Languages outside this set
/// (C#, Swift, Kotlin, Scala, Zig, Lua, Bash) can be enabled at compile time
/// via individual `lang-*` features, or installed as WASM plugins at runtime.
const BUILTIN_LANGUAGES: &[&str] = &[
    "rust",
    "typescript",
    "javascript",
    "python",
    "go",
    "c",
    "cpp",
    "java",
    "ruby",
    "php",
    "html",
    "css",
    "json",
    "yaml",
    "toml",
];

/// Top-level registry structure.
#[derive(Debug, Deserialize)]
pub struct Registry {
    /// Schema version.
    #[allow(dead_code)]
    pub version: String,
    /// Available language packages.
    pub languages: Vec<LanguagePackage>,
}

/// A single language package entry in the registry.
#[derive(Debug, Deserialize)]
pub struct LanguagePackage {
    /// Machine name (e.g., "elixir").
    pub name: String,
    /// Human-readable name (e.g., "Elixir").
    pub display_name: String,
    /// Short description (e.g., "Elixir/Phoenix").
    pub description: String,
    /// File extensions this language covers.
    pub extensions: Vec<String>,
    /// Available capabilities: grammar, extract, lsp.
    pub capabilities: Vec<String>,
    /// LSP server command, if the package supports LSP.
    #[serde(default)]
    pub lsp_server: Option<String>,
    /// GitHub repo containing the tree-sitter grammar (e.g., "tree-sitter/tree-sitter-bash").
    /// Used by the grammar packaging pipeline to build WASM grammars.
    #[serde(default)]
    #[allow(dead_code)]
    pub grammar_repo: Option<String>,
}

/// Parse the baked-in registry JSON.
///
/// Falls back to a cached registry at `~/.cache/cq/registry.json` if it exists
/// and is newer (from a `cq grammar update` that fetched the latest).
///
/// # Errors
///
/// Returns an error if the registry JSON cannot be parsed.
pub fn load_registry() -> anyhow::Result<Registry> {
    // Check for a cached registry first
    if let Some(cache_path) = codequery_core::dirs::registry_cache_path() {
        if cache_path.exists() {
            if let Ok(cached) = std::fs::read_to_string(&cache_path) {
                if let Ok(registry) = serde_json::from_str::<Registry>(&cached) {
                    return Ok(registry);
                }
            }
        }
    }

    let registry: Registry = serde_json::from_str(REGISTRY_JSON)
        .map_err(|e| anyhow::anyhow!("failed to parse language registry: {e}"))?;
    Ok(registry)
}

/// Find installed language packages by scanning the languages directory.
fn find_installed(languages_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(languages_dir) else {
        return Vec::new();
    };
    let mut installed = Vec::new();
    for entry in entries.flatten() {
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                installed.push(name.to_string());
            }
        }
    }
    installed.sort();
    installed
}

/// `cq grammar list` — show installed, available, and built-in languages.
///
/// # Errors
///
/// Returns an error if the registry cannot be loaded.
pub fn run_list() -> anyhow::Result<ExitCode> {
    let registry = load_registry()?;
    let languages_dir = codequery_core::dirs::languages_dir();
    let installed = languages_dir
        .as_ref()
        .map(|d| find_installed(d))
        .unwrap_or_default();

    // Installed section
    println!("Installed:");
    if installed.is_empty() {
        println!("  (none)");
    } else {
        for name in &installed {
            if let Some(pkg) = registry.languages.iter().find(|l| l.name == *name) {
                let caps = pkg.capabilities.join(" + ");
                println!("  {:<12}{} ({caps})", name, pkg.description);
            } else {
                println!("  {name}");
            }
        }
    }

    // Available section (not yet installed)
    println!();
    println!("Available:");
    let available: Vec<&LanguagePackage> = registry
        .languages
        .iter()
        .filter(|l| !installed.contains(&l.name))
        .collect();
    if available.is_empty() {
        println!("  (all packages installed)");
    } else {
        for pkg in &available {
            let caps = pkg.capabilities.join(" + ");
            println!("  {:<12}{} ({caps})", pkg.name, pkg.description);
        }
    }

    // Built-in section
    println!();
    println!("Built-in ({} languages):", BUILTIN_LANGUAGES.len());
    // Print in rows of roughly 8
    let chunks: Vec<&[&str]> = BUILTIN_LANGUAGES.chunks(8).collect();
    for chunk in chunks {
        let line = chunk.join(", ");
        println!("  {line}");
    }

    Ok(ExitCode::Success)
}

/// `cq grammar install <lang>` — install a language package.
///
/// Downloads the language package from GitHub releases (placeholder for now).
/// Creates the directory structure under `~/.local/share/cq/languages/<lang>/`.
///
/// # Errors
///
/// Returns an error if the language is unknown, already a built-in, already
/// installed, or the directory cannot be created.
pub fn run_install(language: &str) -> anyhow::Result<ExitCode> {
    // Reject built-in languages
    if BUILTIN_LANGUAGES.contains(&language) {
        eprintln!("{language} is a built-in language and does not need installation");
        return Ok(ExitCode::UsageError);
    }

    let registry = load_registry()?;
    let Some(_pkg) = registry.languages.iter().find(|l| l.name == language) else {
        eprintln!("unknown language: {language}");
        eprintln!(
            "available: {}",
            registry
                .languages
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        return Ok(ExitCode::UsageError);
    };

    let languages_dir = codequery_core::dirs::languages_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine languages directory"))?;

    let pkg_dir = languages_dir.join(language);
    if pkg_dir.exists() {
        eprintln!("{language} is already installed at {}", pkg_dir.display());
        return Ok(ExitCode::Success);
    }

    let version = env!("CARGO_PKG_VERSION");
    let archive_url = format!("{RELEASE_BASE_URL}/v{version}/lang-{language}.tar.gz");

    eprintln!("Downloading {language} language package for cq v{version}...");
    eprintln!("  from: {archive_url}");

    // Try to download via curl (available on macOS/Linux, most CI)
    let output = std::process::Command::new("curl")
        .args(["-fsSL", "--max-time", "30", &archive_url, "-o", "-"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output();

    match output {
        Ok(result) if result.status.success() => {
            // Create directory and extract tarball
            std::fs::create_dir_all(&pkg_dir)
                .map_err(|e| anyhow::anyhow!("failed to create directory: {e}"))?;

            let tar_output = std::process::Command::new("tar")
                .args(["xzf", "-", "-C"])
                .arg(&pkg_dir)
                .stdin(std::process::Stdio::piped())
                .spawn()
                .and_then(|mut child| {
                    use std::io::Write;
                    if let Some(ref mut stdin) = child.stdin {
                        stdin.write_all(&result.stdout)?;
                    }
                    child.wait()
                });

            match tar_output {
                Ok(status) if status.success() => {
                    // Verify we got a real grammar
                    let grammar_path = pkg_dir.join("grammar.wasm");
                    if grammar_path.exists()
                        && std::fs::metadata(&grammar_path)
                            .map(|m| m.len() > 100)
                            .unwrap_or(false)
                    {
                        eprintln!("  grammar.wasm    \u{2713}");
                    }
                    if pkg_dir.join("extract.toml").exists() {
                        eprintln!("  extract.toml    \u{2713}");
                    }
                    if pkg_dir.join("lsp.toml").exists() {
                        eprintln!("  lsp.toml        \u{2713}");
                    }
                    if pkg_dir.join("stack-graphs.tsg").exists() {
                        eprintln!("  stack-graphs.tsg \u{2713}");
                    }
                    eprintln!("Installed to {}/", pkg_dir.display());
                    Ok(ExitCode::Success)
                }
                _ => {
                    // Clean up failed extraction
                    let _ = std::fs::remove_dir_all(&pkg_dir);
                    eprintln!("error: failed to extract language package for {language}");
                    Ok(ExitCode::ProjectError)
                }
            }
        }
        _ => {
            eprintln!(
                "error: failed to download {language} language package.\n\
                 Release v{version} may not be published yet.\n\
                 \n\
                 The 15 built-in languages work without installation:\n\
                 Python, TypeScript, JavaScript, Rust, Go, C, C++, Java,\n\
                 Ruby, PHP, HTML, CSS, JSON, YAML, TOML\n\
                 \n\
                 If you have a language server installed, use --semantic\n\
                 for {language} support without a grammar package."
            );
            Ok(ExitCode::ProjectError)
        }
    }
}

/// `cq grammar install --all` — install all available packages from the registry.
///
/// # Errors
///
/// Returns an error if installation of any package fails.
pub fn run_install_all() -> anyhow::Result<ExitCode> {
    let registry = load_registry()?;
    let languages_dir = codequery_core::dirs::languages_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine languages directory"))?;

    let mut installed_count = 0;
    for pkg in &registry.languages {
        // Skip built-in languages and already-installed packages
        if BUILTIN_LANGUAGES.contains(&pkg.name.as_str()) {
            continue;
        }
        let pkg_dir = languages_dir.join(&pkg.name);
        if pkg_dir.exists() {
            continue;
        }
        run_install(&pkg.name)?;
        installed_count += 1;
    }

    if installed_count == 0 {
        eprintln!("all available packages are already installed");
    } else {
        eprintln!("Installed {installed_count} package(s)");
    }
    Ok(ExitCode::Success)
}

/// `cq grammar update` — re-download all installed packages for the current version.
///
/// # Errors
///
/// Returns an error if installation of any package fails.
pub fn run_update() -> anyhow::Result<ExitCode> {
    // Refresh the cached registry from GitHub
    eprintln!("Refreshing language registry...");
    if let Some(cache_path) = codequery_core::dirs::registry_cache_path() {
        let output = std::process::Command::new("curl")
            .args(["-fsSL", "--max-time", "10", REGISTRY_URL, "-o"])
            .arg(&cache_path)
            .output();
        match output {
            Ok(result) if result.status.success() => {
                eprintln!("  registry updated from {REGISTRY_URL}");
            }
            _ => {
                eprintln!("  registry refresh failed (using baked-in version)");
            }
        }
    }

    let languages_dir = codequery_core::dirs::languages_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine languages directory"))?;

    let installed = find_installed(&languages_dir);
    if installed.is_empty() {
        eprintln!("No language packages installed. Use: cq grammar install <lang>");
        return Ok(ExitCode::Success);
    }

    let version = env!("CARGO_PKG_VERSION");
    eprintln!(
        "Updating {} package(s) for cq v{version}...",
        installed.len()
    );

    for name in &installed {
        // Remove and reinstall
        let pkg_dir = languages_dir.join(name);
        if pkg_dir.exists() {
            std::fs::remove_dir_all(&pkg_dir)
                .map_err(|e| anyhow::anyhow!("failed to remove {}: {e}", pkg_dir.display()))?;
        }
        run_install(name)?;
    }

    eprintln!("Update complete");
    Ok(ExitCode::Success)
}

/// `cq grammar remove <lang>` — remove an installed language package.
///
/// # Errors
///
/// Returns an error if the directory cannot be removed.
pub fn run_remove(language: &str) -> anyhow::Result<ExitCode> {
    if BUILTIN_LANGUAGES.contains(&language) {
        eprintln!("{language} is a built-in language and cannot be removed");
        return Ok(ExitCode::UsageError);
    }

    let languages_dir = codequery_core::dirs::languages_dir()
        .ok_or_else(|| anyhow::anyhow!("cannot determine languages directory"))?;

    let pkg_dir = languages_dir.join(language);
    if !pkg_dir.exists() {
        eprintln!("{language} is not installed");
        return Ok(ExitCode::Success);
    }

    std::fs::remove_dir_all(&pkg_dir)
        .map_err(|e| anyhow::anyhow!("failed to remove {}: {e}", pkg_dir.display()))?;

    eprintln!("Removed {language}");
    Ok(ExitCode::Success)
}

/// `cq grammar info <lang>` — show details about a language package.
///
/// # Errors
///
/// Returns an error if the registry cannot be loaded.
pub fn run_info(language: &str) -> anyhow::Result<ExitCode> {
    // Check if it's a built-in
    if BUILTIN_LANGUAGES.contains(&language) {
        println!("Language:     {language}");
        println!("Type:         built-in");
        println!("Status:       always available");
        return Ok(ExitCode::Success);
    }

    let registry = load_registry()?;
    let Some(pkg) = registry.languages.iter().find(|l| l.name == language) else {
        eprintln!("unknown language: {language}");
        return Ok(ExitCode::Success);
    };

    let languages_dir = codequery_core::dirs::languages_dir();
    let installed = languages_dir
        .as_ref()
        .is_some_and(|d| d.join(language).exists());

    println!("Language:     {}", pkg.display_name);
    println!("Description:  {}", pkg.description);
    println!("Extensions:   {}", pkg.extensions.join(", "));
    println!("Capabilities: {}", pkg.capabilities.join(", "));
    if let Some(ref lsp) = pkg.lsp_server {
        println!("LSP server:   {lsp}");
    }
    println!(
        "Status:       {}",
        if installed {
            "installed"
        } else {
            "not installed"
        }
    );

    Ok(ExitCode::Success)
}

/// `cq grammar validate <lang>` — validate a grammar's extract.toml.
///
/// Loads the grammar (compiled-in or WASM) and the extract.toml, then
/// checks that all queries compile against the grammar and all symbol
/// kinds are recognized.
///
/// # Errors
///
/// Returns an error if the grammar or config cannot be loaded.
pub fn run_validate(language: &str) -> anyhow::Result<ExitCode> {
    match validate_language(language)? {
        ValidateResult::Builtin => {
            println!("{language}: ok (compiled-in extractor)");
            Ok(ExitCode::Success)
        }
        ValidateResult::Checked(errors, warnings) => {
            if errors.is_empty() && warnings.is_empty() {
                println!("{language}: ok");
                return Ok(ExitCode::Success);
            }
            for w in &warnings {
                println!("{language}: warning: {w}");
            }
            for e in &errors {
                println!("{language}: error: {e}");
            }
            let status = if errors.is_empty() {
                "warnings"
            } else {
                "FAILED"
            };
            println!(
                "{language}: {status} ({} errors, {} warnings)",
                errors.len(),
                warnings.len()
            );
            Ok(ExitCode::Success)
        }
    }
}

/// `cq grammar validate --all` — validate all installed grammars.
///
/// # Errors
///
/// Returns an error if the registry cannot be loaded.
pub fn run_validate_all() -> anyhow::Result<ExitCode> {
    let registry = load_registry()?;
    let languages_dir = codequery_core::dirs::languages_dir();
    let installed = languages_dir
        .as_ref()
        .map(|d| find_installed(d))
        .unwrap_or_default();

    // Validate built-in languages + all installed
    let mut all_langs: Vec<String> = BUILTIN_LANGUAGES.iter().map(|s| (*s).to_string()).collect();
    for name in &installed {
        if !all_langs.contains(name) {
            all_langs.push(name.clone());
        }
    }
    // Also include registry languages not yet installed but available in source
    for pkg in &registry.languages {
        if !all_langs.contains(&pkg.name) {
            // Only validate if we can load the grammar (installed or built-in)
            if codequery_core::Language::from_name(&pkg.name).is_some()
                || installed.contains(&pkg.name)
            {
                all_langs.push(pkg.name.clone());
            }
        }
    }

    all_langs.sort();

    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut failed = Vec::new();
    let mut warned = Vec::new();

    for lang in &all_langs {
        match validate_language(lang) {
            Ok(ValidateResult::Builtin) => {
                println!("{lang}: ok (compiled-in extractor)");
            }
            Ok(ValidateResult::Checked(errors, warnings)) => {
                if errors.is_empty() && warnings.is_empty() {
                    println!("{lang}: ok");
                } else {
                    for w in &warnings {
                        println!("{lang}: warning: {w}");
                    }
                    for e in &errors {
                        println!("{lang}: error: {e}");
                    }
                    total_errors += errors.len();
                    total_warnings += warnings.len();
                    if errors.is_empty() {
                        warned.push(lang.as_str());
                    } else {
                        failed.push(lang.as_str());
                    }
                }
            }
            Err(e) => {
                println!("{lang}: skip ({e})");
            }
        }
    }

    println!();
    println!(
        "Validated {} languages: {} ok, {} warned, {} failed",
        all_langs.len(),
        all_langs.len() - failed.len() - warned.len(),
        warned.len(),
        failed.len(),
    );
    if !failed.is_empty() {
        println!("Failed: {}", failed.join(", "));
    }
    if total_errors > 0 {
        println!("Total: {total_errors} errors, {total_warnings} warnings");
    }

    Ok(ExitCode::Success)
}

/// Validation result for a language.
enum ValidateResult {
    /// Validated with errors and warnings.
    Checked(Vec<String>, Vec<String>),
    /// Language has a builtin extractor but no extract.toml to validate.
    Builtin,
}

/// Validate a single language's extract.toml and stack-graphs.tsg against its grammar.
fn validate_language(name: &str) -> anyhow::Result<ValidateResult> {
    // Load extract.toml — if not found, check for compiled-in extractor
    let Ok(config_str) = load_extract_toml(name) else {
        // If the language has a compiled-in extractor, that's fine
        if codequery_core::Language::from_name(name).is_some() {
            return Ok(ValidateResult::Builtin);
        }
        anyhow::bail!("no extract.toml found for '{name}'");
    };

    let config = codequery_core::load_extract_config(&config_str)
        .map_err(|e| anyhow::anyhow!("invalid extract.toml: {e}"))?;

    // Load grammar
    let ts_lang = codequery_parse::grammar_for_name(name)
        .map_err(|e| anyhow::anyhow!("grammar not available: {e}"))?;

    // Check symbol kinds
    let mut warnings = Vec::new();
    for rule in &config.symbols {
        if codequery_core::extract_config::parse_symbol_kind(&rule.kind).is_none() {
            warnings.push(format!("unknown symbol kind '{}'", rule.kind));
        }
    }

    // Compile queries
    let mut errors: Vec<String> = codequery_parse::validate_config(&config, &ts_lang)
        .into_iter()
        .map(|(i, msg)| {
            let kind = &config.symbols[i].kind;
            format!("rule {i} ({kind}): {msg}")
        })
        .collect();

    // Validate TSG rules if present — try loading via the plugin rules system
    if load_tsg_file(name).is_ok() {
        // Attempt to load the full stack graph language (grammar + TSG compilation)
        if let Some(Err(e)) = codequery_resolve::rules::get_stack_graph_language(name) {
            errors.push(format!("stack-graphs.tsg: {e}"));
        }
    }

    Ok(ValidateResult::Checked(errors, warnings))
}

/// Load stack-graphs.tsg for a language from source tree or installed package.
fn load_tsg_file(name: &str) -> anyhow::Result<String> {
    // Try source tree first (crates/codequery-resolve/tsg/<name>/stack-graphs.tsg)
    let source_path = std::path::Path::new("crates")
        .join("codequery-resolve")
        .join("tsg")
        .join(name)
        .join("stack-graphs.tsg");
    if source_path.exists() {
        return std::fs::read_to_string(&source_path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", source_path.display()));
    }

    // Fall back to installed package
    if let Some(dir) = codequery_core::dirs::languages_dir() {
        let path = dir.join(name).join("stack-graphs.tsg");
        if path.exists() {
            return std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()));
        }
    }

    anyhow::bail!("no stack-graphs.tsg found for '{name}'")
}

/// Load extract.toml for a language from source tree or installed package.
///
/// Source tree takes priority during development so edits are validated
/// immediately without reinstalling the grammar package.
fn load_extract_toml(name: &str) -> anyhow::Result<String> {
    // Try source tree first (for development — languages/<name>/extract.toml)
    let source_path = std::path::Path::new("languages")
        .join(name)
        .join("extract.toml");
    if source_path.exists() {
        return std::fs::read_to_string(&source_path)
            .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", source_path.display()));
    }

    // Fall back to installed package
    if let Some(dir) = codequery_core::dirs::languages_dir() {
        let path = dir.join(name).join("extract.toml");
        if path.exists() {
            return std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("cannot read {}: {e}", path.display()));
        }
    }

    anyhow::bail!("no extract.toml found for '{name}'")
}

/// Look up a file extension in the registry and return the package name if found.
///
/// Used for suggesting installations when a file's language is unknown.
#[must_use]
pub fn find_package_for_extension(ext: &str) -> Option<String> {
    let registry = load_registry().ok()?;
    let dot_ext = if ext.starts_with('.') {
        ext.to_string()
    } else {
        format!(".{ext}")
    };

    for pkg in &registry.languages {
        if pkg.extensions.contains(&dot_ext) {
            return Some(pkg.name.clone());
        }
    }
    None
}

/// Look up a language name or file extension in the registry.
///
/// First checks by name, then by extension. Used for suggesting package
/// installation when `--lang` specifies an unrecognized language.
#[must_use]
pub fn find_package_for_extension_or_name(query: &str) -> Option<String> {
    let registry = load_registry().ok()?;

    // Check by name first
    if let Some(pkg) = registry.languages.iter().find(|l| l.name == query) {
        return Some(pkg.name.clone());
    }

    // Then check by extension
    find_package_for_extension(query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_parses_from_baked_json() {
        let registry = load_registry().unwrap();
        assert_eq!(registry.version, "1");
        assert!(!registry.languages.is_empty());
    }

    #[test]
    fn test_registry_contains_elixir() {
        let registry = load_registry().unwrap();
        let elixir = registry.languages.iter().find(|l| l.name == "elixir");
        assert!(elixir.is_some());
        let elixir = elixir.unwrap();
        assert_eq!(elixir.display_name, "Elixir");
        assert!(elixir.extensions.contains(&".ex".to_string()));
        assert!(elixir.capabilities.contains(&"lsp".to_string()));
        assert_eq!(elixir.lsp_server.as_deref(), Some("elixir-ls"));
    }

    #[test]
    fn test_registry_contains_all_expected_languages() {
        let registry = load_registry().unwrap();
        let names: Vec<&str> = registry.languages.iter().map(|l| l.name.as_str()).collect();
        // Non-common compiled languages (now installable)
        for expected in &["csharp", "swift", "kotlin", "scala", "zig", "lua", "bash"] {
            assert!(names.contains(expected), "missing language: {expected}");
        }
        // WASM-only languages
        for expected in &[
            "elixir", "haskell", "dart", "sql", "ocaml", "r", "perl", "clojure", "erlang", "julia",
        ] {
            assert!(names.contains(expected), "missing language: {expected}");
        }
    }

    #[test]
    fn test_registry_sql_has_no_lsp() {
        let registry = load_registry().unwrap();
        let sql = registry.languages.iter().find(|l| l.name == "sql").unwrap();
        assert!(sql.lsp_server.is_none());
        assert!(!sql.capabilities.contains(&"lsp".to_string()));
    }

    #[test]
    fn test_find_package_for_extension_known() {
        assert_eq!(
            find_package_for_extension(".ex"),
            Some("elixir".to_string())
        );
        assert_eq!(
            find_package_for_extension("exs"),
            Some("elixir".to_string())
        );
        assert_eq!(
            find_package_for_extension(".hs"),
            Some("haskell".to_string())
        );
        assert_eq!(
            find_package_for_extension(".dart"),
            Some("dart".to_string())
        );
        assert_eq!(find_package_for_extension(".sql"), Some("sql".to_string()));
    }

    #[test]
    fn test_find_package_for_extension_unknown() {
        assert_eq!(find_package_for_extension(".xyz_unknown"), None);
    }

    #[test]
    fn test_find_package_for_extension_builtin_in_registry() {
        // Built-in languages are now in the registry (unified extension resolver)
        assert_eq!(find_package_for_extension(".rs"), Some("rust".to_string()));
    }

    #[test]
    fn test_find_package_for_extension_or_name_by_name() {
        assert_eq!(
            find_package_for_extension_or_name("elixir"),
            Some("elixir".to_string())
        );
        assert_eq!(
            find_package_for_extension_or_name("haskell"),
            Some("haskell".to_string())
        );
    }

    #[test]
    fn test_find_package_for_extension_or_name_by_extension() {
        assert_eq!(
            find_package_for_extension_or_name(".dart"),
            Some("dart".to_string())
        );
    }

    #[test]
    fn test_find_package_for_extension_or_name_unknown() {
        assert_eq!(find_package_for_extension_or_name("klingon"), None);
    }

    #[test]
    fn test_builtin_languages_list() {
        assert!(BUILTIN_LANGUAGES.contains(&"rust"));
        assert!(BUILTIN_LANGUAGES.contains(&"python"));
        assert!(BUILTIN_LANGUAGES.contains(&"typescript"));
        assert!(BUILTIN_LANGUAGES.contains(&"html"));
        assert!(BUILTIN_LANGUAGES.contains(&"css"));
        assert!(BUILTIN_LANGUAGES.contains(&"json"));
        assert!(BUILTIN_LANGUAGES.contains(&"yaml"));
        assert!(BUILTIN_LANGUAGES.contains(&"toml"));
        // Non-common languages should not be in the builtin list
        assert!(!BUILTIN_LANGUAGES.contains(&"csharp"));
        assert!(!BUILTIN_LANGUAGES.contains(&"swift"));
        assert!(!BUILTIN_LANGUAGES.contains(&"kotlin"));
        assert!(!BUILTIN_LANGUAGES.contains(&"scala"));
        assert!(!BUILTIN_LANGUAGES.contains(&"zig"));
        assert!(!BUILTIN_LANGUAGES.contains(&"lua"));
        assert!(!BUILTIN_LANGUAGES.contains(&"bash"));
        assert!(!BUILTIN_LANGUAGES.contains(&"elixir"));
    }

    #[test]
    fn test_find_installed_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let installed = find_installed(tmp.path());
        assert!(installed.is_empty());
    }

    #[test]
    fn test_find_installed_with_packages() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir(tmp.path().join("elixir")).unwrap();
        std::fs::create_dir(tmp.path().join("haskell")).unwrap();
        // A file (not a dir) should be ignored
        std::fs::write(tmp.path().join("not_a_package"), "").unwrap();
        let installed = find_installed(tmp.path());
        assert_eq!(installed, vec!["elixir", "haskell"]);
    }

    #[test]
    fn test_find_installed_nonexistent_dir() {
        let installed = find_installed(Path::new("/nonexistent/path/xyz"));
        assert!(installed.is_empty());
    }

    #[test]
    fn test_install_builtin_rejected() {
        let result = run_install("rust").unwrap();
        assert_eq!(result, ExitCode::UsageError);
    }

    #[test]
    fn test_install_unknown_language_rejected() {
        let result = run_install("klingon").unwrap();
        assert_eq!(result, ExitCode::UsageError);
    }

    #[test]
    fn test_remove_builtin_rejected() {
        let result = run_remove("rust").unwrap();
        assert_eq!(result, ExitCode::UsageError);
    }

    #[test]
    fn test_info_builtin_language() {
        let result = run_info("rust").unwrap();
        assert_eq!(result, ExitCode::Success);
    }

    #[test]
    fn test_info_registry_language() {
        let result = run_info("elixir").unwrap();
        assert_eq!(result, ExitCode::Success);
    }

    #[test]
    fn test_info_unknown_language() {
        let result = run_info("klingon").unwrap();
        assert_eq!(result, ExitCode::Success);
    }

    #[test]
    fn test_list_succeeds() {
        let result = run_list();
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ExitCode::Success);
    }

    // CQ_DATA_DIR env var tests consolidated to prevent parallel races.
    #[test]
    fn test_grammar_operations_with_data_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CQ_DATA_DIR", tmp.path().to_str().unwrap());

        // Install fails gracefully for a non-existent language
        let result = run_install("klingon_lang_xyz");
        assert!(result.is_ok());
        let pkg_dir = tmp.path().join("languages").join("klingon_lang_xyz");
        assert!(
            !pkg_dir.exists(),
            "package dir should not exist after failed download"
        );

        // Remove on installed package works
        let pkg_dir = tmp.path().join("languages").join("elixir");
        std::fs::create_dir_all(&pkg_dir).unwrap();
        std::fs::write(pkg_dir.join("grammar.wasm"), "placeholder").unwrap();
        let result = run_remove("elixir").unwrap();
        assert_eq!(result, ExitCode::Success);
        assert!(!pkg_dir.exists());

        // Remove on not-installed returns NoResults
        std::fs::create_dir_all(tmp.path().join("languages")).unwrap();
        let result = run_remove("elixir").unwrap();
        assert_eq!(result, ExitCode::Success);

        std::env::remove_var("CQ_DATA_DIR");
    }
}
