//! Trait definitions.

/// Something that can be validated.
pub trait Validate {
    /// Check if the value is valid.
    fn is_valid(&self) -> bool;

    /// Return validation errors.
    fn errors(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Something that can be displayed as a summary.
pub trait Summary {
    fn summarize(&self) -> String;
}
