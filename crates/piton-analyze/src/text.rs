//! Lexical analysis: tokenization, sentence segmentation, normalization, and
//! lemmatization.
//!
//! Everything here is rule-based and deterministic. The same text always
//! produces the same tokens, because a diagnostic that appears only sometimes
//! is worse than no diagnostic at all.

use piton_core::Span;

/// One token of prose, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The token as written.
    pub text: String,
    /// Lowercased, with surrounding punctuation removed.
    pub normalized: String,
    /// The normalized form reduced to a stem.
    pub lemma: String,
    /// Offset of the token within the text it was taken from.
    pub offset: usize,
}

impl Token {
    pub fn is_word(&self) -> bool {
        !self.normalized.is_empty()
            && self
                .normalized
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '\'')
    }
}

/// A sentence, with its tokens and its position in the original text.
#[derive(Debug, Clone)]
pub struct Sentence {
    pub text: String,
    pub tokens: Vec<Token>,
    /// Offset of the sentence within the text it was taken from.
    pub offset: usize,
}

impl Sentence {
    /// The span of this sentence inside a value whose text began at `base`.
    pub fn span(&self, base: usize) -> Span {
        Span::new(base + self.offset, base + self.offset + self.text.len())
    }

    pub fn words(&self) -> impl Iterator<Item = &Token> {
        self.tokens.iter().filter(|token| token.is_word())
    }
}

/// Splits text into sentences.
///
/// Fenced code blocks are skipped: their contents are verbatim data, not prose,
/// and reading them as English produces nonsense claims. Abbreviations that end
/// in a period do not end a sentence.
pub fn sentences(text: &str) -> Vec<Sentence> {
    let mut out = Vec::new();
    for (segment, base) in prose_segments(text) {
        let chars: Vec<(usize, char)> = segment.char_indices().collect();
        let mut start = 0usize;
        let mut index = 0usize;

        while index < chars.len() {
            let (offset, ch) = chars[index];
            let terminal = matches!(ch, '.' | '!' | '?' | ';');
            let line_break = ch == '\n';

            if terminal && !ends_abbreviation(segment, offset) {
                let end = next_boundary(&chars, index + 1);
                push_sentence(&mut out, segment, start, end, base);
                start = end;
                index = char_index_at(&chars, end).unwrap_or(chars.len());
                continue;
            }
            if line_break {
                // A line break inside a value is a deliberate break, so it ends
                // a statement even without terminal punctuation.
                push_sentence(&mut out, segment, start, offset, base);
                start = offset + 1;
            }
            index += 1;
        }
        push_sentence(&mut out, segment, start, segment.len(), base);
    }
    out
}

fn push_sentence(out: &mut Vec<Sentence>, source: &str, start: usize, end: usize, base: usize) {
    if end <= start {
        return;
    }
    let raw = &source[start..end];
    let leading = raw.len() - raw.trim_start().len();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    out.push(Sentence {
        text: trimmed.to_string(),
        tokens: tokenize(trimmed),
        offset: base + start + leading,
    });
}

fn char_index_at(chars: &[(usize, char)], offset: usize) -> Option<usize> {
    chars.iter().position(|(at, _)| *at >= offset)
}

/// Advances past trailing quotes and closing punctuation so they stay with the
/// sentence they belong to.
fn next_boundary(chars: &[(usize, char)], mut index: usize) -> usize {
    while index < chars.len() && matches!(chars[index].1, '"' | '\'' | ')' | ']' | '”' | '’') {
        index += 1;
    }
    match chars.get(index) {
        Some((offset, _)) => *offset,
        None => chars.last().map(|(o, c)| o + c.len_utf8()).unwrap_or(0),
    }
}

/// Text outside fenced code blocks, with each segment's offset.
fn prose_segments(text: &str) -> Vec<(&str, usize)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    let mut start = 0usize;
    let mut in_fence = false;

    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        let ticks = trimmed.chars().take_while(|c| *c == '`').count();
        if ticks >= 3 {
            if in_fence {
                in_fence = false;
                start = offset + line.len();
            } else {
                if offset > start {
                    out.push((&text[start..offset], start));
                }
                in_fence = true;
            }
        }
        offset += line.len();
    }
    if !in_fence && offset > start {
        out.push((&text[start..offset], start));
    }
    out
}

