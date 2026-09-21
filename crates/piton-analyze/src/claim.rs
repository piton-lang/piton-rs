//! Grammatical and semantic analysis: turning a sentence into a claim.
//!
//! A claim is the analysis's internal shape for "this subject has this value for
//! this property". Claims never replace the prose; they exist so two statements
//! can be compared.
//!
//! Extraction is deliberately narrow. It recognizes copular statements — the
//! form specification prose uses to say what something *is* — and declines
//! everything else. A sentence that produces no claim produces no diagnostic,
//! which is the right outcome when the analysis cannot say what a sentence
//! means.

use crate::lexicon::{self, Class};
use crate::text::{Sentence, Token};

/// Whether the claim asserts or denies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Affirmative,
    Negative,
}

impl Polarity {
    pub fn flipped(self) -> Polarity {
        match self {
            Polarity::Affirmative => Polarity::Negative,
            Polarity::Negative => Polarity::Affirmative,
        }
    }
}

/// How strongly the statement binds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Modality {
    /// `may`, `can`, `optional`.
    Permitted,
    /// `should`, `recommended`.
    Recommended,
    /// A plain statement of fact.
    Asserted,
    /// `must`, `required`.
    Required,
}

impl Modality {
    pub fn as_str(self) -> &'static str {
        match self {
            Modality::Permitted => "permitted",
            Modality::Recommended => "recommended",
            Modality::Asserted => "asserted",
            Modality::Required => "required",
        }
    }
}

/// A noun phrase reduced to a head and its modifiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phrase {
    /// The phrase as written, for showing back to the reader.
    pub text: String,
    /// The head word, lemmatized and canonicalized.
    pub head: String,
    /// The remaining content words, lemmatized and canonicalized, sorted so two
    /// orderings of the same modifiers compare equal.
    pub modifiers: Vec<String>,
}

impl Phrase {
    fn from_tokens(tokens: &[&Token]) -> Option<Phrase> {
        let content: Vec<&&Token> = tokens
            .iter()
            .filter(|token| !is_function_word(&token.normalized))
            .collect();
        let head = content.last()?;
        let mut modifiers: Vec<String> = content[..content.len() - 1]
            .iter()
            .map(|token| lexicon::canonical(&token.lemma).to_string())
            .collect();
        modifiers.sort();
        modifiers.dedup();

        let text = tokens
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        Some(Phrase {
            text,
            head: lexicon::canonical(&head.lemma).to_string(),
            modifiers,
        })
    }

    /// How strongly two phrases name the same thing, from 0 to 1.
    ///
    /// The heads must match, and then the modifiers decide. This is where false
    /// positives live: "the save button" and "the cancel button" share a head
    /// and say incompatible things, and calling them one subject would report a
    /// contradiction that is not there.
    ///
    /// So only two shapes count as the same subject. Identical modifiers are
    /// the same thing. One side's modifiers being a subset of the other's is a
    /// general statement and a specific one about the same thing, which is
    /// worth reporting at reduced confidence. Anything else -- each side
    /// carrying a modifier the other lacks -- is treated as two members of a
    /// family, and scores below the threshold for comparison.
    pub fn similarity(&self, other: &Phrase) -> f64 {
        if !lexicon::equivalent(&self.head, &other.head) {
            return 0.0;
        }
        if self.modifiers == other.modifiers {
            return 1.0;
        }

        let conflicting = self
            .modifiers
            .iter()
            .any(|left| other.modifiers.iter().any(|right| lexicon::excludes(left, right)));
        if conflicting {
            return 0.0;
        }

        let only_here = self
            .modifiers
            .iter()
            .filter(|modifier| !other.modifiers.contains(modifier))
            .count();
        let only_there = other
            .modifiers
            .iter()
            .filter(|modifier| !self.modifiers.contains(modifier))
            .count();

        if only_here == 0 || only_there == 0 {
            return 0.75;
        }

        let shared = self.modifiers.len() - only_here;
        let total = self.modifiers.len().max(other.modifiers.len()).max(1);
        // Deliberately below the comparison threshold: partial overlap is not
        // enough to decide two phrases name one thing.
        0.3 + 0.15 * (shared as f64 / total as f64)
    }
}

