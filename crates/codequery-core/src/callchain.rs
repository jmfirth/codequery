//! Call chain types for multi-level call hierarchy.

use std::path::PathBuf;

use serde::Serialize;

use crate::symbol::SymbolKind;

/// A node in a call hierarchy tree.
#[derive(Debug, Clone, Serialize)]
pub struct CallChainNode {
    /// The symbol name.
    pub name: String,
    /// The symbol kind.
    pub kind: SymbolKind,
    /// The file containing this symbol.
    pub file: PathBuf,
    /// Line number (1-based).
    pub line: usize,
    /// Column number (0-based).
    pub column: usize,
    /// Callers of this symbol (recursive).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub callers: Vec<CallChainNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callchain_node_serializes_to_json() {
        let node = CallChainNode {
            name: "handle_request".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/handler.rs"),
            line: 10,
            column: 0,
            callers: vec![CallChainNode {
                name: "main".to_string(),
                kind: SymbolKind::Function,
                file: PathBuf::from("src/main.rs"),
                line: 5,
                column: 0,
                callers: vec![],
            }],
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(json.contains("\"handle_request\""));
        assert!(json.contains("\"main\""));
    }

    #[test]
    fn callchain_node_skips_empty_callers() {
        let node = CallChainNode {
            name: "leaf".to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from("src/lib.rs"),
            line: 1,
            column: 0,
            callers: vec![],
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(!json.contains("callers"));
    }
}