/// True when the period at `offset` closes a known abbreviation or a single
/// initial rather than a sentence.
fn ends_abbreviation(text: &str, offset: usize) -> bool {
    const ABBREVIATIONS: &[&str] = &[
        "e.g", "i.e", "etc", "vs", "cf", "al", "fig", "no", "approx", "dr", "mr", "mrs", "ms",
        "st", "jr", "sr",
    ];
    let before = &text[..offset];
    let word: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let trimmed = word.trim_end_matches('.').to_lowercase();
    if trimmed.chars().count() == 1 && trimmed.chars().all(|c| c.is_alphabetic()) {
        return true;
    }
    ABBREVIATIONS.contains(&trimmed.as_str())
}

/// Splits a sentence into tokens.
pub fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut start = 0usize;

    let flush = |current: &mut String, start: usize, out: &mut Vec<Token>| {
        if current.is_empty() {
            return;
        }
        let text = std::mem::take(current);
        let normalized = normalize(&text);
        if normalized.is_empty() {
            return;
        }
        out.push(Token {
            lemma: lemma(&normalized),
            normalized,
            text,
            offset: start,
        });
    };

    for (offset, ch) in text.char_indices() {
        if ch.is_alphanumeric() || ch == '\'' || ch == '’' || ch == '_' || ch == '-' {
            if current.is_empty() {
                start = offset;
            }
            current.push(ch);
        } else {
            flush(&mut current, start, &mut out);
        }
    }
    flush(&mut current, start, &mut out);
    out
}

/// Lowercases and strips punctuation that clings to a word.
pub fn normalize(text: &str) -> String {
    let lowered = text.to_lowercase().replace('’', "'");
    let trimmed = lowered.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
    // `don't` keeps its apostrophe; a possessive loses it.
    trimmed.trim_end_matches("'s").to_string()
}

/// Reduces a word to a stem.
///
/// This is a conservative suffix stripper, not a full morphological analyzer:
/// it handles the regular English endings that appear in specification prose and
/// leaves anything it is unsure about alone. Over-stemming would merge words
/// that mean different things, which is the failure this analysis cannot
/// afford.
pub fn lemma(word: &str) -> String {
    if let Some(known) = irregular(word) {
        return known.to_string();
    }
    let length = word.chars().count();
    if length <= 3 {
        return word.to_string();
    }

    for suffix in ["ies", "ied"] {
        if let Some(stem) = word.strip_suffix(suffix) {
            if stem.chars().count() >= 2 {
                return format!("{stem}y");
            }
        }
    }
    for suffix in ["sses", "shes", "ches", "xes", "zes"] {
        if let Some(stem) = word.strip_suffix("es") {
            if word.ends_with(suffix) {
                return stem.to_string();
            }
        }
    }
    // A stem has to look like a word before the suffix comes off. Without the
    // vowel test, `string` loses its `-ing` and becomes `str`, which would then
    // compare equal to every other `str*` word.
    if let Some(stem) = word.strip_suffix("ing") {
        if is_plausible_stem(stem) {
            return undouble(stem);
        }
    }
    if let Some(stem) = word.strip_suffix("ed") {
        if is_plausible_stem(stem) {
            return undouble(stem);
        }
    }
    if let Some(stem) = word.strip_suffix('s') {
        // `-ss` is not a plural, and neither is `is` or `has`.
        if !word.ends_with("ss") && !word.ends_with("us") && stem.chars().count() >= 3 {
            return stem.to_string();
        }
    }
    word.to_string()
}

/// True when a stripped stem is long enough and contains a vowel.
fn is_plausible_stem(stem: &str) -> bool {
    stem.chars().count() >= 3 && stem.chars().any(|c| "aeiouy".contains(c))
}