/// A statement reduced to a comparable form.
#[derive(Debug, Clone)]
pub struct Claim {
    pub subject: Phrase,
    /// The property the value describes, when the lexicon knows it.
    pub property: Option<String>,
    pub value: Phrase,
    pub polarity: Polarity,
    pub modality: Modality,
}

impl Claim {
    /// A one-line rendering, used in diagnostics and in the `claim` output.
    pub fn render(&self) -> String {
        let property = self.property.as_deref().unwrap_or("identity");
        let negation = match self.polarity {
            Polarity::Affirmative => "",
            Polarity::Negative => "not ",
        };
        format!(
            "subject: {}\nproperty: {property}\nvalue: {negation}{}",
            self.subject.head_with_modifiers(),
            self.value.head_with_modifiers()
        )
    }
}

impl Phrase {
    /// The normalized phrase, modifiers first, for display.
    pub fn head_with_modifiers(&self) -> String {
        if self.modifiers.is_empty() {
            return self.head.clone();
        }
        format!("{} {}", self.modifiers.join(" "), self.head)
    }
}

/// Extracts a claim from a sentence, if it makes one.
pub fn extract(sentence: &Sentence) -> Option<Claim> {
    let words: Vec<&Token> = sentence.words().collect();
    if words.len() < 3 {
        return None;
    }

    // Quoted material is being mentioned, not asserted. A specification that
    // writes `we say "the button is blue"` is describing a statement, and
    // treating the description as the statement would report contradictions
    // the document is only talking about.
    let quoted = quoted_spans(&sentence.text);

    // Find the copula that separates subject from value. The first one wins,
    // because a later copula belongs to a subordinate clause.
    let copula = words.iter().position(|token| {
        matches!(lexicon::class(&token.normalized), Some(Class::Copula))
            && !quoted.iter().any(|(start, end)| token.offset >= *start && token.offset < *end)
    })?;
    if copula == 0 {
        // A leading copula is a question or an inversion, not an assertion.
        return None;
    }

    // A modal immediately before the copula sets the modality: `must be blue`.
    let mut modality = Modality::Asserted;
    let mut subject_end = copula;
    if copula >= 1 {
        if let Some(found) = lexicon::modality(&words[copula - 1].normalized) {
            modality = found;
            subject_end = copula - 1;
        } else if matches!(
            lexicon::class(&words[copula - 1].normalized),
            Some(Class::Modal)
        ) {
            subject_end = copula - 1;
        }
    }
    if subject_end == 0 {
        return None;
    }

    // The subject is the noun phrase that ends at the copula, not everything
    // before it. Walking back to the first function word is what keeps a long
    // sentence from collapsing its whole first clause into one subject.
    let subject_start = words[..subject_end]
        .iter()
        .rposition(|token| is_phrase_boundary(&token.normalized))
        .map(|index| index + 1)
        .unwrap_or(0);
    if subject_start >= subject_end {
        return None;
    }

    // A subject that is a pronoun refers to something this analysis cannot
    // resolve, so there is nothing to compare.
    if words[subject_start..subject_end]
        .iter()
        .all(|token| matches!(lexicon::class(&token.normalized), Some(Class::Pronoun)))
    {
        return None;
    }

    let mut index = copula + 1;
    let mut polarity = Polarity::Affirmative;
    while index < words.len()
        && matches!(
            lexicon::class(&words[index].normalized),
            Some(Class::Negation)
        )
    {
        polarity = polarity.flipped();
        index += 1;
    }
    // `is not always` reads as a denial of the value, and so does `never`.
    let value_start = index;
    if value_start >= words.len() {
        return None;
    }

    // The value runs to the end of the clause: a conjunction or a preposition
    // starts material this analysis does not try to attach.
    let value_end = words[value_start..]
        .iter()
        .position(|token| {
            matches!(
                lexicon::class(&token.normalized),
                Some(Class::Conjunction) | Some(Class::Preposition)
            )
        })
        .map(|offset| value_start + offset)
        .unwrap_or(words.len());
    if value_end <= value_start {
        return None;
    }

    let subject = Phrase::from_tokens(&words[subject_start..subject_end])?;
    let value = Phrase::from_tokens(&words[value_start..value_end])?;

    // A subject with no content word, or a value that is only a function word,
    // says nothing comparable.
    if subject.head.is_empty() || value.head.is_empty() {
        return None;
    }

    let property = lexicon::property_of(&value.head).map(str::to_string);
    Some(Claim {
        subject,
        property,
        value,
        polarity,
        modality,
    })
}

