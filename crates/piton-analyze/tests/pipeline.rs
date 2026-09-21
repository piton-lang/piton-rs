//! End-to-end analysis over small fixture projects.
//!
//! These check the whole pipeline, and in particular that it stays quiet: an
//! analysis that reports contradictions nobody made is worse than one that
//! reports none.

use std::path::PathBuf;

use piton_analyze::{analyze, Scope, Settings};
use piton_compile::{Compilation, Project};
use piton_core::Severity;

struct Sandbox {
    dir: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!(
            "piton-analyze-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Sandbox { dir }
    }

    fn file(&self, relative: &str, contents: &str) -> &Sandbox {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, contents).expect("write");
        self
    }

    fn compile(&self, entry: &str) -> Compilation {
        let path = self.dir.join(entry);
        let mut project = Project::for_file(&path);
        project.source_root = self.dir.clone();
        project.root = self.dir.clone();
        let compilation = Compilation::build(project);
        let errors: Vec<String> = compilation
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect();
        assert!(errors.is_empty(), "{errors:#?}");
        compilation
    }
}

fn findings(source: &str, name: &str) -> Vec<(Severity, String, String)> {
    let sandbox = Sandbox::new(name);
    sandbox.file("main.pi", source);
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    report
        .findings
        .iter()
        .map(|finding| {
            (
                finding.severity,
                finding.left.sentence.text.clone(),
                finding.right.sentence.text.clone(),
            )
        })
        .collect()
}

#[test]
fn a_contradiction_inside_one_property_is_an_error() {
    let found = findings(
        "export anchor Button:\n    style:\n        The save button is blue.\n\n        The save button is red.\n",
        "same-property",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn a_contradiction_across_two_anchors_in_one_file_is_reported() {
    let found = findings(
        "export anchor Design:\n    rule: The save button is blue.\n\nexport anchor Theme:\n    rule: The save button is red.\n",
        "same-file",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    // Two unrelated anchors in one file is weaker evidence than one anchor.
    assert_eq!(found[0].0, Severity::Warning);
}

#[test]
fn inheritance_strengthens_a_contradiction() {
    let found = findings(
        "anchor Base:\n    rule: The save button is blue.\n\nexport anchor Derived extends Base:\n    note: The save button is red.\n",
        "inheritance",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        found[0].0,
        Severity::Error,
        "an anchor and its base share a conceptual scope"
    );
}

#[test]
fn different_subjects_are_not_a_contradiction() {
    let found = findings(
        "export anchor Design:\n    save: The save button is blue.\n    cancel: The cancel button is red.\n",
        "different-subjects",
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn different_properties_are_not_a_contradiction() {
    let found = findings(
        "export anchor Design:\n    color: The save button is blue.\n    size: The save button is large.\n",
        "different-properties",
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn values_the_lexicon_does_not_relate_are_not_a_contradiction() {
    let found = findings(
        "export anchor Design:\n    a: The parser is fast.\n    b: The parser is careful.\n",
        "unknown-values",
    );
    assert!(
        found.is_empty(),
        "uncertainty must not become an invented conflict: {found:#?}"
    );
}

#[test]
fn agreement_is_not_a_contradiction() {
    let found = findings(
        "export anchor Design:\n    a: The save button is blue.\n    b: The save button is blue.\n    c: The save button is colour blue.\n",
        "agreement",
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn quoted_statements_are_not_asserted() {
    let found = findings(
        "export anchor Doc:\n    example:\n        In one place we say \"The save button is blue\" and elsewhere we say \"The save button is red\".\n\n        The save button is blue.\n",
        "quoted",
    );
    assert!(
        found.is_empty(),
        "a document describing a contradiction has not made one: {found:#?}"
    );
}

#[test]
fn fenced_examples_are_not_prose() {
    let found = findings(
        "export anchor Doc:\n    example:\n        The save button is blue.\n\n        ```\n        The save button is red.\n        ```\n",
        "fenced",
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_denial_contradicts_its_assertion() {
    let found = findings(
        "export anchor Design:\n    a: The save button is enabled.\n    b: The save button is not enabled.\n",
        "denial",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn distant_files_produce_at_most_a_weak_finding() {
    let sandbox = Sandbox::new("distant");
    sandbox
        .file(
            "far/Other.pi",
            "export anchor Other:\n    rule: The save button is red.\n",
        )
        .file(
            "main.pi",
            "from ./far/Other import Other\n\nexport anchor Design:\n    rule: The save button is blue.\n    uses: {Other}\n",
        );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    assert_eq!(report.findings.len(), 1, "{:#?}", report.findings);
    assert_ne!(
        report.findings[0].severity,
        Severity::Error,
        "structural distance must degrade the finding"
    );
}

#[test]
fn analysis_is_deterministic() {
    let sandbox = Sandbox::new("deterministic");
    sandbox.file(
        "main.pi",
        "export anchor Design:\n    a: The save button is blue.\n    b: The save button is red.\n    c: The cancel button is green.\n    d: The label is hidden.\n    e: The label is visible.\n",
    );
    let compilation = sandbox.compile("main.pi");
    let first = analyze(&compilation, &Scope::Project, &Settings::default());
    let second = analyze(&compilation, &Scope::Project, &Settings::default());

    let render = |report: &piton_analyze::Report| {
        report
            .findings
            .iter()
            .map(|finding| {
                format!(
                    "{:?} {} | {} | {:.3} {:.3} {:.3}",
                    finding.severity,
                    finding.left.sentence.text,
                    finding.right.sentence.text,
                    finding.confidence.semantic,
                    finding.confidence.structural,
                    finding.confidence.contradiction
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(render(&first), render(&second));
    assert_eq!(first.findings.len(), 2, "{:#?}", render(&first));
}

#[test]
fn every_finding_explains_itself() {
    let sandbox = Sandbox::new("explain");
    sandbox.file(
        "main.pi",
        "export anchor Design:\n    a: The save button is blue.\n    b: The save button is red.\n",
    );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    let finding = report.findings.first().expect("a finding");
    assert!(!finding.explanation.is_empty());
    assert!(
        finding
            .explanation
            .iter()
            .any(|line| line.contains("save button")),
        "the explanation names the subject: {:#?}",
        finding.explanation
    );
    assert!(
        finding.explanation.iter().any(|line| line.starts_with("structure:")),
        "the explanation names the structural relationship"
    );
    // Every finding points at a real declaration.
    assert!(finding.left.span().start < finding.right.span().start);
}

#[test]
fn scoping_to_one_anchor_ignores_the_rest() {
    let sandbox = Sandbox::new("scoped");
    sandbox.file(
        "main.pi",
        "export anchor First:\n    a: The save button is blue.\n\nexport anchor Second:\n    b: The save button is red.\n",
    );
    let compilation = sandbox.compile("main.pi");
    let anchor = compilation.find_anchor("First").expect("First");
    let report = analyze(&compilation, &Scope::Anchor(anchor), &Settings::default());
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
}
