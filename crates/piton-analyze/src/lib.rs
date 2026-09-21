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
pub mod structure;
pub mod text;

use std::collections::BTreeSet;
use std::path::PathBuf;

use piton_compile::{Compilation, ModuleId};
use piton_core::{AnchorId, MixedItem, Severity, Span, Value};

use claim::Claim;
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
            minimum_contradiction: 0.4,
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
    /// How many statements produced a comparable claim.
    pub claims: usize,
    /// How many sentences were examined.
    pub sentences: usize,
    /// How many pairs were compared after the locality cutoffs.
    pub compared: usize,
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
    let statements = collect_statements(compilation, scope);
    let structure = Structure::build(compilation);

    let mut report = Report {
        claims: statements.len(),
        sentences: statements.len(),
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
fn collect_statements(compilation: &Compilation, scope: &Scope) -> Vec<Statement> {
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
                &mut collected,
            );
            for statement in collected {
                let key = (
                    owner,
                    statement.site.property.clone(),
                    statement.sentence.text.clone(),
                );
                if seen.insert(key) {
                    out.push(statement);
                }
            }
        }
    }
    out
}

fn visit(
    value: &Value,
    property_path: &str,
    site: &Site,
    path: &PathBuf,
    anchor_name: &str,
    out: &mut Vec<Statement>,
) {
    match value {
        Value::Str(text) => {
            // References inside prose are links, not words; rendering them as
            // their anchor name keeps the sentence readable.
            let rendered = text.to_string();
            let cleaned = strip_reference_markers(&rendered);
            for sentence in text::sentences(&cleaned) {
                if let Some(claim) = claim::extract(&sentence) {
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
                visit(item, property_path, site, path, anchor_name, out);
            }
        }
        Value::Dict(map) => {
            for (name, item) in map {
                let nested = format!("{property_path}.{name}");
                visit(item, &nested, site, path, anchor_name, out);
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
                            out,
                        );
                    }
                    MixedItem::List(items) => {
                        for entry in items {
                            visit(entry, property_path, site, path, anchor_name, out);
                        }
                    }
                    MixedItem::Entry(name, value) => {
                        let nested = format!("{property_path}.{name}");
                        visit(value, &nested, site, path, anchor_name, out);
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