/// The ranges of `text` that sit inside quotation marks.
fn quoted_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    for (offset, ch) in text.char_indices() {
        match ch {
            '"' | '\u{201c}' | '\u{201d}' => match open {
                Some(start) => {
                    out.push((start, offset));
                    open = None;
                }
                None => open = Some(offset + ch.len_utf8()),
            },
            _ => {}
        }
    }
    out
}

/// True when a word ends the noun phrase to its right.
fn is_phrase_boundary(word: &str) -> bool {
    matches!(
        lexicon::class(word),
        Some(Class::Determiner)
            | Some(Class::Preposition)
            | Some(Class::Conjunction)
            | Some(Class::Copula)
            | Some(Class::Modal)
            | Some(Class::Negation)
            | Some(Class::Auxiliary)
    )
}

/// True when a word carries no content of its own.
fn is_function_word(word: &str) -> bool {
    matches!(
        lexicon::class(word),
        Some(Class::Determiner)
            | Some(Class::Copula)
            | Some(Class::Preposition)
            | Some(Class::Conjunction)
            | Some(Class::Auxiliary)
    )
}

/// How strongly two claims contradict, from 0 to 1.
///
/// Zero means the analysis found no reason to believe they conflict, which is
/// the answer whenever the lexicon has nothing to say about the two values.
pub fn contradiction(left: &Claim, right: &Claim) -> f64 {
    // A claim about a property only conflicts with a claim about the same
    // property.
    match (&left.property, &right.property) {
        (Some(a), Some(b)) if a != b => return 0.0,
        _ => {}
    }

    let same_value = left.value.similarity(&right.value) >= 0.999;
    let opposite_polarity = left.polarity != right.polarity;

    if same_value && opposite_polarity {
        // One says it is, the other says it is not.
        return 1.0;
    }
    if same_value {
        return 0.0;
    }

    let exclusive = lexicon::excludes(&left.value.head, &right.value.head);
    if !exclusive {
        return 0.0;
    }
    if opposite_polarity {
        // `is blue` and `is not red` are compatible: both can hold.
        return 0.0;
    }

    // Modifiers can narrow a value enough that two exclusive heads describe
    // different things, so an unmatched modifier softens the conclusion.
    let modifiers_agree = left.value.modifiers == right.value.modifiers;
    let base: f64 = if modifiers_agree { 1.0 } else { 0.7 };

    // A permission and a requirement can coexist more readily than two
    // requirements.
    let modality_penalty: f64 = if left.modality == right.modality {
        0.0
    } else if left.modality == Modality::Permitted || right.modality == Modality::Permitted {
        0.35
    } else {
        0.15
    };
    (base - modality_penalty).clamp(0.0, 1.0)
}

