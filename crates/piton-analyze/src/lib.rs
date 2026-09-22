//! Deterministic semantic analysis of Piton prose.
//!
//! Piton is largely unstructured text organized in a structured format, so
//! checking that a specbase says one thing takes both halves: linguistic
//! analysis of what the prose says, and structural analysis of whether two
//! statements are talking about the same thing.
//!
//! # What it does
//!
//! 1. Collect every prose statement, with the anchor and property it came from.
//! 2. Reduce each to a claim — subject, property, value, polarity, modality.
//! 3. Compare claims pairwise, nearest structural neighbours first.
//! 4. Combine semantic, structural, and contradiction evidence into a severity.
//!
//! # What it does not do
//!
//! Extraction is rule-based over a curated lexicon. There is no statistical
//! part-of-speech tagger and no dependency parser, so claims come only from
//! copular statements, and the lexicon's relations are the only ones the
//! analysis will act on. A sentence it cannot read produces no claim, and a
//! value pair it knows nothing about produces no contradiction. Both are
//! deliberate: the `conservative` principle says uncertainty must reduce
//! confidence rather than invent meaning.

pub mod claim;
pub mod lexicon;
pub mod nlg;
pub mod structure;
pub mod text;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use piton_compile::{Compilation, ModuleId};
use piton_core::{AnchorId, MixedItem, Severity, Span, Value};

use claim::{Claim, Phrase};
use structure::{Relation, Site, Structure};
use text::Sentence;

/// What part of the specbase to analyze.
#[derive(Debug, Clone, Default)]
pub enum Scope {
    /// Everything reachable in the project.
    #[default]
    Project,
    /// One module and the statements inside it.
    Module(ModuleId),
    /// One anchor.
    Anchor(AnchorId),
}

/// The contradiction strength below which a finding is not reported.
///
/// A pair the lexicon cannot relate scores below this, so it is observed but
/// stays quiet unless someone lowers the bar with `--min-severity`.
pub const DEFAULT_MINIMUM_CONTRADICTION: f64 = 0.4;

/// Tuning that must be recorded, because the same source and the same settings
/// have to produce the same result.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Pairs below this structural confidence are never compared.
    pub minimum_structural: f64,
    /// Pairs below this subject identity are never compared.
    pub minimum_semantic: f64,
    /// Findings below this contradiction strength are dropped.
    pub minimum_contradiction: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            // Two statements in unrelated corners of a project are not evidence
            // of anything on their own.
            minimum_structural: 0.15,
            minimum_semantic: 0.5,
            minimum_contradiction: DEFAULT_MINIMUM_CONTRADICTION,
        }
    }
}

/// One statement, kept with everything needed to explain a finding about it.
#[derive(Debug, Clone)]
pub struct Statement {
    pub site: Site,
    pub sentence: Sentence,
    pub claim: Claim,
    /// The file the statement was written in.
    pub path: PathBuf,
    /// The anchor's name, for display.
    pub anchor_name: String,
}

impl Statement {
    /// Where to point a reader.
    ///
    /// This is the property's declaration, not the sentence. A resolved value
    /// is assembled from inheritance, interpolation, and line joining, so an
    /// offset into it does not correspond to any position in the file; the
    /// declaration does, and it is where the text is edited.
    pub fn span(&self) -> Span {
        self.site.span
    }
}

/// The three kinds of evidence, kept separate rather than collapsed into one
/// opaque score.
#[derive(Debug, Clone, Copy)]
pub struct Confidence {
    /// How strongly linguistic analysis says the two statements are about the
    /// same concept.
    pub semantic: f64,
    /// How strongly Piton structure says they share a conceptual scope.
    pub structural: f64,
    /// How strongly the extracted claims are incompatible.
    pub contradiction: f64,
}

impl Confidence {
    /// Severity follows from the combination: an error needs all three to be
    /// strong, and any weak leg degrades the finding rather than suppressing it.
    pub fn severity(&self) -> Severity {
        if self.semantic >= 0.85 && self.structural >= 0.8 && self.contradiction >= 0.85 {
            Severity::Error
        } else if self.semantic >= 0.6 && self.structural >= 0.5 && self.contradiction >= 0.6 {
            Severity::Warning
        } else {
            Severity::Info
        }
    }
}

/// One reported relationship between two statements.
#[derive(Debug, Clone)]
pub struct Finding {
    pub left: Statement,
    pub right: Statement,
    pub relation: Relation,
    pub confidence: Confidence,
    pub severity: Severity,
    /// Why the analysis considered the statements related and incompatible.
    pub explanation: Vec<String>,
}

