//! File discovery with `.gitignore`-aware walking and language detection.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::error::{CoreError, Result};

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
}

impl Language {
    /// Parse a language name from a user-provided string (case-insensitive).
    ///
    /// Accepts common names and aliases for all Tier 1 and Tier 2 languages.
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
            _ => None,
        }
    }
}

/// Detect the language of a file from its extension.
///
/// Recognizes all Tier 1 and Tier 2 language extensions.
#[must_use]
pub fn language_for_file(path: &Path) -> Option<Language> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext {
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
            _ => None,
        })
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

    let walker = WalkBuilder::new(&walk_root).build();

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
        if language_for_file(path).is_some() {
            let relative = path
                .strip_prefix(root)
                .map_err(|e| CoreError::Path(format!("cannot make path relative: {e}")))?;
            files.push(relative.to_path_buf());
        }
    }

    files.sort();
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(language_for_file(Path::new("data.json")), None);
        assert_eq!(language_for_file(Path::new("style.css")), None);
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
}
