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
    /// Modifies a verb or an adjective. Never the head of a noun phrase, which
    /// is what makes `lists can only contain strings` about lists rather than
    /// about `only`.
    Adverb,
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
    /// Verb to the canonical relation it expresses.
    relations: HashMap<&'static str, &'static str>,
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

        let mut relations = HashMap::new();
        for group in RELATIONS {
            let canonical = group[0];
            for verb in *group {
                relations.insert(*verb, canonical);
            }
        }

        Tables {
            classes,
            relations,
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

/// True when a word restricts its statement to the value it names.
///
/// `only`, `solely`, `exclusively` turn a relation that would otherwise take
/// many objects into one that takes a single answer: a package *contains* many
/// things, but a list that *can only contain* strings cannot also only contain
/// booleans.
pub fn is_restrictive(word: &str) -> bool {
    RESTRICTIVE.contains(&canonical(word))
}

/// True when a word modifies rather than names.
/// Words ending in `-ly` that are not adverbs.
///
/// Most are verbs in their own right, and reading one as an adverb would step
/// over the very word the claim turns on.
const NOT_ADVERBS: &[&str] = &[
    "reply", "imply", "comply", "multiply", "family", "assembly", "anomaly", "ally", "belly",
    "holy", "ugly", "jelly", "rally", "folly", "monopoly", "panoply", "italy", "july", "poly",
];

pub fn is_adverb(word: &str) -> bool {
    if matches!(class(word), Some(Class::Adverb)) {
        return true;
    }
    // Adverbs are an open class, and a specification coins them freely:
    // `intentionally`, `structurally`, `sufficiently`. Listing them all is not
    // possible, but `-ly` derives almost all of them. A word the lexicon
    // already knows as something else keeps that meaning, so this only ever
    // names words nothing else claims.
    word.len() > 4
        && word.ends_with("ly")
        && class(word).is_none()
        && relation(word).is_none()
        && !NOT_ADVERBS.contains(&word)
}

/// The relation a verb expresses, if it is one the analysis can compare.
///
/// A relation is a claim shape of its own: `the adapter emits a skill` says
/// something comparable to `the adapter emits a command`, in the same way that
/// `the button is blue` is comparable to `the button is red`. Verbs outside this
/// table produce no claim, because the analysis has nothing to say about what
/// they mean.
pub fn relation(word: &str) -> Option<&'static str> {
    tables().relations.get(word).copied()
}

/// True when a word is an auxiliary that carries no relation of its own.
pub fn is_auxiliary(word: &str) -> bool {
    matches!(class(word), Some(Class::Auxiliary)) || matches!(word, "does" | "did" | "do")
}