/// The result of an analysis run.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub findings: Vec<Finding>,
    /// Every statement that produced a comparable claim, in source order.
    ///
    /// Reported by `piton analyze --claims`: a run that finds nothing should
    /// still be able to show what it read.
    pub statements: Vec<Statement>,
    /// How many statements produced a comparable claim.
    pub claims: usize,
    /// How many sentences were examined.
    pub sentences: usize,
    /// How many pairs were compared after the locality cutoffs.
    pub compared: usize,
    /// What the analysis read and what it did not.
    pub coverage: Coverage,
}

/// How much of the specbase the analysis could read.
///
/// The analysis only reads two sentence shapes, and the rest produces nothing.
/// That is deliberate, but it means a clean run is ambiguous on its own: it can
/// mean the prose agrees, or it can mean almost none of it was read. This is
/// the number that tells those apart.
#[derive(Debug, Default, Clone)]
pub struct Coverage {
    /// Files holding at least one anchor in scope.
    pub files: usize,
    /// Anchors in scope.
    pub anchors: usize,
    /// Properties on those anchors.
    pub properties: usize,
    /// Values that are prose, which is all the analysis can read. Counts
    /// nested values too, so this is not a subset of `properties`.
    pub prose_values: usize,
    /// Sentences found in that prose.
    pub sentences: usize,
    /// Sentences that produced a comparable claim.
    pub claims: usize,
    /// Sentences the document was quoting rather than asserting.
    pub mentioned: usize,
    /// Sentences that produced no claim, counted by reason.
    pub gaps: BTreeMap<String, usize>,
    /// Values that are not prose at all: a list item, an enum value, an
    /// identifier. They are excluded from `sentences`, because counting
    /// `claude-code` as an unread sentence says nothing about how much of the
    /// prose was understood.
    pub fragments: usize,
    /// Words that sit where a verb would but that the lexicon does not name,
    /// most frequent first. These are what would widen coverage.
    pub unknown_verbs: Vec<(String, usize)>,
}

/// Running counts while statements are collected.
#[derive(Default)]
struct Tally {
    prose_values: usize,
    sentences: usize,
    mentioned: usize,
    fragments: usize,
    gaps: BTreeMap<String, usize>,
    unknown_verbs: BTreeMap<String, usize>,
}

impl Coverage {
    /// The share of sentences that produced a claim, from 0 to 1.
    pub fn understood(&self) -> f64 {
        if self.sentences == 0 {
            return 0.0;
        }
        self.claims as f64 / self.sentences as f64
    }
}

impl Report {
    pub fn has_errors(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.severity == Severity::Error)
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == severity)
            .count()
    }
}

/// Runs the analysis.
pub fn analyze(compilation: &Compilation, scope: &Scope, settings: &Settings) -> Report {
    let (statements, coverage) = collect_statements(compilation, scope);
    let structure = Structure::build(compilation);

    let mut report = Report {
        claims: statements.len(),
        sentences: coverage.sentences,
        statements: statements.clone(),
        coverage,
        ..Report::default()
    };

    // Compare nearest neighbours first. The ordering does not change the result
    // -- every pair below the cutoffs is skipped either way -- but it means the
    // work is done in the order the specification describes.
    let mut pairs: Vec<(usize, usize, Relation)> = Vec::new();
    for left in 0..statements.len() {
        for right in (left + 1)..statements.len() {
            let relation = structure.relate(&statements[left].site, &statements[right].site);
            if relation.confidence() < settings.minimum_structural {
                continue;
            }
            pairs.push((left, right, relation));
        }
    }
    pairs.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(a.0.cmp(&b.0))
            .then(a.1.cmp(&b.1))
    });

    for (left_index, right_index, relation) in pairs {
        let left = &statements[left_index];
        let right = &statements[right_index];

        let semantic = claim::subject_identity(&left.claim, &right.claim);
        if semantic < settings.minimum_semantic {
            continue;
        }
        report.compared += 1;

        let contradiction = claim::contradiction(&left.claim, &right.claim);
        if contradiction < settings.minimum_contradiction {
            continue;
        }

        let confidence = Confidence {
            semantic,
            structural: relation.confidence(),
            contradiction,
        };
        report.findings.push(Finding {
            explanation: explain(left, right, relation, &confidence),
            left: left.clone(),
            right: right.clone(),
            relation,
            severity: confidence.severity(),
            confidence,
        });
    }

    report.statements.sort_by(|a, b| {
        a.path
            .cmp(&b.path)
            .then(a.span().start.cmp(&b.span().start))
            .then(a.sentence.offset.cmp(&b.sentence.offset))
    });

    // Output order must not depend on traversal order.
    report.findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then(a.left.path.cmp(&b.left.path))
            .then(a.left.span().start.cmp(&b.left.span().start))
            .then(a.right.path.cmp(&b.right.path))
            .then(a.right.span().start.cmp(&b.right.span().start))
    });
    report
}

