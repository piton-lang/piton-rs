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
    /// True when a word inside the phrase denies it: `no adapter`, `neither
    /// adapter`. Such a word is a function word and would otherwise be
    /// stripped, turning `no cursor is visible` into the claim that one is.
    pub negated: bool,
    /// True when the head was written in the plural. Comparison does not care
    /// -- `list` and `lists` are one subject -- but restating the claim does,
    /// because the verb has to agree with it.
    pub plural: bool,
}

impl Phrase {
    fn from_tokens(tokens: &[&Token]) -> Option<Phrase> {
        let content: Vec<&&Token> = tokens
            .iter()
            .filter(|token| !is_function_word(&token.normalized))
            .collect();
        let head = content.last()?;
        let negated = tokens
            .iter()
            .any(|token| lexicon::is_negative(&token.normalized));
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
        // A plural head is one the stemmer took an `-s` off. Words that end
        // in `-ss` or `-sis` are singular already and keep it.
        let written = &head.normalized;
        let plural = written.ends_with('s')
            && !written.ends_with("ss")
            && !written.ends_with("sis")
            && written.strip_suffix('s') == Some(head.lemma.as_str());
        Some(Phrase {
            text,
            head: lexicon::canonical(&head.lemma).to_string(),
            modifiers,
            negated,
            plural,
        })
    }

