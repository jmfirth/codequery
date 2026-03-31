//! Text edit types for rename and refactoring operations.

use std::path::PathBuf;

use serde::Serialize;

use crate::query::Resolution;

/// A single text replacement in a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextEdit {
    /// The file to edit.
    pub file: PathBuf,
    /// Start line (1-based).
    pub line: usize,
    /// Start column (0-based).
    pub column: usize,
    /// End line (1-based).
    pub end_line: usize,
    /// End column (0-based).
    pub end_column: usize,
    /// The replacement text.
    pub new_text: String,
}

/// Result of a rename operation.
#[derive(Debug, Clone, Serialize)]
pub struct RenameResult {
    /// The old symbol name.
    pub old_name: String,
    /// The new symbol name.
    pub new_name: String,
    /// The text edits to apply.
    pub edits: Vec<TextEdit>,
    /// Number of files affected.
    pub files_affected: usize,
    /// Whether the edits were applied to disk.
    pub applied: bool,
    /// The resolution tier used.
    pub resolution: Resolution,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_edit_serializes_to_json() {
        let edit = TextEdit {
            file: PathBuf::from("src/lib.rs"),
            line: 10,
            column: 4,
            end_line: 10,
            end_column: 7,
            new_text: "Bar".to_string(),
        };
        let json = serde_json::to_string(&edit).unwrap();
        assert!(json.contains("\"new_text\":\"Bar\""));
    }

    #[test]
    fn rename_result_serializes_to_json() {
        let result = RenameResult {
            old_name: "Foo".to_string(),
            new_name: "Bar".to_string(),
            edits: vec![],
            files_affected: 0,
            applied: false,
            resolution: Resolution::Syntactic,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"old_name\":\"Foo\""));
        assert!(json.contains("\"applied\":false"));
    }
}
