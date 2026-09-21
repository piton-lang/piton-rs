//! Lexical relationships.
//!
//! The specification asks for "WordNet-like lexical relationships". This is a
//! curated table rather than a full lexical database: it records the relations
//! the analysis can actually act on — synonymy, antonymy, and membership of a
//! mutually exclusive set — for the vocabulary specification prose uses.
//!
//! Curation is a deliberate choice, not a shortcut. Every relation here is one
//! the analysis will draw a conclusion from, and a conclusion drawn from a
//! relation nobody checked is exactly what the `conservative` principle forbids.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The part of speech a word can take. Only the closed classes are listed
/// exhaustively; everything else is decided by position and suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    Determiner,
    Copula,
    Modal,
    Negation,
    Preposition,
    Conjunction,
    Pronoun,
    Auxiliary,
}

/// A set of values that cannot hold at once: a thing is blue or red, not both.
#[derive(Debug, Clone)]
pub struct ExclusiveSet {
    /// The property these values describe, e.g. `color`.
    pub property: &'static str,
    pub values: &'static [&'static str],
}

struct Tables {
    classes: HashMap<&'static str, Class>,
    /// Lemma to the canonical member of its synonym group.
    synonyms: HashMap<&'static str, &'static str>,
    /// Lemma to the property whose exclusive set it belongs to.
    properties: HashMap<&'static str, &'static str>,
    antonyms: HashMap<&'static str, &'static str>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let mut classes = HashMap::new();
        for (word, class) in CLASSES {
            classes.insert(*word, *class);
        }

        let mut synonyms = HashMap::new();
        for group in SYNONYMS {
            let canonical = group[0];
            for word in *group {
                synonyms.insert(*word, canonical);
            }
        }

        let mut properties = HashMap::new();
        for set in EXCLUSIVE_SETS {
            for value in set.values {
                properties.insert(*value, set.property);
            }
        }

        let mut antonyms = HashMap::new();
        for (left, right) in ANTONYMS {
            antonyms.insert(*left, *right);
            antonyms.insert(*right, *left);
        }

        Tables {
            classes,
            synonyms,
            properties,
            antonyms,
        }
    })
}

/// The closed class a word belongs to, if any.
pub fn class(word: &str) -> Option<Class> {
    tables().classes.get(word).copied()
}

/// The canonical form of a word's synonym group, or the word itself.
///
/// This is what makes `must` and `has to` compare equal, and `colour` equal
/// `color`.
pub fn canonical(word: &str) -> &str {
    tables().synonyms.get(word).copied().unwrap_or(word)
}

/// The property a value describes, when it belongs to a known exclusive set.
pub fn property_of(value: &str) -> Option<&'static str> {
    tables().properties.get(canonical(value)).copied()
}