    /// Builds a phrase from an anchor's name, for a sentence whose subject is
    /// the anchor it was written in.
    ///
    /// Names are written as one word -- `SkillBehavior`, `claude-adapter` --
    /// so they are split back into the words they were made of before being
    /// lemmatized, which is what lets an anchor's name meet the same words
    /// written out in prose.
    pub fn from_name(name: &str) -> Option<Phrase> {
        let mut words: Vec<String> = Vec::new();
        let mut current = String::new();
        for ch in name.chars() {
            if ch == '-' || ch == '_' || ch == '.' || ch == ' ' {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                continue;
            }
            if ch.is_uppercase() && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(ch.to_ascii_lowercase());
        }
        if !current.is_empty() {
            words.push(current);
        }
        let mut content: Vec<String> = words
            .into_iter()
            .filter(|word| !is_function_word(word))
            .map(|word| lexicon::canonical(&crate::text::lemma(&word)).to_string())
            .collect();
        let head = content.pop()?;
        content.sort();
        content.dedup();
        Some(Phrase {
            text: name.to_string(),
            head,
            modifiers: content,
            negated: false,
            plural: false,
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

/// Which shape a claim took.
///
/// The difference matters when the lexicon cannot relate two values. A
/// predication answers *what is it*, and one subject has one answer: `is
/// written in Rust` and `is written in Python` compete. A relation answers
/// *what does it involve*, and one subject has many: `contains a skill` and
/// `contains a command` are both true at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimKind {
    Predication,
    Relation,
}

/// A statement reduced to a comparable form.
#[derive(Debug, Clone)]
pub struct Claim {
    pub kind: ClaimKind,
    /// True when the statement restricts the subject to this value: `can only
    /// contain strings` admits strings and nothing else.
    ///
    /// This is what makes a relation behave like a predication. A package
    /// contains many things, so two `contains` claims agree; a list that can
    /// *only* contain strings cannot also only contain booleans.
    pub restricted: bool,
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
    /// True when the phrase is not one the analysis should build a claim on.
    ///
    /// Two shapes are refused. One longer than `limit` is a mis-read clause.
    /// One containing a pronoun refers to something the analysis cannot
    /// resolve, which is what `make sure you always include` looks like after a
    /// verb has been mistaken for a head noun.
    pub fn is_unusable(&self, limit: usize) -> bool {
        if self.modifiers.len() + 1 > limit {
            return true;
        }
        std::iter::once(&self.head)
            .chain(&self.modifiers)
            .any(|word| matches!(lexicon::class(word), Some(Class::Pronoun)))
    }

    /// The normalized phrase, modifiers first, for display.
    pub fn head_with_modifiers(&self) -> String {
        if self.modifiers.is_empty() {
            return self.head.clone();
        }
        format!("{} {}", self.modifiers.join(" "), self.head)
    }
}

/// Extracts a claim from a sentence, if it makes one.
///
/// Two shapes are recognized. A copular statement says what something *is*; a
/// relational one says what it *does* to something else. Anything outside both
/// produces no claim, which is the right answer when the analysis cannot say
/// what a sentence means.
pub fn extract(sentence: &Sentence, context: Option<&Phrase>) -> Option<Claim> {
    extract_copular(sentence).or_else(|| extract_relational(sentence, context))
}

/// Why a sentence produced no claim.
///
/// A sentence the analysis cannot read is a designed outcome rather than a
/// failure, but it is only an honest one if it can be counted. These are the
/// three ways reading stops, in the order the extractors hit them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gap {
    /// Fewer than three words: a heading, a label, a list item.
    NotASentence,
    /// No copula, and no verb the lexicon names. `candidate` is a word that
    /// sits where a verb would, when one can be identified with confidence.
    UnknownVerb { candidate: Option<String> },
    /// A verb was found, but the noun phrase on one side of it was not usable
    /// -- a pronoun, or longer than a noun phrase runs.
    UnreadablePhrase,
}

impl Gap {
    pub fn as_str(&self) -> &'static str {
        match self {
            Gap::NotASentence => "not a sentence",
            Gap::UnknownVerb { .. } => "no verb the lexicon knows",
            Gap::UnreadablePhrase => "subject or value is not a noun phrase",
        }
    }
}

/// Reads a sentence, or says why it could not be read.
pub fn explain(sentence: &Sentence, context: Option<&Phrase>) -> Result<Claim, Gap> {
    if let Some(claim) = extract(sentence, context) {
        return Ok(claim);
    }
    let words: Vec<&Token> = sentence.words().collect();
    if words.len() < 3 {
        return Err(Gap::NotASentence);
    }
    let has_copula = words
        .iter()
        .any(|token| matches!(lexicon::class(&token.normalized), Some(Class::Copula)));
    let has_relation = words.iter().any(|token| {
        lexicon::relation(&token.normalized)
            .or_else(|| lexicon::relation(&token.lemma))
            .is_some()
    });
    if has_copula || has_relation {
        // The shape was there; the phrases around it were not.
        return Err(Gap::UnreadablePhrase);
    }
    Err(Gap::UnknownVerb {
        candidate: verb_candidate(&words),
    })
}

/// A word sitting where a verb would, when the position alone makes it clear.
///
/// There is no part-of-speech tagger here, so this only reports a word it can
/// place structurally: one directly after a modal or auxiliary, which is a verb
/// in any English sentence. Anything less certain is left unnamed rather than
/// guessed at, because the point of the count is to be trusted.
fn verb_candidate(words: &[&Token]) -> Option<String> {
    for (index, token) in words.iter().enumerate() {
        let is_marker = matches!(lexicon::class(&token.normalized), Some(Class::Modal))
            || lexicon::is_auxiliary(&token.lemma);
        if !is_marker {
            continue;
        }
        let next = words[index + 1..].iter().find(|later| {
            !is_function_word(&later.normalized)
                // `allows you to compile` puts a pronoun between the marker
                // and the verb. A pronoun is never the verb.
                && !matches!(lexicon::class(&later.normalized), Some(Class::Pronoun))
        })?;
        return Some(next.lemma.clone());
    }
    None
}

/// `<noun phrase> [modal] is [not] <value>`
fn extract_copular(sentence: &Sentence) -> Option<Claim> {
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

    // Step back over the modals and adverbs between the subject and its
    // copula: `must only be blue`.
    let mut modality = Modality::Asserted;
    let mut restricted = false;
    let mut subject_end = copula;
    while subject_end > 0 {
        let word = &words[subject_end - 1].normalized;
        if let Some(found) = lexicon::modality(word) {
            modality = found;
        } else if lexicon::is_adverb(word) {
            restricted |= lexicon::is_restrictive(word);
        } else if !matches!(lexicon::class(word), Some(Class::Modal)) {
            break;
        }
        subject_end -= 1;
    }
    if subject_end == 0 {
        return None;
    }

    // The subject is the noun phrase that ends at the copula, not everything
    // before it.
    let subject_start = noun_phrase_start(&words[..subject_end]);
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
    while index < words.len() {
        let word = &words[index].normalized;
        if lexicon::is_negative(word) {
            polarity = polarity.flipped();
        } else if lexicon::is_adverb(word) {
            restricted |= lexicon::is_restrictive(word);
        } else {
            break;
        }
        index += 1;
    }
    // `is not always` reads as a denial of the value, and so does `never`.
    let value_start = index;
    if value_start >= words.len() {
        return None;
    }

    // The complement of a copula runs to the end of the clause, prepositions
    // included: what `is written in Rust` says is `Rust`, and stopping at `in`
    // throws away the only part that carries the meaning. A conjunction still
    // ends it, because what follows is a separate clause.
    let value_end = words[value_start..]
        .iter()
        .position(|token| {
            matches!(
                lexicon::class(&token.normalized),
                Some(Class::Conjunction)
            ) || token.break_before
        })
        .map(|offset| value_start + offset)
        .unwrap_or(words.len());
    if value_end <= value_start {
        return None;
    }

    let subject = Phrase::from_tokens(&words[subject_start..subject_end])?;
    let value = Phrase::from_tokens(&words[value_start..value_end])?;
    if subject.negated {
        polarity = polarity.flipped();
    }

    // A subject with no content word, or a value that is only a function word,
    // says nothing comparable.
    if subject.head.is_empty() || value.head.is_empty() {
        return None;
    }
    if subject.is_unusable(MAX_SUBJECT_WORDS) || value.is_unusable(MAX_VALUE_WORDS) {
        return None;
    }

    let property = lexicon::property_of(&value.head).map(str::to_string);
    Some(Claim {
        kind: ClaimKind::Predication,
        restricted,
        subject,
        property,
        value,
        polarity,
        modality,
    })
}

/// `<noun phrase> [modal] [auxiliary] [not] <relation verb> <object>`
///
/// The verb has to be one the lexicon names, because a relation the analysis
/// cannot compare is not worth extracting.
fn extract_relational(sentence: &Sentence, context: Option<&Phrase>) -> Option<Claim> {
    let words: Vec<&Token> = sentence.words().collect();
    if words.len() < 3 {
        return None;
    }
    let quoted = quoted_spans(&sentence.text);

    // Several words in a sentence can name a relation, because English nouns
    // and verbs share spellings: `the build writes a manifest` offers `build`
    // before `writes`. Without a part-of-speech tagger the only way to tell
    // them apart is to try -- a candidate with no subject in front of it was
    // not the verb -- so each is tried in turn and the first that yields a
    // claim wins.
    let candidates: Vec<(usize, &'static str)> = words
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
        if quoted
            .iter()
            .any(|(start, end)| token.offset >= *start && token.offset < *end)
        {
            return None;
        }
        // The written form first, then the stem, because the stemmer leaves
        // some forms alone on purpose.
        let found = lexicon::relation(&token.normalized)
            .or_else(|| lexicon::relation(&token.lemma))?;
        // `has emitted a skill` is a claim about emitting. An auxiliary names
        // a relation of its own -- `has a skill` -- but loses the verb slot to
        // any content verb after it.
        if lexicon::is_auxiliary(&token.lemma)
            && words[index + 1..].iter().any(|later| {
                lexicon::relation(&later.normalized)
                    .or_else(|| lexicon::relation(&later.lemma))
                    .is_some_and(|_| !lexicon::is_auxiliary(&later.lemma))
            })
        {
            return None;
        }
            Some((index, found))
        })
        .collect();