/// Builds the explanation a reader needs to judge the finding for themselves.
fn explain(
    left: &Statement,
    right: &Statement,
    relation: Relation,
    confidence: &Confidence,
) -> Vec<String> {
    let mut out = Vec::new();
    out.push(format!(
        "both statements describe `{}`",
        left.claim.subject.head_with_modifiers()
    ));
    if left.claim.subject.head_with_modifiers() != right.claim.subject.head_with_modifiers() {
        out.push(format!(
            "the second names it `{}`",
            right.claim.subject.head_with_modifiers()
        ));
    }
    if let Some(property) = &left.claim.property {
        out.push(format!("both assign the `{property}` property"));
    }
    out.push(format!("structure: {}", relation.as_str()));
    out.push(format!(
        "confidence: semantic {:.2}, structural {:.2}, contradiction {:.2}",
        confidence.semantic, confidence.structural, confidence.contradiction
    ));
    out
}

/// Walks the resolved values and pulls out every sentence that makes a claim.
fn collect_statements(compilation: &Compilation, scope: &Scope) -> (Vec<Statement>, Coverage) {
    let anchors: Vec<AnchorId> = match scope {
        Scope::Anchor(anchor) => vec![*anchor],
        Scope::Module(module) => compilation
            .store()
            .anchors
            .iter()
            .filter(|def| def.module == *module)
            .map(|def| def.id)
            .collect(),
        Scope::Project => {
            let reachable: BTreeSet<AnchorId> = piton_compile::reach::from_entry(compilation)
                .reached
                .iter()
                .map(|entry| entry.anchor)
                .collect();
            reachable.into_iter().collect()
        }
    };

    let mut out = Vec::new();
    let mut tally = Tally::default();
    let mut coverage = Coverage::default();
    let mut files: BTreeSet<PathBuf> = BTreeSet::new();
    // An inherited property resolves onto every anchor that inherits it, so the
    // same sentence would otherwise be collected once per descendant and
    // reported that many times. Keying on the declaration that supplies the
    // value collapses those copies, while still keeping two resolutions that
    // genuinely differ -- which is what `self` interpolation produces.
    let mut seen: BTreeSet<(AnchorId, String, String)> = BTreeSet::new();

    for anchor in anchors {
        let def = compilation.store().anchor(anchor);
        let path = compilation.anchor_module_path(anchor);
        if path.to_string_lossy().starts_with('@') {
            // A bundled package is the compiler's own vocabulary.
            continue;
        }
        // An imperative's subject is the anchor it was written in.
        let context = Phrase::from_name(&def.name);
        coverage.anchors += 1;
        coverage.properties += def.properties.len();
        files.insert(path.clone());
        for (name, value) in &def.properties {
            // Point at the declaration that supplies the value, which for an
            // inherited property is in the base that declares it.
            let (span, module, path) = match def.slots.get(name) {
                Some(slot) => {
                    let owner = compilation.store().anchor(slot.owner);
                    (
                        slot.span,
                        owner.module,
                        compilation.anchor_module_path(slot.owner),
                    )
                }
                None => (def.span, def.module, path.clone()),
            };
            let owner = def.slots.get(name).map(|slot| slot.owner).unwrap_or(anchor);
            let mut collected = Vec::new();
            let mut collected_gaps: Vec<(String, claim::Gap)> = Vec::new();
            visit(
                value,
                name,
                &Site {
                    anchor,
                    module,
                    property: name.clone(),
                    span,
                },
                &path,
                &def.name,
                context.as_ref(),
                &mut collected,
                &mut collected_gaps,
                &mut tally,
            );
            // A statement the document quoted is one it described, not one it
            // made. The quotes are gone from the value by now, so the
            // compilation is asked instead.
            let before = collected.len();
            collected.retain(|statement| {
                !compilation.is_mentioned(anchor, &statement.sentence.text)
            });
            tally.mentioned += before - collected.len();
            for (text, gap) in collected_gaps {
                if !seen.insert((owner, name.clone(), text)) {
                    continue;
                }
                if gap == claim::Gap::NotASentence {
                    tally.fragments += 1;
                    continue;
                }
                tally.sentences += 1;
                if let claim::Gap::UnknownVerb {
                    candidate: Some(word),
                } = &gap
                {
                    *tally.unknown_verbs.entry(word.clone()).or_default() += 1;
                }
                *tally.gaps.entry(gap.as_str().to_string()).or_default() += 1;
            }
            for statement in collected {
                let key = (
                    owner,
                    statement.site.property.clone(),
                    statement.sentence.text.clone(),
                );
                if seen.insert(key) {
                    tally.sentences += 1;
                    out.push(statement);
                }
            }
        }
    }

    // A mentioned statement was read successfully and then set aside, so it
    // counts against neither the claims nor the gaps.
    coverage.files = files.len();
    coverage.prose_values = tally.prose_values;
    coverage.sentences = tally.sentences;
    coverage.claims = out.len();
    coverage.mentioned = tally.mentioned;
    coverage.gaps = tally.gaps;
    coverage.fragments = tally.fragments;
    let mut verbs: Vec<(String, usize)> = tally.unknown_verbs.into_iter().collect();
    // Frequency first, then alphabetically, so the list is deterministic.
    verbs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    coverage.unknown_verbs = verbs;
    (out, coverage)
}