/// How strongly two claims are about the same subject, from 0 to 1.
pub fn subject_identity(left: &Claim, right: &Claim) -> f64 {
    left.subject.similarity(&right.subject)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::sentences;

    fn claim(text: &str) -> Option<Claim> {
        extract(sentences(text).first()?)
    }

    #[test]
    fn the_specifications_own_example_extracts() {
        let blue = claim("The Save Button is Blue").expect("claim");
        assert_eq!(blue.subject.head, "button");
        assert_eq!(blue.subject.modifiers, vec!["save"]);
        assert_eq!(blue.property.as_deref(), Some("color"));
        assert_eq!(blue.value.head, "blue");
        assert_eq!(blue.polarity, Polarity::Affirmative);

        let red = claim("The Save Button is Red").expect("claim");
        assert_eq!(red.value.head, "red");
        assert!(contradiction(&blue, &red) >= 0.9);
        assert!(subject_identity(&blue, &red) >= 0.999);
    }

    #[test]
    fn the_rendered_claim_matches_the_documented_shape() {
        let blue = claim("The Save Button is Blue").expect("claim");
        assert_eq!(
            blue.render(),
            "subject: save button\nproperty: color\nvalue: blue"
        );
    }

    #[test]
    fn modality_comes_from_the_modal_verb() {
        assert_eq!(claim("The button must be blue").unwrap().modality, Modality::Required);
        assert_eq!(
            claim("The button should be blue").unwrap().modality,
            Modality::Recommended
        );
        assert_eq!(claim("The button may be blue").unwrap().modality, Modality::Permitted);
        assert_eq!(claim("The button is blue").unwrap().modality, Modality::Asserted);
    }

    #[test]
    fn negation_flips_polarity() {
        let claim = claim("The button is not blue").expect("claim");
        assert_eq!(claim.polarity, Polarity::Negative);
        assert_eq!(claim.value.head, "blue");
    }

    #[test]
    fn a_denial_contradicts_the_matching_assertion() {
        let asserted = claim("The button is blue").unwrap();
        let denied = claim("The button is not blue").unwrap();
        assert_eq!(contradiction(&asserted, &denied), 1.0);
    }

    #[test]
    fn a_denial_of_a_different_value_is_compatible() {
        // "is blue" and "is not red" can both be true.
        let blue = claim("The button is blue").unwrap();
        let not_red = claim("The button is not red").unwrap();
        assert_eq!(contradiction(&blue, &not_red), 0.0);
    }

    #[test]
    fn unrelated_values_never_contradict() {
        let blue = claim("The button is blue").unwrap();
        let large = claim("The button is large").unwrap();
        assert_eq!(
            contradiction(&blue, &large),
            0.0,
            "different properties are not a conflict"
        );

        let fast = claim("The parser is fast").unwrap();
        let careful = claim("The parser is careful").unwrap();
        assert_eq!(
            contradiction(&fast, &careful),
            0.0,
            "the lexicon knows nothing about these, so neither does the analysis"
        );
    }

    #[test]
    fn different_subjects_are_not_the_same_subject() {
        let save = claim("The save button is blue").unwrap();
        let cancel = claim("The cancel button is red").unwrap();
        assert!(
            subject_identity(&save, &cancel) < 0.5,
            "two members of a family must not compare as one subject"
        );
    }

    #[test]
    fn a_general_statement_and_a_specific_one_partly_agree() {
        let general = claim("The button is blue").unwrap();
        let specific = claim("The primary save button is blue").unwrap();
        let identity = subject_identity(&general, &specific);
        assert!((identity - 0.75).abs() < 1e-9, "got {identity}");
    }

    #[test]
    fn a_narrower_subject_is_partly_the_same() {
        let button = claim("The button is blue").unwrap();
        let save = claim("The save button is red").unwrap();
        let identity = subject_identity(&button, &save);
        assert!(identity > 0.0 && identity < 1.0, "got {identity}");
    }

    #[test]
    fn synonyms_are_the_same_value() {
        let grey = claim("The border is grey").unwrap();
        let gray = claim("The border is gray").unwrap();
        assert_eq!(contradiction(&grey, &gray), 0.0);
    }

    #[test]
    fn the_subject_is_the_noun_phrase_before_the_copula() {
        let extracted =
            claim("We might have a spec for a large application, and the save button is blue")
                .expect("claim");
        assert_eq!(extracted.subject.head, "button");
        assert_eq!(extracted.subject.modifiers, vec!["save"]);
    }

    #[test]
    fn quoted_statements_are_mentioned_not_asserted() {
        assert!(
            claim("In one location we say \"The Save Button is Blue\".").is_none(),
            "a quoted statement is being described, not made"
        );
        // The same sentence without the quotes does assert something.
        assert!(claim("In one location the Save Button is Blue.").is_some());
    }

    #[test]
    fn sentences_that_make_no_claim_produce_none() {
        assert!(claim("Read the specification carefully.").is_none());
        assert!(claim("Is the button blue?").is_none(), "a question asserts nothing");
        assert!(claim("It is blue.").is_none(), "an unresolved pronoun");
        assert!(claim("Blue.").is_none());
    }

    #[test]
    fn a_permission_softens_a_conflict_with_a_requirement() {
        let must = claim("The button must be blue").unwrap();
        let may = claim("The button may be red").unwrap();
        let strict = claim("The button must be red").unwrap();
        assert!(contradiction(&must, &may) < contradiction(&must, &strict));
    }
}
