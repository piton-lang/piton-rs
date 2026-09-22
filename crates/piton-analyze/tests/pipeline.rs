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
    // Two anchors in separate directories, neither referring to the other.
    // Nothing but the project connects them.
    let sandbox = Sandbox::new("distant");
    sandbox
        .file(
            "a/One.pi",
            "export anchor One:\n    rule: The save button is blue.\n",
        )
        .file(
            "b/Two.pi",
            "export anchor Two:\n    rule: The save button is red.\n",
        )
        .file(
            "main.pi",
            "from ./a/One import One\nfrom ./b/Two import Two\n\nexport anchor Root:\n    one: {One}\n    two: {Two}\n",
        );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    assert_eq!(report.findings.len(), 1, "{:#?}", report.findings);
    assert_eq!(
        report.findings[0].severity,
        Severity::Info,
        "structural distance must degrade the finding"
    );
}

#[test]
fn a_reference_between_anchors_is_close() {
    // An `@{...}` reference is the author saying these two are about each
    // other, which is strong evidence they share a subject.
    let sandbox = Sandbox::new("referenced");
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
    assert_eq!(report.findings[0].severity, Severity::Error);
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

#[test]
fn a_contradicted_relation_is_reported() {
    let found = findings(
        "export anchor Adapter:\n    emits: The adapter emits a skill.\n    denies: The adapter does not emit a skill.\n",
        "relation",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn relations_about_different_things_stay_quiet() {
    let found = findings(
        "export anchor Adapter:\n    a: The adapter emits a skill.\n    b: The adapter emits a command.\n    c: The adapter uses a template.\n    d: The package contains a manifest.\n",
        "relation-quiet",
    );
    assert!(
        found.is_empty(),
        "producing several things is ordinary prose: {found:#?}"
    );
}

#[test]
fn relation_extraction_adds_claims_without_adding_noise() {
    // Every extra claim is a chance for a false positive, so the useful
    // measure is that more statements are read and nothing new is reported.
    let sandbox = Sandbox::new("relation-volume");
    sandbox.file(
        "main.pi",
        "export anchor A:\n    a: The compiler emits a document.\n    b: The project contains a configuration.\n    c: The adapter supports a command.\n    d: Read the specification carefully.\n",
    );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    assert_eq!(report.claims, 3, "the imperative sentence makes no claim");
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
}

#[test]
fn a_specification_that_names_two_languages_is_reported() {
    // Two statements about one subject, sharing a predicate and differing only
    // in its complement. The lexicon knows neither value.
    let found = findings(
        "export anchor Cli:\n    description:\n        The CLI is written in Rust.\n        The CLI is written in Python.\n",
        "two-languages",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(
        found[0].0,
        Severity::Error,
        "one subject has one answer to what it is written in"
    );
}

#[test]
fn distance_still_degrades_a_competing_answer() {
    // The same two statements in loosely related anchors are the same
    // linguistic evidence with weaker structural evidence, so the finding
    // degrades rather than disappearing.
    let sandbox = Sandbox::new("distant-answers");
    sandbox
        .file(
            "a/One.pi",
            "export anchor One:\n    rule: The CLI is written in Rust.\n",
        )
        .file(
            "b/Two.pi",
            "export anchor Two:\n    rule: The CLI is written in Python.\n",
        )
        .file(
            "main.pi",
            "from ./a/One import One\nfrom ./b/Two import Two\n\nexport anchor Root:\n    one: {One}\n    two: {Two}\n",
        );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    assert_eq!(report.findings.len(), 1, "{:#?}", report.findings);
    assert_ne!(report.findings[0].severity, Severity::Error);
}

#[test]
fn a_restriction_makes_a_relation_exclusive() {
    // A package contains many things, so two `contains` claims agree. A list
    // that can *only* contain strings cannot also only contain booleans.
    let open = findings(
        "export anchor Lists:\n    a: Lists can contain strings.\n    b: Lists can contain booleans.\n",
        "relation-open",
    );
    assert!(open.is_empty(), "{open:#?}");

    let restricted = findings(
        "export anchor Lists:\n    a: Lists can only contain strings.\n    b: Lists can only contain booleans.\n",
        "relation-restricted",
    );
    assert_eq!(restricted.len(), 1, "{restricted:#?}");
    assert_eq!(restricted[0].0, Severity::Error);
}

#[test]
fn saying_the_same_thing_twice_is_not_a_contradiction() {
    let found = findings(
        "export anchor Cli:\n    a: The CLI is written in Rust.\n    b: The CLI is written in Rust.\n",
        "same-language",
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn unrelated_remarks_about_one_subject_stay_quiet() {
    // No shared frame, and nothing in the lexicon relates the values, so these
    // are two ordinary observations rather than a conflict.
    let found = findings(
        "export anchor Parser:\n    a: The parser is fast.\n    b: The parser is careful.\n    c: The parser is recursive.\n",
        "unrelated-remarks",
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn a_weak_observation_surfaces_when_the_bar_is_lowered() {
    // The specification asks for uncertainty to degrade a finding rather than
    // remove it, so the observation is still there to be asked for.
    let sandbox = Sandbox::new("lowered-bar");
    sandbox.file(
        "main.pi",
        "export anchor Parser:\n    a: The parser is fast.\n    b: The parser is careful.\n",
    );
    let compilation = sandbox.compile("main.pi");

    let quiet = analyze(&compilation, &Scope::Project, &Settings::default());
    assert!(quiet.findings.is_empty());

    let curious = Settings {
        minimum_contradiction: 0.2,
        ..Settings::default()
    };
    let report = analyze(&compilation, &Scope::Project, &curious);
    assert_eq!(report.findings.len(), 1, "{:#?}", report.findings);
    assert_eq!(report.findings[0].severity, Severity::Info);
}

#[test]
fn negation_survives_a_contraction() {
    // A contraction is one token -- the tokenizer keeps the apostrophe -- so
    // each one has to be spelled out. Missing, `won't` is not a negation and
    // not a function word, so it lands in the subject and the claim silently
    // reads as an affirmative one about "the adapter won't".
    for (positive, negative) in [
        ("will emit", "won't emit"),
        ("can emit", "can't emit"),
        ("has emitted", "hasn't emitted"),
        ("did emit", "didn't emit"),
    ] {
        let source = format!(
            "export anchor A:\n    a: The adapter {positive} a skill.\n    b: The adapter {negative} a skill.\n"
        );
        let found = findings(&source, &format!("neg-{positive}").replace(' ', "-"));
        assert_eq!(found.len(), 1, "{positive} vs {negative}: {found:#?}");
        assert_eq!(found[0].0, Severity::Error, "{positive} vs {negative}");
    }
}

#[test]
fn a_denied_subject_denies_the_claim() {
    // `no` is a determiner, so it is stripped as a function word. Stripping it
    // without recording it turns `no cursor is visible` into the claim that a
    // cursor is, which agrees with the statement it contradicts.
    let found = findings(
        "export anchor A:\n    a: No cursor is visible.\n    b: The cursor is visible.\n",
        "denied-subject",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);

    // Two negatives cancel rather than accumulate.
    let doubled = findings(
        "export anchor A:\n    a: No adapter has no cache.\n    b: The adapter has a cache.\n",
        "double-negative",
    );
    assert!(doubled.is_empty(), "{doubled:#?}");
}

#[test]
fn without_is_a_negative_preposition() {
    let found = findings(
        "export anchor A:\n    a: The build is with a cache.\n    b: The build is without a cache.\n",
        "without",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn existential_there_names_no_subject() {
    // `there` fills the subject slot without naming anything. Reading it as a
    // subject merges every existential sentence in the project into one.
    let found = findings(
        "export anchor A:\n    a: There is a visible cursor.\n    b: There is a hidden cursor.\n",
        "existential",
    );
    assert!(found.is_empty(), "{found:#?}");
}

#[test]
fn coverage_accounts_for_every_sentence() {
    // The point of the number is that it reconciles: a sentence is either read
    // or it is counted under a reason it was not. Anything that escapes both
    // makes the percentage a guess.
    let sandbox = Sandbox::new("coverage-sums");
    sandbox.file(
        "main.pi",
        "export anchor A:\n             a: The save button is blue.\n             b: The adapter emits a skill.\n             c: The parser frobnicates the input.\n             d: Overview\n             e: It is blue.\n",
    );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    let coverage = &report.coverage;

    let gaps: usize = coverage.gaps.values().sum();
    assert_eq!(
        coverage.claims + gaps,
        coverage.sentences,
        "claims {} + gaps {} != sentences {}: {coverage:#?}",
        coverage.claims,
        gaps,
        coverage.sentences
    );
    assert_eq!(coverage.claims, 2, "{coverage:#?}");
    assert!(coverage.understood() > 0.0 && coverage.understood() < 1.0);
}

#[test]
fn coverage_names_the_verbs_it_does_not_know() {
    let sandbox = Sandbox::new("coverage-verbs");
    sandbox.file(
        "main.pi",
        "export anchor A:\n    a: The parser will frobnicate the input.\n",
    );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    assert_eq!(
        report.coverage.unknown_verbs,
        vec![("frobnicate".to_string(), 1)],
        "{:#?}",
        report.coverage
    );
}

#[test]
fn an_inherited_sentence_is_counted_once() {
    // An inherited property resolves onto every descendant. Counting the same
    // sentence once per descendant inflates the denominator and makes coverage
    // depend on how deep the inheritance happens to go.
    let sandbox = Sandbox::new("coverage-inherit");
    sandbox
        .file(
            "base.pi",
            "export abstract anchor Base:\n    rule: The parser frobnicates the input.\n",
        )
        .file(
            "one.pi",
            "from ./base import Base\n\nexport anchor One extends Base:\n    label: first\n",
        )
        .file(
            "two.pi",
            "from ./base import Base\n\nexport anchor Two extends Base:\n    label: second\n",
        )
        .file(
            "main.pi",
            "from ./one import One\nfrom ./two import Two\n\nexport anchor Root:\n    a: {One}\n    b: {Two}\n",
        );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());

    assert_eq!(report.coverage.anchors, 4, "{:#?}", report.coverage);
    // `rule` resolves onto both One and Two, but it is one sentence.
    assert_eq!(
        report.coverage.gaps.get("no verb the lexicon knows"),
        Some(&1),
        "{:#?}",
        report.coverage
    );
    let gaps: usize = report.coverage.gaps.values().sum();
    assert_eq!(
        report.coverage.claims + gaps,
        report.coverage.sentences,
        "{:#?}",
        report.coverage
    );
}

#[test]
fn a_noun_that_is_also_a_verb_does_not_steal_the_verb_slot() {
    // `build` names a relation, and `the build` is a noun phrase. Taking the
    // first candidate would read `build` as the verb, leaving no subject in
    // front of it, and the sentence would go unread.
    let found = findings(
        "export anchor A:\n             a: The build writes a manifest.\n             b: The build does not write a manifest.\n",
        "noun-verb",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn added_verbs_compare_within_their_group_and_not_across() {
    // A group asserts that its members say the same thing, so `generates` has
    // to meet `emits`. Groups that stayed apart have to stay apart: `emits a
    // skill` and `exposes a skill` are different claims about one subject.
    let same = findings(
        "export anchor A:\n             a: The adapter can only generate a skill.\n             b: The adapter can only emit a command.\n",
        "verb-group-same",
    );
    assert_eq!(same.len(), 1, "generate and emit are one relation: {same:#?}");

    let across = findings(
        "export anchor A:\n             a: The adapter can only emit a skill.\n             b: The adapter can only expose a command.\n",
        "verb-group-across",
    );
    assert!(
        across.is_empty(),
        "emit and expose are different relations: {across:#?}"
    );
}

#[test]
fn an_irregular_past_form_reaches_its_verb() {
    let found = findings(
        "export anchor A:\n             a: The compiler was written in Rust.\n             b: The compiler was written in Python.\n",
        "irregular",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn an_imperative_is_a_claim_about_its_anchor() {
    // A specification is written in the imperative, and the subject is elided
    // rather than absent: it is the anchor the sentence was written in.
    let found = findings(
        "export anchor SkillAdapter:\n             a: Emit the prompt as the skill body.\n             b: Do not emit the prompt as the skill body.\n",
        "imperative",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn an_imperative_in_another_anchor_is_a_different_subject() {
    // Two adapters may legitimately disagree; they are not one subject.
    let sandbox = Sandbox::new("imperative-scope");
    sandbox
        .file(
            "claude.pi",
            "export anchor ClaudeAdapter:\n    rule: Emit the prompt as the skill body.\n",
        )
        .file(
            "codex.pi",
            "export anchor CodexAdapter:\n    rule: Do not emit the prompt as the skill body.\n",
        )
        .file(
            "main.pi",
            "from ./claude import ClaudeAdapter\nfrom ./codex import CodexAdapter\n\nexport anchor Root:\n    a: {ClaudeAdapter}\n    b: {CodexAdapter}\n",
        );
    let compilation = sandbox.compile("main.pi");
    let report = analyze(&compilation, &Scope::Project, &Settings::default());
    assert!(report.findings.is_empty(), "{:#?}", report.findings);
}

#[test]
fn a_derived_adverb_is_not_part_of_the_claim() {
    // Adverbs are an open class. `intentionally` is not in any table, and
    // taken as a content word it becomes part of the subject.
    let found = findings(
        "export anchor A:\n             a: Piton intentionally does not provide a way to access items.\n             b: Piton provides a way to access items.\n",
        "derived-adverb",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn a_filename_does_not_end_a_sentence() {
    let found = findings(
        "export anchor A:\n             a: The adapter reads AGENTS.md from the working directory.\n             b: The adapter does not read AGENTS.md from the working directory.\n",
        "filename",
    );
    assert_eq!(found.len(), 1, "{found:#?}");
    assert_eq!(found[0].0, Severity::Error);
}

#[test]
fn a_value_that_is_not_prose_is_not_an_unread_sentence() {
    // A list of adapter names is data. Counting each as an unread sentence
    // makes coverage a measure of how much data a specification holds.
    let sandbox = Sandbox::new("coverage-fragments");
    sandbox.file(
        "main.pi",
        concat!(
            "export anchor A:\n",
            "    adapters:\n",
            "        - claude-code\n",
            "        - codex\n",
            "        - opencode\n",
            "    rule: The save button is blue.\n",
        ),
    );
    let compilation = sandbox.compile("main.pi");
    let coverage = analyze(&compilation, &Scope::Project, &Settings::default()).coverage;
    assert_eq!(coverage.fragments, 3, "{coverage:#?}");
    assert_eq!(coverage.sentences, 1, "{coverage:#?}");
    assert_eq!(coverage.understood(), 1.0, "{coverage:#?}");
}
