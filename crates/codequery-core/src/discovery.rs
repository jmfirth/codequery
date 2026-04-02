//! File discovery with `.gitignore`-aware walking and language detection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ignore::WalkBuilder;

use crate::config::ProjectConfig;
use crate::error::{CoreError, Result};

/// Baked-in registry JSON (all 71 languages with their extensions).
const REGISTRY_JSON: &str = include_str!("../../../languages/registry.json");

/// Cached extension → language name map built from the registry.
static EXTENSION_MAP: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Get the extension → language name map, building it on first access.
fn extension_map() -> &'static HashMap<String, String> {
    EXTENSION_MAP.get_or_init(|| {
        let mut map = HashMap::new();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(REGISTRY_JSON) {
            if let Some(languages) = parsed.get("languages").and_then(|l| l.as_array()) {
                for lang in languages {
                    let Some(name) = lang.get("name").and_then(|n| n.as_str()) else {
                        continue;
                    };
                    if let Some(exts) = lang.get("extensions").and_then(|e| e.as_array()) {
                        for ext in exts {
                            if let Some(ext_str) = ext.as_str() {
                                // Strip leading dot: ".rs" → "rs"
                                let key = ext_str.strip_prefix('.').unwrap_or(ext_str);
                                map.insert(key.to_string(), name.to_string());
                            }
                        }
                    }
                }
            }
        }
        map
    })
}

/// Derive the WASM function name for a language from the registry's `grammar_repo`.
///
/// Tree-sitter WASM modules export `tree_sitter_<suffix>` where `<suffix>` comes
/// from the grammar repo name (e.g., `tree-sitter-c-sharp` → `c_sharp`). This
/// function extracts that suffix from the registry, handling cases where our
/// canonical language name differs (`csharp` → `c_sharp`, `objective-c` → `objc`, etc.).
///
/// Returns `None` if the language isn't in the registry or has no `grammar_repo`.
#[must_use]
pub fn wasm_name_for_language(name: &str) -> Option<String> {
    static WASM_NAME_MAP: OnceLock<HashMap<String, String>> = OnceLock::new();

    let map = WASM_NAME_MAP.get_or_init(|| {
        let mut m = HashMap::new();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(REGISTRY_JSON) {
            if let Some(languages) = parsed.get("languages").and_then(|l| l.as_array()) {
                for lang in languages {
                    let Some(lang_name) = lang.get("name").and_then(|n| n.as_str()) else {
                        continue;
                    };
                    let Some(repo) = lang.get("grammar_repo").and_then(|r| r.as_str()) else {
                        continue;
                    };
                    // Extract suffix: "tree-sitter/tree-sitter-c-sharp" → "c-sharp" → "c_sharp"
                    let repo_name = repo.rsplit('/').next().unwrap_or(repo);
                    let suffix = repo_name.strip_prefix("tree-sitter-").unwrap_or(repo_name);
                    let wasm_name = suffix.replace('-', "_");
                    m.insert(lang_name.to_string(), wasm_name);
                }
            }
        }
        m
    });

    map.get(name).cloned()
}

/// Resolve a file extension to a language name using the registry.
///
/// Returns the language name (e.g. "rust", "elixir") for any language
/// in the registry, whether built-in or installable.
#[must_use]
pub fn language_name_for_file(path: &Path) -> Option<String> {
    let ext_str = path.extension().and_then(|ext| ext.to_str())?;
    extension_map().get(ext_str).cloned()
}

/// Supported source languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Language {
    // --- Tier 1 ---
    /// The Rust programming language.
    Rust,
    /// TypeScript (`.ts`, `.tsx`).
    TypeScript,
    /// JavaScript (`.js`, `.jsx`).
    JavaScript,
    /// Python (`.py`).
    Python,
    /// Go (`.go`).
    Go,
    /// C (`.c`, `.h`).
    C,
    /// C++ (`.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`, `.hh`).
    Cpp,
    /// Java (`.java`).
    Java,
    // --- Tier 2 ---
    /// Ruby (`.rb`).
    Ruby,
    /// PHP (`.php`).
    Php,
    /// C# (`.cs`).
    CSharp,
    /// Swift (`.swift`).
    Swift,
    /// Kotlin (`.kt`).
    Kotlin,
    /// Scala (`.scala`).
    Scala,
    /// Zig (`.zig`).
    Zig,
    /// Lua (`.lua`).
    Lua,
    /// Bash (`.sh`, `.bash`).
    Bash,
    // --- Structured data ---
    /// HTML (`.html`, `.htm`).
    Html,
    /// CSS (`.css`).
    Css,
    /// JSON (`.json`).
    Json,
    /// YAML (`.yaml`, `.yml`).
    Yaml,
    /// TOML (`.toml`).
    Toml,
}