    candidates
        .into_iter()
        .find_map(|(verb_index, relation)| relational_at(&words, verb_index, relation, context))
}

/// Reads a relational claim around a verb that is already chosen.
fn relational_at(
    words: &[&Token],
    verb_index: usize,
    relation: &'static str,
    context: Option<&Phrase>,
) -> Option<Claim> {

    // Walk back over the modal, auxiliary and negation that may sit between the
    // subject and its verb: `must not contain`, `does not include`.
    let mut subject_end = verb_index;
    let mut polarity = Polarity::Affirmative;
    let mut modality = Modality::Asserted;
    let mut restricted = false;
    while subject_end > 0 {
        let word = &words[subject_end - 1].normalized;
        if lexicon::is_negative(word) {
            polarity = polarity.flipped();
        } else if let Some(found) = lexicon::modality(word) {
            modality = found;
        } else if lexicon::is_adverb(word) {
            // `can only contain` restricts the relation to one answer.
            restricted |= lexicon::is_restrictive(word);
        } else if matches!(lexicon::class(word), Some(Class::Modal)) || lexicon::is_auxiliary(word)
        {
            // A modal or auxiliary with no modality of its own.
        } else {
            break;
        }
        subject_end -= 1;
    }
    // Nothing before the verb is the imperative mood, which is how most of a
    // specification is written: `Emit prompt as the body`, `Do not assume the
    // nested-loading behavior`. The subject is elided rather than absent -- it
    // is the anchor the sentence was written in -- so the surrounding
    // structure supplies it, which is the same thing structure does everywhere
    // else in this analysis.
    let imperative = subject_end == 0;
    let subject = if imperative {
        modality = Modality::Required;
        context?.clone()
    } else {
        let subject_start = noun_phrase_start(&words[..subject_end]);
        if subject_start >= subject_end {
            return None;
        }
        if words[subject_start..subject_end]
            .iter()
            .all(|token| matches!(lexicon::class(&token.normalized), Some(Class::Pronoun)))
        {
            return None;
        }
        Phrase::from_tokens(&words[subject_start..subject_end])?
    };

    // A negative before the object denies it: `has no label`.
    let mut value_start = verb_index + 1;
    while value_start < words.len() {
        let word = &words[value_start].normalized;
        if lexicon::is_negative(word) {
            polarity = polarity.flipped();
        } else if lexicon::is_adverb(word) {
            restricted |= lexicon::is_restrictive(word);
        } else {
            break;
        }
        value_start += 1;
    }
    if value_start >= words.len() {
        return None;
    }

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

    let value = Phrase::from_tokens(&words[value_start..value_end])?;
    if subject.negated {
        polarity = polarity.flipped();
    }
    if subject.head.is_empty() || value.head.is_empty() {
        return None;
    }
    if subject.is_unusable(MAX_SUBJECT_WORDS) || value.is_unusable(MAX_VALUE_WORDS) {
        return None;
    }

    Some(Claim {
        kind: ClaimKind::Relation,
        restricted,
        subject,
        // The relation is the property: two statements only compare when they
        // say something about the same relation.
        property: Some(relation.to_string()),
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

/// Where the noun phrase ending at `words` begins.
///
/// It stops at a function word, and at punctuation: a comma ends a phrase just
/// as surely as a preposition does, and without that a subject can swallow the
/// clause in front of it.
fn noun_phrase_start(words: &[&Token]) -> usize {
    for index in (0..words.len()).rev() {
        if words[index].break_before {
            let word = &words[index].normalized;
            // A break can land on a function word -- `, and contradiction` --
            // and a conjunction begins no noun phrase. A negative determiner
            // is the exception, as everywhere else: it belongs to the phrase.
            if is_phrase_boundary(word) && !lexicon::is_negative(word) {
                return (index + 1).min(words.len());
            }
            return index;
        }
        if is_phrase_boundary(&words[index].normalized) {
            // A negative determiner bounds the phrase but belongs to it: drop
            // `no` from `no adapter` and the claim reads as an affirmative one
            // about adapters. `Phrase` strips it from the head either way.
            return if lexicon::is_negative(&words[index].normalized) {
                index
            } else {
                index + 1
            };
        }
    }
    0
}

/// The most content words a subject is allowed to carry.
///
/// A subject longer than this is not a noun phrase, it is a mis-read clause,
/// and a claim built on one says nothing true about the source.
const MAX_SUBJECT_WORDS: usize = 4;

/// The most content words a value is allowed to carry.
///
/// A complement gets more room than a subject, because it legitimately carries
/// a prepositional phrase: `written in Rust`, `a combination of static
/// analysis`. Holding it to a subject's length would throw those away.
const MAX_VALUE_WORDS: usize = 6;

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
            | Some(Class::Adverb)
            | Some(Class::Negation)
    ) || lexicon::is_adverb(word)
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

    if opposite_polarity {
        // `is blue` and `is not red` are compatible: both can hold.
        return 0.0;
    }

    if !lexicon::excludes(&left.value.head, &right.value.head) {
        // The lexicon has nothing to say about these two values, and the
        // specification is explicit that uncertainty should reduce confidence
        // rather than remove the observation: a weak relationship is reported
        // as information, not silence.
        //
        // What separates a real competition from two unrelated remarks is the
        // frame around the value. `is written in Rust` and `is written in
        // Python` share a predicate and differ only in its complement, which is
        // the shape of two answers to one question. `is fast` and `is careful`
        // share nothing, and can both hold.
        let shared_frame = !left.value.modifiers.is_empty()
            && left.value.modifiers == right.value.modifiers;
        let predication = left.kind == ClaimKind::Predication
            && right.kind == ClaimKind::Predication;
        // `can only contain strings` admits one answer, whatever the verb.
        let restricted = left.restricted && right.restricted;

        return if restricted || (shared_frame && predication) {
            // Two answers to one question. A subject has one answer to what it
            // *is*, so this is as incompatible as an exclusive pair; what keeps
            // a distant or loosely related pair from becoming an error is the
            // structural evidence, which is weighed separately.
            0.9
        } else {
            // Either the values share no frame and can both hold, or the claim
            // is a relation, where one subject takes many objects. Below the
            // default reporting threshold; `--min-severity information`
            // surfaces it for anyone who wants to look.
            0.3
        };
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
        extract(sentences(text).first()?, None)
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
    fn unrelated_values_stay_quiet() {
        use crate::DEFAULT_MINIMUM_CONTRADICTION as THRESHOLD;

        let blue = claim("The button is blue").unwrap();
        let large = claim("The button is large").unwrap();
        assert_eq!(
            contradiction(&blue, &large),
            0.0,
            "two known but different properties are not a conflict at all"
        );

        // The lexicon has nothing to say about these, and they share no frame,
        // so the observation is kept but scores below the reporting threshold.
        let fast = claim("The parser is fast").unwrap();
        let careful = claim("The parser is careful").unwrap();
        let weak = contradiction(&fast, &careful);
        assert!(weak > 0.0 && weak < THRESHOLD, "got {weak}");
    }

    #[test]
    fn a_copular_complement_keeps_its_prepositional_phrase() {
        // `is written in Rust` says `Rust`; stopping at `in` throws away the
        // only part that carries the meaning.
        let claim = claim("The CLI is written in Rust").expect("claim");
        assert_eq!(claim.subject.head, "cli");
        assert_eq!(claim.value.head, "rust");
        // `written` lemmatizes to its verb, which is what makes it compare
        // with `writes` elsewhere.
        assert_eq!(claim.value.modifiers, vec!["write"]);
    }

    #[test]
    fn two_answers_to_one_question_contradict() {
        // A shared predicate with different complements is the shape of two
        // answers to one question, even when the lexicon knows neither value.
        // A subject has one answer to what it *is*, so this is as incompatible
        // as a known exclusive pair.
        let rust = claim("The CLI is written in Rust").unwrap();
        let python = claim("The CLI is written in Python").unwrap();
        assert!(contradiction(&rust, &python) >= 0.85);
        assert_eq!(subject_identity(&rust, &python), 1.0);
    }

    #[test]
    fn a_relation_takes_many_objects_even_in_one_frame() {
        use crate::DEFAULT_MINIMUM_CONTRADICTION as THRESHOLD;

        // The same shared frame means something different for a relation: a
        // package holds many things at once, so this is not a conflict.
        let skill = claim("The package contains a piton skill").unwrap();
        let command = claim("The package contains a piton command").unwrap();
        assert_eq!(skill.kind, ClaimKind::Relation);
        assert!(contradiction(&skill, &command) < THRESHOLD);
    }

    #[test]
    fn the_same_answer_twice_is_agreement() {
        let one = claim("The CLI is written in Rust").unwrap();
        let two = claim("The CLI is written in Rust").unwrap();
        assert_eq!(contradiction(&one, &two), 0.0);
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
    fn a_mis_read_clause_produces_no_claim() {
        // A subject longer than a noun phrase, or one carrying a pronoun, means
        // the sentence was not understood.
        assert!(
            claim("When you're reporting problems, make sure you always include the file").is_none()
        );
        assert!(claim("The quick brown lazy spotted fox is red").is_none());
    }

    #[test]
    fn punctuation_stops_a_subject_running_backwards() {
        let extracted = claim("Given the configuration, the save button is blue").expect("claim");
        assert_eq!(extracted.subject.head, "button");
        assert_eq!(extracted.subject.modifiers, vec!["save"]);
    }

    #[test]
    fn a_relation_is_a_claim_of_its_own() {
        let claim = claim("The adapter emits a skill").expect("claim");
        assert_eq!(claim.subject.head, "adapter");
        assert_eq!(claim.property.as_deref(), Some("produce"));
        assert_eq!(claim.value.head, "skill");
        assert_eq!(claim.polarity, Polarity::Affirmative);
    }

    #[test]
    fn a_denied_relation_contradicts_the_asserted_one() {
        let emits = claim("The adapter emits a skill").unwrap();
        let denies = claim("The adapter does not emit a skill").unwrap();
        assert_eq!(denies.polarity, Polarity::Negative);
        assert_eq!(contradiction(&emits, &denies), 1.0);
    }

    #[test]
    fn no_before_an_object_denies_it() {
        let has = claim("The button has a label").unwrap();
        let lacks = claim("The button has no label").unwrap();
        assert_eq!(lacks.polarity, Polarity::Negative);
        assert_eq!(contradiction(&has, &lacks), 1.0);
    }

    #[test]
    fn different_relations_never_contradict() {
        // Producing one thing and consuming another is not a conflict.
        let produces = claim("The adapter emits a skill").unwrap();
        let uses = claim("The adapter uses a command").unwrap();
        assert_eq!(contradiction(&produces, &uses), 0.0);
    }

    #[test]
    fn different_objects_of_one_relation_stay_quiet() {
        use crate::DEFAULT_MINIMUM_CONTRADICTION as THRESHOLD;

        // Saying a thing contains two things is ordinary prose, so the
        // observation stays below the reporting threshold.
        let one = claim("The package contains a skill").unwrap();
        let two = claim("The package contains a command").unwrap();
        assert!(contradiction(&one, &two) < THRESHOLD);
    }

    #[test]
    fn modality_carries_through_a_relation() {
        assert_eq!(
            claim("The adapter must emit a skill").unwrap().modality,
            Modality::Required
        );
        assert_eq!(
            claim("The adapter may emit a skill").unwrap().modality,
            Modality::Permitted
        );
    }

    #[test]
    fn an_unknown_verb_produces_no_claim() {
        // The lexicon decides what is comparable; a verb outside it says
        // nothing the analysis can check.
        assert!(claim("The adapter frobnicates a skill").is_none());
        assert!(claim("The compiler bamboozles the tree").is_none());
    }

    #[test]
    fn a_copular_reading_wins_when_both_are_possible() {
        // The sentence holds a copula and a relation verb; the copula comes
        // first, so it is read as saying what the output *is*.
        let claim = claim("The output is a document that contains text").expect("claim");
        assert_eq!(claim.subject.head, "output");
        assert_eq!(claim.property, None, "read as an identity, not a relation");
    }

    #[test]
    fn a_permission_softens_a_conflict_with_a_requirement() {
        let must = claim("The button must be blue").unwrap();
        let may = claim("The button may be red").unwrap();
        let strict = claim("The button must be red").unwrap();
        assert!(contradiction(&must, &may) < contradiction(&must, &strict));
    }
}