/// How strongly a word signals an obligation rather than a description.
pub fn modality(word: &str) -> Option<crate::claim::Modality> {
    use crate::claim::Modality;
    match canonical(word) {
        "must" | "shall" => Some(Modality::Required),
        "should" => Some(Modality::Recommended),
        // `can` grants permission the way `may` does, which is what makes
        // `can only contain strings` a bound rather than a statement of fact.
        "may" | "can" | "could" | "might" => Some(Modality::Permitted),
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
    ("both", Class::Determiner),
    ("either", Class::Determiner),
    ("neither", Class::Determiner),
    ("another", Class::Determiner),
    ("such", Class::Determiner),
    ("many", Class::Determiner),
    ("much", Class::Determiner),
    ("most", Class::Determiner),
    ("few", Class::Determiner),
    ("several", Class::Determiner),
    ("various", Class::Determiner),
    ("my", Class::Determiner),
    ("his", Class::Determiner),
    ("her", Class::Determiner),
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
    ("become", Class::Copula),
    ("becomes", Class::Copula),
    ("became", Class::Copula),
    ("remain", Class::Copula),
    ("remains", Class::Copula),
    ("remained", Class::Copula),
    ("seem", Class::Copula),
    ("seems", Class::Copula),
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
    // Written whole, because the tokenizer keeps an apostrophe inside a word:
    // a contraction is never split into a separate `n't`.
    ("don't", Class::Negation),
    ("doesn't", Class::Negation),
    ("didn't", Class::Negation),
    ("isn't", Class::Negation),
    ("aren't", Class::Negation),
    ("wasn't", Class::Negation),
    ("weren't", Class::Negation),
    ("won't", Class::Negation),
    ("can't", Class::Negation),
    ("couldn't", Class::Negation),
    ("shouldn't", Class::Negation),
    ("wouldn't", Class::Negation),
    ("mustn't", Class::Negation),
    ("shan't", Class::Negation),
    ("hasn't", Class::Negation),
    ("haven't", Class::Negation),
    ("hadn't", Class::Negation),
    ("needn't", Class::Negation),
    ("none", Class::Negation),
    ("nor", Class::Negation),
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
    ("as", Class::Preposition),
    ("about", Class::Preposition),
    ("against", Class::Preposition),
    ("among", Class::Preposition),
    ("around", Class::Preposition),
    ("above", Class::Preposition),
    ("below", Class::Preposition),
    ("beyond", Class::Preposition),
    ("between", Class::Preposition),
    ("during", Class::Preposition),
    ("inside", Class::Preposition),
    ("outside", Class::Preposition),
    ("onto", Class::Preposition),
    ("upon", Class::Preposition),
    ("per", Class::Preposition),
    ("via", Class::Preposition),
    ("toward", Class::Preposition),
    ("towards", Class::Preposition),
    ("than", Class::Preposition),
    ("alongside", Class::Preposition),
    ("without", Class::Preposition),
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
    ("yet", Class::Conjunction),
    ("since", Class::Conjunction),
    ("once", Class::Conjunction),
    ("whether", Class::Conjunction),
    ("until", Class::Conjunction),
    ("after", Class::Conjunction),
    ("before", Class::Conjunction),
    ("it", Class::Pronoun),
    ("they", Class::Pronoun),
    ("we", Class::Pronoun),
    ("you", Class::Pronoun),
    ("he", Class::Pronoun),
    ("she", Class::Pronoun),
    ("them", Class::Pronoun),
    ("which", Class::Pronoun),
    ("who", Class::Pronoun),
    ("whom", Class::Pronoun),
    ("whose", Class::Pronoun),
    ("what", Class::Pronoun),
    ("i", Class::Pronoun),
    ("me", Class::Pronoun),
    ("us", Class::Pronoun),
    ("him", Class::Pronoun),
    ("itself", Class::Pronoun),
    ("themselves", Class::Pronoun),
    // Existential `there` fills the subject slot without naming a subject.
    ("there", Class::Pronoun),
    ("have", Class::Auxiliary),
    ("do", Class::Auxiliary),
    ("get", Class::Auxiliary),
    ("has", Class::Auxiliary),
    ("had", Class::Auxiliary),
    ("having", Class::Auxiliary),
    ("does", Class::Auxiliary),
    ("did", Class::Auxiliary),
    ("done", Class::Auxiliary),
    ("only", Class::Adverb),
    ("solely", Class::Adverb),
    ("exclusively", Class::Adverb),
    ("just", Class::Adverb),
    ("merely", Class::Adverb),
    ("also", Class::Adverb),
    ("always", Class::Adverb),
    ("still", Class::Adverb),
    ("already", Class::Adverb),
    ("simply", Class::Adverb),
    ("largely", Class::Adverb),
    ("generally", Class::Adverb),
    ("typically", Class::Adverb),
    ("usually", Class::Adverb),
    ("often", Class::Adverb),
    ("sometimes", Class::Adverb),
    ("rather", Class::Adverb),
    ("quite", Class::Adverb),
    ("very", Class::Adverb),
    ("therefore", Class::Adverb),
    ("instead", Class::Adverb),
    ("further", Class::Adverb),
    ("again", Class::Adverb),
    ("directly", Class::Adverb),
    ("explicitly", Class::Adverb),
    ("implicitly", Class::Adverb),
    ("automatically", Class::Adverb),
    ("silently", Class::Adverb),
    // Conjunctive and temporal adverbs. They open a sentence without being its
    // subject, which is what `Then build the tooling` turns on.
    ("then", Class::Adverb),
    ("now", Class::Adverb),
    ("next", Class::Adverb),
    ("thus", Class::Adverb),
    ("hence", Class::Adverb),
    ("however", Class::Adverb),
    ("otherwise", Class::Adverb),
    ("moreover", Class::Adverb),
    ("furthermore", Class::Adverb),
    ("meanwhile", Class::Adverb),
    ("afterwards", Class::Adverb),
    ("here", Class::Adverb),
    ("soon", Class::Adverb),
    ("later", Class::Adverb),
    ("earlier", Class::Adverb),
    ("together", Class::Adverb),
];

/// Adverbs that restrict a statement to the single value it names.
const RESTRICTIVE: &[&str] = &["only", "solely", "exclusively", "just", "merely"];

/// Words that deny what follows them without being a `Negation` themselves.
///
/// `without a cache` and `has no label` are negative claims, and reading them
/// as positive ones about `without cache` and `label` loses the only thing
/// they were saying.
const NEGATIVE: &[&str] = &["no", "none", "neither", "nor", "without", "nothing"];

/// True when this word denies whatever follows it.
pub fn is_negative(word: &str) -> bool {
    NEGATIVE.contains(&word) || matches!(class(word), Some(Class::Negation))
}

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

/// Verbs that express a comparable relation. The first entry names it.
///
/// Both the written form and its stem appear, because the stemmer is
/// deliberately conservative and leaves some forms alone.
/// Verbs grouped by the relation they assert, the first word naming the group.
///
/// Two claims are only comparable when they assert the same relation, so a
/// group is a claim that its members say the same thing about a subject and
/// its object. That is why these are narrower than a thesaurus would draw
/// them: `emits a skill` and `exposes a skill` are close in English and
/// different in a specification, so they stay apart and simply do not compare.
///
/// Both the bare and the `-s` form are written out. The stemmer would reach
/// most of them, but it is deliberately conservative and leaves some forms
/// alone, and a verb that silently fails to resolve is a sentence that goes
/// unread.
const RELATIONS: &[&[&str]] = &[
    &["have", "has", "contain", "contains", "include", "includes", "hold", "holds", "carry", "carries", "own", "owns", "share", "shares", "nest", "nests", "embed", "embeds", "wrap", "wraps"],
    &["require", "requires", "need", "needs", "expect", "expects", "depend", "depends", "rely", "relies", "assume", "assumes"],
    &["produce", "produces", "emit", "emits", "return", "returns", "yield", "yields", "create", "creates", "generate", "generates", "build", "builds", "make", "makes", "write", "writes", "render", "renders", "invent", "invents", "construct", "constructs"],
    &["provide", "provides", "expose", "exposes", "offer", "offers", "supply", "supplies", "give", "gives", "implement", "implements", "satisfy", "satisfies", "meet", "meets", "fulfill", "fulfills", "fulfil", "fulfils"],
    &["use", "uses", "consume", "consumes", "read", "reads", "apply", "applies", "adopt", "adopts", "operate", "operates"],
    &["load", "loads", "fetch", "fetches"],
    &["run", "runs", "execute", "executes", "invoke", "invokes", "perform", "performs"],
    &["support", "supports", "accept", "accepts", "allow", "allows", "permit", "permits", "enable", "enables", "take", "takes"],
    &["reject", "rejects", "refuse", "refuses", "forbid", "forbids", "disallow", "disallows", "deny", "denies", "prevent", "prevents", "exclude", "excludes"],
    &["ignore", "ignores", "skip", "skips", "drop", "drops", "discard", "discards", "omit", "omits", "lose", "loses"],
    &["resolve", "resolves", "map", "maps", "compile", "compiles", "convert", "converts", "translate", "translates", "transform", "transforms", "normalize", "normalizes", "serialize", "serializes", "evaluate", "evaluates", "interpret", "interprets", "turn", "turns", "escape", "escapes", "encode", "encodes", "decode", "decodes", "reinterpret", "reinterprets", "treat", "treats", "expand", "expands", "flatten", "flattens", "parse", "parses"],
    &["define", "defines", "declare", "declares", "specify", "specifies", "describe", "describes", "establish", "establishes", "discuss", "discusses", "explain", "explains", "state", "states"],
    &["validate", "validates", "check", "checks", "verify", "verifies", "ensure", "ensures", "guarantee", "guarantees", "enforce", "enforces", "assess", "assesses"],
    &["report", "reports", "show", "shows", "display", "displays", "print", "prints", "surface", "surfaces", "warn", "warns", "log", "logs", "note", "notes"],
    &["identify", "identifies", "detect", "detects", "find", "finds", "discover", "discovers", "recognize", "recognizes", "locate", "locates", "search", "searches", "distinguish", "distinguishes", "analyze", "analyzes", "examine", "examines", "inspect", "inspects", "infer", "infers", "deduce", "deduces", "conclude", "concludes"],
    &["replace", "replaces", "override", "overrides", "supersede", "supersedes", "overwrite", "overwrites", "shadow", "shadows", "revise", "revises", "update", "updates"],
    &["refer", "refers", "mention", "mentions", "see", "sees", "point", "points", "cite", "cites"],
    &["extend", "extends", "inherit", "inherits", "derive", "derives"],
    &["combine", "combines", "merge", "merges", "join", "joins", "compose", "composes", "stack", "stacks"],
    &["follow", "follows", "obey", "obeys", "respect", "respects", "honor", "honors", "honour", "honours"],
    &["configure", "configures", "assign", "assigns", "initialize", "initializes"],
    &["store", "stores", "save", "saves", "persist", "persists", "keep", "keeps", "preserve", "preserves", "retain", "retains"],
    &["cause", "causes", "trigger", "triggers", "raise", "raises", "throw", "throws", "fail", "fails", "panic", "panics"],
    &["prefer", "prefers", "favor", "favors", "favour", "favours"],
    &["add", "adds", "append", "appends", "insert", "inserts", "attach", "attaches", "introduce", "introduces", "register", "registers"],
    &["remove", "removes", "delete", "deletes", "strip", "strips", "subtract", "subtracts", "trim", "trims"],
    &["constrain", "constrains", "restrict", "restricts", "limit", "limits"],
    &["defer", "defers", "delegate", "delegates"],
    &["place", "places", "put", "puts", "position", "positions", "move", "moves"],
    &["solve", "solves", "address", "addresses", "fix", "fixes"],
    &["propose", "proposes", "suggest", "suggests", "recommend", "recommends"],
    &["mirror", "mirrors", "match", "matches", "reflect", "reflects"],
    &["represent", "represents", "express", "expresses", "denote", "denotes"],
    &["strengthen", "strengthens", "reinforce", "reinforces"],
    &["weaken", "weakens", "degrade", "degrades"],
    &["filter", "filters", "select", "selects"],
    &["split", "splits", "divide", "divides", "separate", "separates"],
    &["iterate", "iterates", "traverse", "traverses", "walk", "walks", "visit", "visits"],
    &["count", "counts", "measure", "measures", "rank", "ranks"],
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
    fn no_word_belongs_to_two_relation_groups() {
        // The groups become one map, so a word in two of them silently takes
        // whichever meaning was written last.
        let mut seen: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
        for group in RELATIONS {
            for word in *group {
                if let Some(first) = seen.insert(word, group[0]) {
                    panic!("`{word}` is in both `{first}` and `{}`", group[0]);
                }
            }
        }
    }

    #[test]
    fn a_relation_verb_is_not_also_a_closed_class_word() {
        // A verb classed as a function word is stripped before it can be read
        // as a verb, so the sentence goes unread. `have` and `do` are the
        // designed exception: they are auxiliaries that also name a relation,
        // which is why an auxiliary yields the verb slot to a content verb
        // after it and keeps it otherwise.
        for group in RELATIONS {
            for word in *group {
                let Some(found) = class(word) else { continue };
                assert_eq!(
                    found,
                    Class::Auxiliary,
                    "`{word}` is both a relation verb and a {found:?}"
                );
            }
        }
    }

    #[test]
    fn restrictive_adverbs_are_recognized() {
        assert!(is_adverb("only"));
        assert!(is_restrictive("only"));
        assert!(is_restrictive("exclusively"));
        // An adverb that does not restrict.
        assert!(is_adverb("usually"));
        assert!(!is_restrictive("usually"));
        assert!(!is_adverb("list"));
    }

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
