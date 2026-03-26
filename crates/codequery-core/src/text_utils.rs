//! Text utilities for code searching.
//!
//! Provides word-boundary-aware text matching for the grep pre-filter stage of
//! narrow commands. These utilities avoid regex overhead by using simple character
//! boundary checks.

/// Check if `source` contains `word` at a word boundary.
///
/// A word boundary is defined as a position where the adjacent character is not
/// alphanumeric or underscore. This is more precise than `str::contains()` and
/// avoids false positives for short symbol names embedded in longer identifiers.
#[must_use]
pub fn contains_word(source: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }

    let source_bytes = source.as_bytes();
    let word_bytes = word.as_bytes();
    let word_len = word_bytes.len();

    let mut start = 0;
    while start + word_len <= source_bytes.len() {
        if let Some(pos) = source[start..].find(word) {
            let abs_pos = start + pos;
            let before_ok = abs_pos == 0 || !is_word_char(source_bytes[abs_pos - 1]);
            let after_pos = abs_pos + word_len;
            let after_ok =
                after_pos >= source_bytes.len() || !is_word_char(source_bytes[after_pos]);

            if before_ok && after_ok {
                return true;
            }
            start = abs_pos + 1;
        } else {
            break;
        }
    }
    false
}

/// Returns true if the byte represents a word character (alphanumeric or underscore).
fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_word_substring_not_at_boundary_returns_false() {
        assert!(!contains_word("fn foo_bar()", "foo"));
    }

    #[test]
    fn test_contains_word_standalone_function_name_returns_true() {
        assert!(contains_word("fn foo()", "foo"));
    }

    #[test]
    fn test_contains_word_followed_by_semicolon_returns_true() {
        assert!(contains_word("let x = foo;", "foo"));
    }

    #[test]
    fn test_contains_word_empty_word_returns_false() {
        assert!(!contains_word("fn foo()", ""));
    }

    #[test]
    fn test_contains_word_at_start_of_source() {
        assert!(contains_word("foo(bar)", "foo"));
    }

    #[test]
    fn test_contains_word_at_end_of_source() {
        assert!(contains_word("let x = foo", "foo"));
    }

    #[test]
    fn test_contains_word_prefix_match_returns_false() {
        assert!(!contains_word("foobar", "foo"));
    }

    #[test]
    fn test_contains_word_suffix_match_returns_false() {
        assert!(!contains_word("barfoo_x", "foo"));
    }

    #[test]
    fn test_contains_word_surrounded_by_underscores_returns_false() {
        assert!(!contains_word("_foo_", "foo"));
    }

    #[test]
    fn test_contains_word_preceded_by_dot_returns_true() {
        assert!(contains_word("self.foo()", "foo"));
    }

    #[test]
    fn test_contains_word_not_found_at_all() {
        assert!(!contains_word("let x = bar;", "foo"));
    }

    #[test]
    fn test_contains_word_multiple_occurrences_one_at_boundary() {
        // "foo_bar" has foo not at boundary, but standalone "foo" is at boundary
        assert!(contains_word("foo_bar foo", "foo"));
    }

    #[test]
    fn test_contains_word_exact_match() {
        assert!(contains_word("foo", "foo"));
    }
}