impl Language {
    /// The canonical machine name for this language.
    ///
    /// Used for feature flag names, grammar lookups, and display.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Go => "go",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Java => "java",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::CSharp => "csharp",
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            Self::Scala => "scala",
            Self::Zig => "zig",
            Self::Lua => "lua",
            Self::Bash => "bash",
            Self::Html => "html",
            Self::Css => "css",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
        }
    }

    /// Parse a language name from a user-provided string (case-insensitive).
    ///
    /// Accepts common names and aliases for all supported languages.
    ///
    /// Returns `None` if the string doesn't match any known language.
    #[must_use]
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Some(Self::Rust),
            "typescript" | "ts" => Some(Self::TypeScript),
            "javascript" | "js" => Some(Self::JavaScript),
            "python" | "py" => Some(Self::Python),
            "go" => Some(Self::Go),
            "c" => Some(Self::C),
            "cpp" | "c++" | "cxx" => Some(Self::Cpp),
            "java" => Some(Self::Java),
            "ruby" | "rb" => Some(Self::Ruby),
            "php" => Some(Self::Php),
            "csharp" | "c#" | "cs" => Some(Self::CSharp),
            "swift" => Some(Self::Swift),
            "kotlin" | "kt" => Some(Self::Kotlin),
            "scala" => Some(Self::Scala),
            "zig" => Some(Self::Zig),
            "lua" => Some(Self::Lua),
            "bash" | "sh" => Some(Self::Bash),
            "html" => Some(Self::Html),
            "css" => Some(Self::Css),
            "json" => Some(Self::Json),
            "yaml" | "yml" => Some(Self::Yaml),
            "toml" => Some(Self::Toml),
            _ => None,
        }
    }
}

/// Detect the language of a file from its extension.
///
/// Recognizes all Tier 1 and Tier 2 language extensions.
#[must_use]
pub fn language_for_file(path: &Path) -> Option<Language> {
    language_for_file_with_overrides(path, &HashMap::new())
}

/// Detect the language of a file, consulting extension overrides first.
///
/// Overrides map a file extension (with leading dot, e.g. `".jsx"`) to a
/// language name string (e.g. `"javascript"`). If the file's extension
/// matches an override, the override takes precedence over built-in mappings.
#[must_use]
pub fn language_for_file_with_overrides<S: std::hash::BuildHasher>(
    path: &Path,
    overrides: &HashMap<String, String, S>,
) -> Option<Language> {
    let ext_str = path.extension().and_then(|ext| ext.to_str())?;

    // Check overrides first (keyed with leading dot, e.g. ".jsx")
    let dotted = format!(".{ext_str}");
    if let Some(lang_name) = overrides.get(&dotted) {
        return Language::from_name(lang_name);
    }

    match ext_str {
        "rs" => Some(Language::Rust),
        "ts" | "tsx" => Some(Language::TypeScript),
        "js" | "jsx" => Some(Language::JavaScript),
        "py" => Some(Language::Python),
        "go" => Some(Language::Go),
        "c" | "h" => Some(Language::C),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some(Language::Cpp),
        "java" => Some(Language::Java),
        "rb" => Some(Language::Ruby),
        "php" => Some(Language::Php),
        "cs" => Some(Language::CSharp),
        "swift" => Some(Language::Swift),
        "kt" => Some(Language::Kotlin),
        "scala" => Some(Language::Scala),
        "zig" => Some(Language::Zig),
        "lua" => Some(Language::Lua),
        "sh" | "bash" => Some(Language::Bash),
        "html" | "htm" => Some(Language::Html),
        "css" => Some(Language::Css),
        "json" => Some(Language::Json),
        "yaml" | "yml" => Some(Language::Yaml),
        "toml" => Some(Language::Toml),
        // Fallback: check the registry for runtime/installable languages
        _ => extension_map()
            .get(ext_str)
            .and_then(|name| Language::from_name(name)),
    }
}

