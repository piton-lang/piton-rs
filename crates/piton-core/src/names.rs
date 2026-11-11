//! Name normalization.
//!
//! Belay renders "anchor names and property names with prose as
//! word-separated titles". Belay also derives skill, command and agent
//! identities from anchor names. The two operations use one rule to split
//! words. Thus a name always splits into the same words, in each direction.
//!
//! A split occurs only where an uppercase letter follows a lowercase letter or
//! a digit. A split also occurs at literal `-` and `_` separators. Runs of
//! uppercase letters do *not* split, and this is intentional. `whatIsAType`
//! renders as `What Is AType`, as in the reference output. A split for acronyms
//! gives `What Is A Type`, which is a different document.

/// Splits a source name into its words.
pub fn split_words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();

    let chars: Vec<char> = name.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch == '-' || ch == '_' || ch == ' ' {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            continue;
        }

        let previous = if i == 0 { None } else { Some(chars[i - 1]) };
        let boundary = match previous {
            Some(prev) => {
                (prev.is_lowercase() || prev.is_ascii_digit())
                    && (ch.is_uppercase() || ch.is_ascii_digit() && !prev.is_ascii_digit())
            }
            None => false,
        };
        if boundary && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// Renders a source name as a word-separated title: `orderOfPrecedence` becomes
/// `Order Of Precedence`.
pub fn title_case(name: &str) -> String {
    let words = split_words(name);
    if words.is_empty() {
        return name.to_string();
    }
    words
        .iter()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    let mut out: String = first.to_uppercase().collect();
                    out.push_str(chars.as_str());
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Renders a source name in kebab case: `BuildTooling` becomes `build-tooling`.
///
/// Skill directories, command names (before the `x-` prefix) and agent
/// identities use this normalization.
pub fn kebab_case(name: &str) -> String {
    let words = split_words(name);
    if words.is_empty() {
        return name.to_ascii_lowercase();
    }
    words
        .iter()
        .map(|w| w.to_lowercase())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Words that the language reserves. You cannot use them as user-defined
/// keywords. But you can use them as keys.
pub const RESERVED_WORDS: &[&str] = &[
    "anchor", "abstract", "export", "from", "import", "use", "as", "extends", "pass", "this",
    "self", "super", "true", "false", "null", "any", "simple", "complex", "string", "number",
    "boolean", "list", "dictionary",
];

pub fn is_reserved(word: &str) -> bool {
    RESERVED_WORDS.contains(&word)
}

/// True when `name` is a permitted user-defined keyword. All letters are lowercase.
/// Kebab case is optional.
pub fn is_valid_keyword(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut previous_hyphen = true;
    for ch in name.chars() {
        match ch {
            '-' => {
                if previous_hyphen {
                    return false;
                }
                previous_hyphen = true;
            }
            c if c.is_lowercase() || c.is_ascii_digit() => previous_hyphen = false,
            _ => return false,
        }
    }
    !previous_hyphen
}

/// True when `name` obeys the identity rules for the skill directories of all
/// adapters. The rules are 1-64 lowercase alphanumerics with single hyphen
/// separators.
pub fn is_valid_artifact_name(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > 64 {
        return false;
    }
    let mut previous_hyphen = true;
    for ch in name.chars() {
        match ch {
            '-' => {
                if previous_hyphen {
                    return false;
                }
                previous_hyphen = true;
            }
            c if c.is_ascii_lowercase() || c.is_ascii_digit() => previous_hyphen = false,
            _ => return false,
        }
    }
    !previous_hyphen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_match_reference_output() {
        assert_eq!(title_case("whatIsAType"), "What Is AType");
        assert_eq!(title_case("orderOfPrecedence"), "Order Of Precedence");
        assert_eq!(title_case("combiningCollectionTypes"), "Combining Collection Types");
        assert_eq!(title_case("CncComparison"), "Cnc Comparison");
        assert_eq!(title_case("SourceFilesAndModules"), "Source Files And Modules");
        assert_eq!(title_case("Mutability"), "Mutability");
        assert_eq!(title_case("Resue"), "Resue");
        assert_eq!(title_case("codeBlocks"), "Code Blocks");
        assert_eq!(title_case("listOfComplexTypes"), "List Of Complex Types");
    }

    #[test]
    fn kebab_matches_generated_skill_names() {
        assert_eq!(kebab_case("BuildTooling"), "build-tooling");
        assert_eq!(kebab_case("InspectSpec"), "inspect-spec");
        assert_eq!(kebab_case("Agent"), "agent");
    }

    #[test]
    fn keyword_validation() {
        assert!(is_valid_keyword("anchor"));
        assert!(is_valid_keyword("belay-config"));
        assert!(is_valid_keyword("arithmetic-operator"));
        assert!(!is_valid_keyword("Anchor"));
        assert!(!is_valid_keyword("belay--config"));
        assert!(!is_valid_keyword("trailing-"));
    }

    #[test]
    fn artifact_name_validation() {
        assert!(is_valid_artifact_name("build-tooling"));
        assert!(!is_valid_artifact_name("Build-Tooling"));
        assert!(!is_valid_artifact_name(&"a".repeat(65)));
    }
}