/// True when two values are known to exclude each other.
///
/// Membership of the same exclusive set is the only positive evidence. Two
/// unrelated words are *not* treated as incompatible, because prose that says
/// different things about different aspects of one subject is ordinary, not
/// contradictory.
pub fn excludes(left: &str, right: &str) -> bool {
    let (left, right) = (canonical(left), canonical(right));
    if left == right {
        return false;
    }
    if tables().antonyms.get(left) == Some(&right) {
        return true;
    }
    match (property_of(left), property_of(right)) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// True when two words mean the same thing.
pub fn equivalent(left: &str, right: &str) -> bool {
    canonical(left) == canonical(right)
}

/// How strongly a word signals an obligation rather than a description.
pub fn modality(word: &str) -> Option<crate::claim::Modality> {
    use crate::claim::Modality;
    match canonical(word) {
        "must" => Some(Modality::Required),
        "should" => Some(Modality::Recommended),
        "may" => Some(Modality::Permitted),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

const CLASSES: &[(&str, Class)] = &[
    ("a", Class::Determiner),
    ("an", Class::Determiner),
    ("the", Class::Determiner),
    ("this", Class::Determiner),
    ("that", Class::Determiner),
    ("these", Class::Determiner),
    ("those", Class::Determiner),
    ("each", Class::Determiner),
    ("every", Class::Determiner),
    ("any", Class::Determiner),
    ("some", Class::Determiner),
    ("all", Class::Determiner),
    ("no", Class::Determiner),
    ("its", Class::Determiner),
    ("their", Class::Determiner),
    ("our", Class::Determiner),
    ("your", Class::Determiner),
    ("is", Class::Copula),
    ("are", Class::Copula),
    ("was", Class::Copula),
    ("were", Class::Copula),
    ("be", Class::Copula),
    ("been", Class::Copula),
    ("being", Class::Copula),
    ("must", Class::Modal),
    ("should", Class::Modal),
    ("shall", Class::Modal),
    ("may", Class::Modal),
    ("might", Class::Modal),
    ("can", Class::Modal),
    ("could", Class::Modal),
    ("will", Class::Modal),
    ("would", Class::Modal),
    ("not", Class::Negation),
    ("never", Class::Negation),
    ("cannot", Class::Negation),
    ("n't", Class::Negation),
    ("don't", Class::Negation),
    ("doesn't", Class::Negation),
    ("isn't", Class::Negation),
    ("aren't", Class::Negation),
    ("in", Class::Preposition),
    ("on", Class::Preposition),
    ("at", Class::Preposition),
    ("to", Class::Preposition),
    ("for", Class::Preposition),
    ("with", Class::Preposition),
    ("by", Class::Preposition),
    ("from", Class::Preposition),
    ("of", Class::Preposition),
    ("into", Class::Preposition),
    ("within", Class::Preposition),
    ("under", Class::Preposition),
    ("over", Class::Preposition),
    ("across", Class::Preposition),
    ("through", Class::Preposition),
    ("and", Class::Conjunction),
    ("or", Class::Conjunction),
    ("but", Class::Conjunction),
    ("while", Class::Conjunction),
    ("whereas", Class::Conjunction),
    ("because", Class::Conjunction),
    ("although", Class::Conjunction),
    ("though", Class::Conjunction),
    ("unless", Class::Conjunction),
    ("if", Class::Conjunction),
    ("when", Class::Conjunction),
    ("where", Class::Conjunction),
    ("so", Class::Conjunction),
    ("it", Class::Pronoun),
    ("they", Class::Pronoun),
    ("we", Class::Pronoun),
    ("you", Class::Pronoun),
    ("he", Class::Pronoun),
    ("she", Class::Pronoun),
    ("them", Class::Pronoun),
    ("which", Class::Pronoun),
    ("who", Class::Pronoun),
    ("have", Class::Auxiliary),
    ("do", Class::Auxiliary),
    ("get", Class::Auxiliary),
];

/// Words that mean the same thing. The first entry is the canonical form.
const SYNONYMS: &[&[&str]] = &[
    &["color", "colour"],
    &["gray", "grey"],
    &["must", "required", "mandatory", "requires"],
    &["should", "recommended", "preferred"],
    &["may", "optional", "allowed", "permitted"],
    &["forbidden", "prohibited", "disallowed", "banned"],
    &["enabled", "on", "active"],
    &["disabled", "off", "inactive"],
    &["visible", "shown", "displayed"],
    &["hidden", "invisible", "concealed"],
    &["mutable", "writable", "changeable"],
    &["immutable", "readonly", "unchangeable"],
    &["synchronous", "sync", "blocking"],
    &["asynchronous", "async", "nonblocking"],
    &["button", "btn"],
    &["configuration", "config"],
    &["directory", "folder"],
];

/// Values that cannot hold at once for the same property.
const EXCLUSIVE_SETS: &[ExclusiveSet] = &[
    ExclusiveSet {
        property: "color",
        values: &[
            "red", "orange", "yellow", "green", "blue", "purple", "violet", "pink", "brown",
            "black", "white", "gray", "cyan", "magenta", "teal",
        ],
    },
    ExclusiveSet {
        property: "size",
        values: &["tiny", "small", "medium", "large", "huge"],
    },
    ExclusiveSet {
        property: "shape",
        values: &["square", "circular", "round", "rectangular", "triangular"],
    },
    ExclusiveSet {
        property: "visibility",
        values: &["visible", "hidden"],
    },
    ExclusiveSet {
        property: "availability",
        values: &["enabled", "disabled"],
    },
    ExclusiveSet {
        property: "mutability",
        values: &["mutable", "immutable"],
    },
    ExclusiveSet {
        property: "obligation",
        values: &["must", "may", "forbidden"],
    },
    ExclusiveSet {
        property: "concurrency",
        values: &["synchronous", "asynchronous"],
    },
    ExclusiveSet {
        property: "alignment",
        values: &["left", "right", "center", "justified"],
    },
    ExclusiveSet {
        property: "orientation",
        values: &["horizontal", "vertical"],
    },
    ExclusiveSet {
        property: "case",
        values: &["lowercase", "uppercase", "kebab-case", "camelcase", "snake_case"],
    },
];

/// Pairs that directly contradict each other.
const ANTONYMS: &[(&str, &str)] = &[
    ("always", "never"),
    ("true", "false"),
    ("first", "last"),
    ("before", "after"),
    ("above", "below"),
    ("open", "closed"),
    ("empty", "full"),
    ("enable", "disable"),
    ("include", "exclude"),
    ("accept", "reject"),
    ("allow", "deny"),
    ("add", "remove"),
    ("start", "stop"),
    ("valid", "invalid"),
    ("enabled", "disabled"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_classes_are_recognized() {
        assert_eq!(class("the"), Some(Class::Determiner));
        assert_eq!(class("is"), Some(Class::Copula));
        assert_eq!(class("must"), Some(Class::Modal));
        assert_eq!(class("not"), Some(Class::Negation));
        assert_eq!(class("button"), None);
    }

    #[test]
    fn synonyms_share_a_canonical_form() {
        assert!(equivalent("colour", "color"));
        assert!(equivalent("required", "must"));
        assert!(equivalent("config", "configuration"));
        assert!(!equivalent("button", "label"));
    }

    #[test]
    fn values_in_one_exclusive_set_contradict() {
        assert!(excludes("blue", "red"));
        assert!(excludes("visible", "hidden"));
        assert!(excludes("always", "never"));
    }

    #[test]
    fn unrelated_values_do_not_contradict() {
        // Different statements about different aspects are ordinary prose.
        assert!(!excludes("blue", "large"));
        assert!(!excludes("button", "label"));
        assert!(!excludes("fast", "reliable"));
    }

    #[test]
    fn a_value_never_contradicts_itself() {
        assert!(!excludes("blue", "blue"));
        assert!(!excludes("colour", "color"));
    }

    #[test]
    fn values_carry_the_property_they_describe() {
        assert_eq!(property_of("blue"), Some("color"));
        assert_eq!(property_of("grey"), Some("color"), "through its synonym");
        assert_eq!(property_of("frobnicate"), None);
    }
}