/// Discover source files under `root`, optionally scoped to a subdirectory.
///
/// Uses the `ignore` crate for `.gitignore`-aware walking.
/// Filters to files with recognized source extensions.
/// Returns sorted paths (relative to `root`) for deterministic output.
///
/// # Errors
///
/// Returns `CoreError::Path` if the walk root (or scoped subdirectory) does not exist,
/// or if a filesystem error occurs during walking.
pub fn discover_files(root: &Path, scope: Option<&Path>) -> Result<Vec<PathBuf>> {
    let walk_root = match scope {
        Some(s) => root.join(s),
        None => root.to_path_buf(),
    };

    if !walk_root.exists() {
        return Err(CoreError::Path(format!(
            "discovery path does not exist: {}",
            walk_root.display()
        )));
    }

    let walker = WalkBuilder::new(&walk_root)
        .add_custom_ignore_filename(".cqignore")
        .build();

    let mut files = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|e| CoreError::Path(format!("walk error: {e}")))?;

        // Skip directories — we only want files.
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }

        let path = entry.path();
        if language_name_for_file(path).is_some() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| CoreError::Path(format!("cannot make path relative: {e}")))?;
            files.push(relative.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

/// Discover source files with project configuration applied.
///
/// Like [`discover_files`], but also applies the project configuration:
/// - Language overrides expand which file extensions are recognized.
/// - Exclude patterns filter out matching paths after discovery.
///
/// # Errors
///
/// Returns the same errors as [`discover_files`].
pub fn discover_files_with_config(
    root: &Path,
    scope: Option<&Path>,
    config: &ProjectConfig,
) -> Result<Vec<PathBuf>> {
    let walk_root = match scope {
        Some(s) => root.join(s),
        None => root.to_path_buf(),
    };

    if !walk_root.exists() {
        return Err(CoreError::Path(format!(
            "discovery path does not exist: {}",
            walk_root.display()
        )));
    }

    let walker = WalkBuilder::new(&walk_root)
        .add_custom_ignore_filename(".cqignore")
        .build();

    let exclude_matchers = build_exclude_matchers(&config.exclude);

    let mut files = Vec::new();
    for entry in walker {
        let entry = entry.map_err(|e| CoreError::Path(format!("walk error: {e}")))?;

        let Some(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }

        let path = entry.path();
        if language_for_file_with_overrides(path, &config.language_overrides).is_some() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| CoreError::Path(format!("cannot make path relative: {e}")))?;

            if !is_excluded(relative, &exclude_matchers) {
                files.push(relative.to_path_buf());
            }
        }
    }

    files.sort();
    Ok(files)
}

/// Build glob matchers from exclude pattern strings.
fn build_exclude_matchers(patterns: &[String]) -> Vec<glob::Pattern> {
    patterns
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect()
}

