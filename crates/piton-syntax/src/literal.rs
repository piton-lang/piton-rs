//! What a whole value means when it is not prose.
//!
//! An unconstrained value is inferred from how it looks: `42` is a number,
//! `true` a boolean, `null` nothing, and anything else a string. The evaluator
//! decides that, and the parser marks the same values in the syntax tree, so
//! the two share the rule here rather than each keeping a copy.

use crate::kind::SyntaxKind;

/// Parses a Piton number literal: an optional minus, digits with `_` allowed
/// between them, and at most one `.` with a digit on each side. `.5`, `5.`,
/// `1e3` and `1__0` are not numbers.
pub fn parse_number_literal(text: &str) -> Option<f64> {
    let text = text.trim();
    let digits = text.strip_prefix('-').unwrap_or(text);
    if !digits.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut seen_dot = false;
    let mut previous = ' ';
    for ch in digits.chars() {
        match ch {
            '0'..='9' => {}
            '_' if previous.is_ascii_digit() => {}
            '.' if !seen_dot && previous.is_ascii_digit() => seen_dot = true,
            _ => return None,
        }
        previous = ch;
    }
    if !previous.is_ascii_digit() {
        return None;
    }
    let cleaned: String = text.chars().filter(|c| *c != '_').collect();
    cleaned.parse::<f64>().ok()
}

/// The token a value written as exactly `text` is, when it is not prose.
pub fn whole_value_kind(text: &str) -> Option<SyntaxKind> {
    match text {
        "true" => Some(SyntaxKind::KW_TRUE),
        "false" => Some(SyntaxKind::KW_FALSE),
        "null" => Some(SyntaxKind::KW_NULL),
        _ if parse_number_literal(text).is_some() => Some(SyntaxKind::NUMBER),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_the_language() {
        for number in ["3", "-1_000.50", "0.5", "42"] {
            assert!(parse_number_literal(number).is_some(), "{number}");
        }
        for text in [".5", "5.", "1e3", "1__0", "_1", "3 apples", "-", "1.2.3"] {
            assert!(parse_number_literal(text).is_none(), "{text}");
        }
    }

    #[test]
    fn whole_values_are_classified() {
        assert_eq!(whole_value_kind("true"), Some(SyntaxKind::KW_TRUE));
        assert_eq!(whole_value_kind("null"), Some(SyntaxKind::KW_NULL));
        assert_eq!(whole_value_kind("12"), Some(SyntaxKind::NUMBER));
        assert_eq!(whole_value_kind("true story"), None);
    }
}
