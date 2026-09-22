//! Restating claims as English.
//!
//! Everything else here reads prose into claims. This goes the other way, and
//! writes a claim back out as a sentence.
//!
//! The point is not to reproduce the specification -- it already says what it
//! says, better. The point is to show what was *understood*, in the plainest
//! form that understanding takes, so that a reader can see where the reading
//! went wrong. A restatement that sounds odd is a claim that was extracted
//! oddly, and that is the signal worth having.
//!
//! The noun phrases keep the words the author wrote, because a restatement
//! built from stems reads as `The analysi must report example` and tells a
//! reader nothing. What is not kept is the verb: it is written as the name of
//! the relation group it was read into, so `emits` comes back as `produces`
//! and the reader can see which family the sentence landed in. The mood,
//! polarity and restriction are likewise the reading rather than the prose.

use crate::claim::{Claim, ClaimKind, Modality, Phrase, Polarity};

/// Restates a claim as a single English sentence.
pub fn realize(claim: &Claim) -> String {
    let subject = noun_phrase(&claim.subject);
    let value = noun_phrase(&claim.value);
    let negative = claim.polarity == Polarity::Negative;
    // `only` sits after the modal and before the verb in both moods: `must
    // only contain`, `only contains`.
    let only = if claim.restricted { "only " } else { "" };

    // The verb agrees with a subject written in the plural.
    let plural = claim.subject.plural;
    let be = if plural { "are" } else { "is" };
    let does = if plural { "do" } else { "does" };

    let predicate = match claim.kind {
        ClaimKind::Predication => match (claim.modality, negative) {
            (Modality::Asserted, false) => format!("{be} {only}{value}"),
            (Modality::Asserted, true) => format!("{be} not {only}{value}"),
            (modality, false) => format!("{} be {only}{value}", modal(modality)),
            (modality, true) => format!("{} not be {only}{value}", modal(modality)),
        },
        ClaimKind::Relation => {
            // The relation is named by the group it was read into, which is
            // already a bare verb.
            let verb = claim.property.as_deref().unwrap_or("relate to");
            let agreed = if plural {
                verb.to_string()
            } else {
                third_person(verb)
            };
            match (claim.modality, negative) {
                (Modality::Asserted, false) => format!("{only}{agreed} {value}"),
                (Modality::Asserted, true) => format!("{does} not {only}{verb} {value}"),
                (modality, false) => format!("{} {only}{verb} {value}", modal(modality)),
                (modality, true) => format!("{} not {only}{verb} {value}", modal(modality)),
            }
        }
    };
    format!("{} {predicate}.", opening(&subject))
}

/// The modal verb a modality is written with.
fn modal(modality: Modality) -> &'static str {
    match modality {
        Modality::Required => "must",
        Modality::Recommended => "should",
        Modality::Permitted => "may",
        // An asserted claim takes no modal; callers handle it before here.
        Modality::Asserted => "does",
    }
}

/// A phrase written back out, as the author wrote it.
fn noun_phrase(phrase: &Phrase) -> String {
    let text = phrase.text.trim();
    if text.is_empty() {
        return phrase.head_with_modifiers();
    }
    text.to_string()
}

/// Capitalizes the first letter, leaving the rest alone so that a name written
/// in the source keeps its own capitals.
fn opening(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => text.to_string(),
    }
}

/// The third-person singular of a bare verb.
fn third_person(verb: &str) -> String {
    match verb {
        "have" => return "has".to_string(),
        "do" => return "does".to_string(),
        "be" => return "is".to_string(),
        "go" => return "goes".to_string(),
        _ => {}
    }
    if verb.ends_with('s')
        || verb.ends_with('x')
        || verb.ends_with('z')
        || verb.ends_with("ch")
        || verb.ends_with("sh")
    {
        return format!("{verb}es");
    }
    // A consonant before a final `y` turns it into `ies`: `identify` gives
    // `identifies`, while `stay` keeps its vowel.
    if let Some(stem) = verb.strip_suffix('y') {
        let vowel_before = stem.chars().last().is_some_and(|c| "aeiou".contains(c));
        if !vowel_before {
            return format!("{stem}ies");
        }
    }
    format!("{verb}s")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::extract;
    use crate::text::sentences;

    fn restate(text: &str) -> String {
        let sentence = sentences(text).into_iter().next().expect("sentence");
        let claim = extract(&sentence, None).expect("claim");
        realize(&claim)
    }

    #[test]
    fn a_predication_is_restated_as_what_something_is() {
        assert_eq!(restate("The save button is blue."), "Save button is blue.");
        assert_eq!(restate("The cursor is not visible."), "Cursor is not visible.");
    }

    #[test]
    fn a_relation_is_restated_with_its_verb_conjugated() {
        // `emit` is restated as the group it was read into, and the verb
        // agrees with a plural subject.
        assert_eq!(restate("The adapters emit a skill."), "Adapters produce a skill.");
        assert_eq!(
            restate("The compiler identifies a type."),
            "Compiler identifies a type."
        );
    }

    #[test]
    fn modality_becomes_a_modal_verb() {
        assert_eq!(
            restate("The adapter must emit a skill."),
            "Adapter must produce a skill."
        );
        assert_eq!(
            restate("The adapter should not emit a skill."),
            "Adapter should not produce a skill."
        );
    }

    #[test]
    fn a_restriction_is_restated_as_only() {
        assert_eq!(
            restate("Lists can only contain strings."),
            "Lists may only have strings."
        );
    }

    #[test]
    fn third_person_follows_the_spelling_rules() {
        assert_eq!(third_person("have"), "has");
        assert_eq!(third_person("produce"), "produces");
        assert_eq!(third_person("identify"), "identifies");
        assert_eq!(third_person("match"), "matches");
        assert_eq!(third_person("use"), "uses");
        assert_eq!(third_person("resolve"), "resolves");
    }
}