/// Check if a relative path matches any of the exclude patterns.
fn is_excluded(path: &Path, matchers: &[glob::Pattern]) -> bool {
    let path_str = path.to_string_lossy();
    matchers.iter().any(|m| m.matches(&path_str))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ProjectConfig;
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// Helper to create a minimal project structure in a temp dir.
    fn create_project(files: &[&str]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        // Create a .git dir so the ignore crate has a root context
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        for file in files {
            let path = tmp.path().join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, "// placeholder").unwrap();
        }
        tmp
    }

    #[test]
    fn test_discover_files_finds_rs_files() {
        let tmp = create_project(&["src/main.rs", "src/lib.rs", "src/util.rs"]);

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(
            files,
            vec![
                PathBuf::from("src/lib.rs"),
                PathBuf::from("src/main.rs"),
                PathBuf::from("src/util.rs"),
            ]
        );
    }

    #[test]
    fn test_discover_files_respects_gitignore() {
        let tmp = create_project(&["src/main.rs", "src/generated.rs"]);
        std::fs::write(tmp.path().join(".gitignore"), "src/generated.rs\n").unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_skips_target_directory() {
        let tmp = create_project(&["src/main.rs", "target/debug/build.rs"]);
        // The ignore crate skips hidden dirs and respects .gitignore.
        // `target/` is typically in .gitignore for Rust projects.
        std::fs::write(tmp.path().join(".gitignore"), "target/\n").unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_scope_limits_to_subdirectory() {
        let tmp = create_project(&["src/main.rs", "tests/test_it.rs"]);

        let files = discover_files(tmp.path(), Some(Path::new("src"))).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_returns_relative_paths() {
        let tmp = create_project(&["src/main.rs"]);

        let files = discover_files(tmp.path(), None).unwrap();
        for f in &files {
            assert!(
                f.is_relative(),
                "expected relative path, got: {}",
                f.display()
            );
        }
    }

    #[test]
    fn test_discover_files_returns_sorted_paths() {
        let tmp = create_project(&["z.rs", "a.rs", "m.rs"]);

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(
            files,
            vec![
                PathBuf::from("a.rs"),
                PathBuf::from("m.rs"),
                PathBuf::from("z.rs"),
            ]
        );
    }

    #[test]
    fn test_discover_files_empty_directory_returns_empty_vec() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir(tmp.path().join(".git")).unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_discover_files_nonexistent_scope_returns_error() {
        let tmp = TempDir::new().unwrap();

        let result = discover_files(tmp.path(), Some(Path::new("nonexistent")));
        assert!(result.is_err());
    }

    #[test]
    fn test_language_for_file_returns_rust_for_rs() {
        assert_eq!(
            language_for_file(Path::new("src/main.rs")),
            Some(Language::Rust)
        );
    }

    #[test]
    fn test_language_for_file_returns_none_for_unrecognized_extensions() {
        assert_eq!(language_for_file(Path::new("readme.txt")), None);
        assert_eq!(language_for_file(Path::new("no_extension")), None);
        assert_eq!(language_for_file(Path::new("image.png")), None);
        assert_eq!(language_for_file(Path::new("archive.tar.gz")), None);
    }

    #[test]
    fn test_language_for_file_returns_typescript_for_ts_tsx() {
        assert_eq!(
            language_for_file(Path::new("app.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            language_for_file(Path::new("component.tsx")),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn test_language_for_file_returns_javascript_for_js_jsx() {
        assert_eq!(
            language_for_file(Path::new("app.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            language_for_file(Path::new("component.jsx")),
            Some(Language::JavaScript)
        );
    }

    #[test]
    fn test_language_for_file_returns_python_for_py() {
        assert_eq!(
            language_for_file(Path::new("script.py")),
            Some(Language::Python)
        );
    }

    #[test]
    fn test_language_for_file_returns_go_for_go() {
        assert_eq!(language_for_file(Path::new("main.go")), Some(Language::Go));
    }

    #[test]
    fn test_language_for_file_returns_c_for_c_h() {
        assert_eq!(language_for_file(Path::new("main.c")), Some(Language::C));
        assert_eq!(language_for_file(Path::new("header.h")), Some(Language::C));
    }

    #[test]
    fn test_language_for_file_returns_cpp_for_cpp_extensions() {
        assert_eq!(
            language_for_file(Path::new("main.cpp")),
            Some(Language::Cpp)
        );
        assert_eq!(language_for_file(Path::new("main.cc")), Some(Language::Cpp));
        assert_eq!(
            language_for_file(Path::new("main.cxx")),
            Some(Language::Cpp)
        );
        assert_eq!(
            language_for_file(Path::new("header.hpp")),
            Some(Language::Cpp)
        );
        assert_eq!(
            language_for_file(Path::new("header.hxx")),
            Some(Language::Cpp)
        );
        assert_eq!(
            language_for_file(Path::new("header.hh")),
            Some(Language::Cpp)
        );
    }

    #[test]
    fn test_language_for_file_returns_java_for_java() {
        assert_eq!(
            language_for_file(Path::new("Main.java")),
            Some(Language::Java)
        );
    }

    // -----------------------------------------------------------------------
    // Language::from_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_language_from_name_all_languages() {
        let cases = [
            ("rust", Language::Rust),
            ("rs", Language::Rust),
            ("typescript", Language::TypeScript),
            ("ts", Language::TypeScript),
            ("javascript", Language::JavaScript),
            ("js", Language::JavaScript),
            ("python", Language::Python),
            ("py", Language::Python),
            ("go", Language::Go),
            ("c", Language::C),
            ("cpp", Language::Cpp),
            ("c++", Language::Cpp),
            ("cxx", Language::Cpp),
            ("java", Language::Java),
            ("ruby", Language::Ruby),
            ("rb", Language::Ruby),
            ("php", Language::Php),
            ("csharp", Language::CSharp),
            ("c#", Language::CSharp),
            ("cs", Language::CSharp),
            ("swift", Language::Swift),
            ("kotlin", Language::Kotlin),
            ("kt", Language::Kotlin),
            ("scala", Language::Scala),
            ("zig", Language::Zig),
            ("lua", Language::Lua),
            ("bash", Language::Bash),
            ("sh", Language::Bash),
            ("html", Language::Html),
            ("css", Language::Css),
            ("json", Language::Json),
            ("yaml", Language::Yaml),
            ("yml", Language::Yaml),
            ("toml", Language::Toml),
        ];
        for (input, expected) in cases {
            assert_eq!(
                Language::from_name(input),
                Some(expected),
                "failed for input: {input}"
            );
        }
    }

    #[test]
    fn test_language_from_name_case_insensitive() {
        assert_eq!(Language::from_name("Rust"), Some(Language::Rust));
        assert_eq!(Language::from_name("PYTHON"), Some(Language::Python));
        assert_eq!(
            Language::from_name("TypeScript"),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn test_language_from_name_unknown_returns_none() {
        assert_eq!(Language::from_name("unknown"), None);
        assert_eq!(Language::from_name(""), None);
        assert_eq!(Language::from_name("brainfuck"), None);
    }

    // -----------------------------------------------------------------------
    // Tier 2: language_for_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_language_for_file_returns_ruby_for_rb() {
        assert_eq!(language_for_file(Path::new("app.rb")), Some(Language::Ruby));
    }

    #[test]
    fn test_language_for_file_returns_php_for_php() {
        assert_eq!(
            language_for_file(Path::new("index.php")),
            Some(Language::Php)
        );
    }

    #[test]
    fn test_language_for_file_returns_csharp_for_cs() {
        assert_eq!(
            language_for_file(Path::new("Program.cs")),
            Some(Language::CSharp)
        );
    }

    #[test]
    fn test_language_for_file_returns_swift_for_swift() {
        assert_eq!(
            language_for_file(Path::new("main.swift")),
            Some(Language::Swift)
        );
    }

    #[test]
    fn test_language_for_file_returns_kotlin_for_kt() {
        assert_eq!(
            language_for_file(Path::new("Main.kt")),
            Some(Language::Kotlin)
        );
    }

    #[test]
    fn test_language_for_file_returns_scala_for_scala() {
        assert_eq!(
            language_for_file(Path::new("Main.scala")),
            Some(Language::Scala)
        );
    }

    #[test]
    fn test_language_for_file_returns_zig_for_zig() {
        assert_eq!(
            language_for_file(Path::new("main.zig")),
            Some(Language::Zig)
        );
    }

    #[test]
    fn test_language_for_file_returns_lua_for_lua() {
        assert_eq!(
            language_for_file(Path::new("init.lua")),
            Some(Language::Lua)
        );
    }

    #[test]
    fn test_language_for_file_returns_bash_for_sh_bash() {
        assert_eq!(
            language_for_file(Path::new("setup.sh")),
            Some(Language::Bash)
        );
        assert_eq!(
            language_for_file(Path::new("install.bash")),
            Some(Language::Bash)
        );
    }

    // -----------------------------------------------------------------------
    // .cqignore support
    // -----------------------------------------------------------------------

    #[test]
    fn test_discover_files_respects_cqignore() {
        let tmp = create_project(&["src/main.rs", "src/generated.rs", "vendor/dep.rs"]);
        std::fs::write(tmp.path().join(".cqignore"), "vendor/\nsrc/generated.rs\n").unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_cqignore_with_glob_pattern() {
        let tmp = create_project(&["src/main.rs", "src/gen_foo.rs", "src/gen_bar.rs"]);
        std::fs::write(tmp.path().join(".cqignore"), "src/gen_*\n").unwrap();

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_no_cqignore_discovers_all() {
        let tmp = create_project(&["src/main.rs", "vendor/dep.rs"]);

        let files = discover_files(tmp.path(), None).unwrap();
        assert_eq!(
            files,
            vec![PathBuf::from("src/main.rs"), PathBuf::from("vendor/dep.rs")]
        );
    }

    // -----------------------------------------------------------------------
    // language_for_file_with_overrides
    // -----------------------------------------------------------------------

    #[test]
    fn test_language_for_file_with_overrides_uses_override() {
        let overrides = HashMap::from([(".svelte".to_string(), "javascript".to_string())]);
        assert_eq!(
            language_for_file_with_overrides(Path::new("App.svelte"), &overrides),
            Some(Language::JavaScript)
        );
    }

    #[test]
    fn test_language_for_file_with_overrides_override_takes_precedence() {
        // Override .h to be C++ instead of C
        let overrides = HashMap::from([(".h".to_string(), "cpp".to_string())]);
        assert_eq!(
            language_for_file_with_overrides(Path::new("header.h"), &overrides),
            Some(Language::Cpp)
        );
    }

    #[test]
    fn test_language_for_file_with_overrides_falls_back_to_builtin() {
        let overrides = HashMap::from([(".svelte".to_string(), "javascript".to_string())]);
        // .rs is not overridden, should still resolve via built-in
        assert_eq!(
            language_for_file_with_overrides(Path::new("main.rs"), &overrides),
            Some(Language::Rust)
        );
    }

    #[test]
    fn test_language_for_file_with_overrides_empty_overrides() {
        let overrides = HashMap::new();
        assert_eq!(
            language_for_file_with_overrides(Path::new("main.rs"), &overrides),
            Some(Language::Rust)
        );
        assert_eq!(
            language_for_file_with_overrides(Path::new("unknown.xyz"), &overrides),
            None
        );
    }

    #[test]
    fn test_language_for_file_with_overrides_invalid_language_name() {
        let overrides = HashMap::from([(".xyz".to_string(), "nonexistent_language".to_string())]);
        assert_eq!(
            language_for_file_with_overrides(Path::new("file.xyz"), &overrides),
            None
        );
    }

    // -----------------------------------------------------------------------
    // discover_files_with_config
    // -----------------------------------------------------------------------

    #[test]
    fn test_discover_files_with_config_exclude_patterns() {
        let tmp = create_project(&["src/main.rs", "vendor/dep.rs", "generated/output.rs"]);

        let config = ProjectConfig {
            exclude: vec!["vendor/**".to_string(), "generated/**".to_string()],
            language_overrides: HashMap::new(),
            cache_enabled: None,
            lsp: None,
        };

        let files = discover_files_with_config(tmp.path(), None, &config).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_with_config_language_overrides() {
        let tmp = create_project(&["src/main.rs", "src/app.svelte"]);

        let config = ProjectConfig {
            exclude: Vec::new(),
            language_overrides: HashMap::from([(".svelte".to_string(), "javascript".to_string())]),
            cache_enabled: None,
            lsp: None,
        };

        let files = discover_files_with_config(tmp.path(), None, &config).unwrap();
        assert_eq!(
            files,
            vec![
                PathBuf::from("src/app.svelte"),
                PathBuf::from("src/main.rs")
            ]
        );
    }

    #[test]
    fn test_discover_files_with_config_empty_config() {
        let tmp = create_project(&["src/main.rs", "src/lib.rs"]);

        let config = ProjectConfig {
            exclude: Vec::new(),
            language_overrides: HashMap::new(),
            cache_enabled: None,
            lsp: None,
        };

        let files = discover_files_with_config(tmp.path(), None, &config).unwrap();
        assert_eq!(
            files,
            vec![PathBuf::from("src/lib.rs"), PathBuf::from("src/main.rs")]
        );
    }

    #[test]
    fn test_discover_files_with_config_combined_cqignore_and_exclude() {
        let tmp = create_project(&["src/main.rs", "vendor/dep.rs", "build/output.rs"]);
        // .cqignore excludes vendor/
        std::fs::write(tmp.path().join(".cqignore"), "vendor/\n").unwrap();

        // Config excludes build/
        let config = ProjectConfig {
            exclude: vec!["build/**".to_string()],
            language_overrides: HashMap::new(),
            cache_enabled: None,
            lsp: None,
        };

        let files = discover_files_with_config(tmp.path(), None, &config).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_with_config_scope() {
        let tmp = create_project(&["src/main.rs", "tests/test_it.rs"]);

        let config = ProjectConfig {
            exclude: Vec::new(),
            language_overrides: HashMap::new(),
            cache_enabled: None,
            lsp: None,
        };

        let files =
            discover_files_with_config(tmp.path(), Some(Path::new("src")), &config).unwrap();
        assert_eq!(files, vec![PathBuf::from("src/main.rs")]);
    }

    #[test]
    fn test_discover_files_with_config_nonexistent_scope_returns_error() {
        let tmp = TempDir::new().unwrap();
        let config = ProjectConfig {
            exclude: Vec::new(),
            language_overrides: HashMap::new(),
            cache_enabled: None,
            lsp: None,
        };

        let result =
            discover_files_with_config(tmp.path(), Some(Path::new("nonexistent")), &config);
        assert!(result.is_err());
    }
}