fn visit(
    value: &Value,
    property_path: &str,
    site: &Site,
    path: &PathBuf,
    anchor_name: &str,
    context: Option<&Phrase>,
    out: &mut Vec<Statement>,
    gaps: &mut Vec<(String, claim::Gap)>,
    tally: &mut Tally,
) {
    match value {
        Value::Str(text) => {
            tally.prose_values += 1;
            // References inside prose are links, not words; rendering them as
            // their anchor name keeps the sentence readable.
            let rendered = text.to_string();
            let cleaned = strip_reference_markers(&rendered);
            for sentence in text::sentences(&cleaned) {
                let claim = match claim::explain(&sentence, context) {
                    Ok(claim) => claim,
                    Err(gap) => {
                        gaps.push((sentence.text.clone(), gap));
                        continue;
                    }
                };
                {
                    out.push(Statement {
                        site: Site {
                            property: property_path.to_string(),
                            ..site.clone()
                        },
                        sentence,
                        claim,
                        path: path.clone(),
                        anchor_name: anchor_name.to_string(),
                    });
                }
            }
        }
        Value::List(items) => {
            for item in items {
                visit(item, property_path, site, path, anchor_name, context, out, gaps, tally);
            }
        }
        Value::Dict(map) => {
            for (name, item) in map {
                let nested = format!("{property_path}.{name}");
                visit(item, &nested, site, path, anchor_name, context, out, gaps, tally);
            }
        }
        Value::Mixed(mixed) => {
            for item in &mixed.items {
                match item {
                    MixedItem::Text(text) => {
                        visit(
                            &Value::Str(text.clone()),
                            property_path,
                            site,
                            path,
                            anchor_name,
                            context,
                            out,
                            gaps,
                            tally,
                        );
                    }
                    MixedItem::List(items) => {
                        for entry in items {
                            visit(entry, property_path, site, path, anchor_name, context, out, gaps, tally);
                        }
                    }
                    MixedItem::Entry(name, value) => {
                        let nested = format!("{property_path}.{name}");
                        visit(value, &nested, site, path, anchor_name, context, out, gaps, tally);
                    }
                }
            }
        }
        // An anchor embedded by value is analyzed where it is declared, so it is
        // not analyzed again at every use site.
        _ => {}
    }
}

/// Replaces the placeholder a reference leaves in rendered text.
fn strip_reference_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("@{") {
        out.push_str(&rest[..start]);
        match rest[start..].find('}') {
            Some(end) => rest = &rest[start + end + 1..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_needs_every_leg_to_be_strong() {
        let strong = Confidence {
            semantic: 1.0,
            structural: 1.0,
            contradiction: 1.0,
        };
        assert_eq!(strong.severity(), Severity::Error);

        let weak_structure = Confidence {
            structural: 0.55,
            ..strong
        };
        assert_eq!(
            weak_structure.severity(),
            Severity::Warning,
            "uncertainty degrades the finding rather than removing it"
        );

        let weak_semantics = Confidence {
            semantic: 0.55,
            structural: 0.4,
            contradiction: 0.5,
        };
        assert_eq!(weak_semantics.severity(), Severity::Info);
    }

    #[test]
    fn reference_markers_leave_readable_prose() {
        assert_eq!(
            strip_reference_markers("Runs @{Analysis} on the project"),
            "Runs  on the project"
        );
        assert_eq!(strip_reference_markers("no markers"), "no markers");
    }
}