/// Undoes the consonant doubling English adds before `-ing` and `-ed`.
fn undouble(stem: &str) -> String {
    let chars: Vec<char> = stem.chars().collect();
    if chars.len() >= 3 {
        let last = chars[chars.len() - 1];
        let previous = chars[chars.len() - 2];
        if last == previous && !"aeiou".contains(last) && !"lsz".contains(last) {
            return chars[..chars.len() - 1].iter().collect();
        }
    }
    // A bare consonant cluster usually wants its `e` back: `us` -> `use`.
    if chars.len() >= 2 && !"aeiouy".contains(chars[chars.len() - 1]) {
        return stem.to_string();
    }
    stem.to_string()
}

fn irregular(word: &str) -> Option<&'static str> {
    const IRREGULAR: &[(&str, &str)] = &[
        ("is", "be"),
        ("are", "be"),
        ("am", "be"),
        ("was", "be"),
        ("were", "be"),
        ("been", "be"),
        ("being", "be"),
        ("has", "have"),
        ("had", "have"),
        ("having", "have"),
        ("does", "do"),
        ("did", "do"),
        ("doing", "do"),
        ("children", "child"),
        ("people", "person"),
        ("men", "man"),
        ("women", "woman"),
        ("this", "this"),
        ("these", "these"),
        ("its", "its"),
        ("as", "as"),
        ("was", "be"),
    ];
    IRREGULAR
        .iter()
        .find(|(from, _)| *from == word)
        .map(|(_, to)| *to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentences_split_on_terminal_punctuation() {
        let found = sentences("The button is blue. The label is red! Is it?");
        let texts: Vec<&str> = found.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(
            texts,
            vec!["The button is blue.", "The label is red!", "Is it?"]
        );
    }

    #[test]
    fn abbreviations_do_not_end_a_sentence() {
        let found = sentences("Use JSON, YAML, etc. as the output. Then stop.");
        assert_eq!(found.len(), 2, "{found:#?}");
        assert!(found[0].text.contains("etc."));
    }

    #[test]
    fn fenced_blocks_are_not_prose() {
        let text = "Before the fence.\n\n```\nsubject: Save Button\nvalue: blue\n```\n\nAfter the fence.";
        let found = sentences(text);
        let texts: Vec<&str> = found.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(texts, vec!["Before the fence.", "After the fence."]);
    }

    #[test]
    fn line_breaks_end_a_statement() {
        let found = sentences("The button is blue\nThe label is red");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn offsets_point_back_into_the_source() {
        let text = "First one. Second one.";
        let found = sentences(text);
        assert_eq!(&text[found[1].offset..found[1].offset + found[1].text.len()], "Second one.");
    }

    #[test]
    fn tokens_keep_their_written_form_and_a_lemma() {
        let tokens = tokenize("The Buttons are rendering.");
        let normalized: Vec<&str> = tokens.iter().map(|t| t.normalized.as_str()).collect();
        assert_eq!(normalized, vec!["the", "buttons", "are", "rendering"]);
        let lemmas: Vec<&str> = tokens.iter().map(|t| t.lemma.as_str()).collect();
        assert_eq!(lemmas, vec!["the", "button", "be", "render"]);
    }

    #[test]
    fn stemming_is_conservative() {
        // Regular endings are stripped.
        assert_eq!(lemma("buttons"), "button");
        assert_eq!(lemma("copies"), "copy");
        assert_eq!(lemma("matched"), "match");
        // Words that merely end in those letters are left alone.
        assert_eq!(lemma("class"), "class");
        assert_eq!(lemma("status"), "status");
        assert_eq!(lemma("red"), "red");
        // `string` must keep its ending: the stem `str` has no vowel.
        assert_eq!(lemma("string"), "string");
        assert_eq!(lemma("bring"), "bring");
        assert_eq!(lemma("thing"), "thing");
    }

    #[test]
    fn possessives_normalize_to_the_noun() {
        assert_eq!(normalize("Button's"), "button");
        assert_eq!(normalize("\u{201c}quoted\u{201d}"), "quoted");
    }
}
