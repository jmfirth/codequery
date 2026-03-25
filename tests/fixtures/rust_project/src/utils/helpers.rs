//! Helper utilities.

/// Format a name for display.
pub fn format_name(first: &str, last: &str) -> String {
    format!("{first} {last}")
}

/// A duplicate symbol name (also exists in services.rs).
pub fn helper() -> &'static str {
    "utils helper"
}

pub(crate) fn internal_util() -> u32 {
    42
}
