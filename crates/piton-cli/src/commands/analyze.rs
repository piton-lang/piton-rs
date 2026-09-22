//! `piton analyze` — run semantic analysis over the project.
//!
//! Analysis reads what the prose says and combines it with what the Piton
//! structure knows, then reports where the two together suggest the specbase
//! contradicts itself.

use std::path::Path;

use piton_analyze::{Coverage, Finding, Report, Scope, Settings};
use piton_core::{Severity, LineIndex};

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(
    targets: &[String],
    explain: bool,
    list_claims: bool,
    show_coverage: bool,
    format: Option<&str>,
    minimum: Option<&str>,
) -> u8 {
    let interpretation = match format {
        None | Some("human") => false,
        Some("interpretation") => true,
        Some(other) => {
            report::fail(format!(
                "unknown format `{other}`; use human or interpretation"
            ));
            return EXIT_ERRORS;
        }
    };
    let (loaded, _) = project::current();
    if loaded.config_path.is_none() && targets.is_empty() {
        report::fail("no piton.config.pi found; name a file or anchor to analyze");
        return EXIT_ERRORS;
    }
    let root = loaded.root.clone();
    let compilation = project::compile_project(loaded);

    let mut settings = Settings::default();
    if let Some(level) = minimum {
        match level {
            "error" => settings.minimum_contradiction = 0.85,
            "warning" => settings.minimum_contradiction = 0.6,
            "information" | "info" => settings.minimum_contradiction = 0.4,
            other => {
                report::fail(format!(
                    "unknown severity `{other}`; use error, warning, or information"
                ));
                return EXIT_ERRORS;
            }
        }
    }

    let scopes = match resolve_scopes(&compilation, targets) {
        Ok(scopes) => scopes,
        Err(message) => {
            report::fail(message);
            return EXIT_ERRORS;
        }
    };

    let mut combined = Report::default();
    for scope in &scopes {
        let part = piton_analyze::analyze(&compilation, scope, &settings);
        combined.findings.extend(part.findings);
        combined.statements.extend(part.statements);
        combined.claims += part.claims;
        combined.sentences += part.sentences;
        combined.compared += part.compared;
        merge_coverage(&mut combined.coverage, part.coverage);
    }

    if show_coverage {
        return print_coverage(&combined);
    }

    if interpretation {
        return print_interpretation(&combined, &compilation, &root);
    }

    if list_claims {
        return print_claims(&combined, &compilation, &root);
    }

    for finding in &combined.findings {
        print_finding(finding, &compilation, &root, explain);
    }

    let errors = combined.count(Severity::Error);
    let warnings = combined.count(Severity::Warning);
    let information = combined.count(Severity::Info);

    eprintln!(
        "{} {} produced a comparable claim; {} {} compared",
        combined.claims,
        report::plural(combined.claims, "statement", "statements"),
        combined.compared,
        report::plural(combined.compared, "pair", "pairs")
    );
    if combined.findings.is_empty() {
        eprintln!("no contradictions found");
        return EXIT_SUCCESS;
    }
    eprintln!(
        "{errors} {}, {warnings} {}, {information} informational",
        report::plural(errors, "error", "errors"),
        report::plural(warnings, "warning", "warnings")
    );

    if errors > 0 {
        EXIT_ERRORS
    } else {
        EXIT_SUCCESS
    }
}

/// Lists what the analysis understood, grouped by the file it came from.
///
/// A run that reports nothing is the goal, not a sign that nothing happened, so
/// this is how to see which statements were read and what each became.
fn print_claims(
    report: &Report,
    compilation: &piton_compile::Compilation,
    root: &std::path::Path,
) -> u8 {
    let mut current = std::path::PathBuf::new();
    for statement in &report.statements {
        if statement.path != current {
            current = statement.path.clone();
            println!("{}", project::display(&current, root));
        }
        let claim = &statement.claim;
        let negation = match claim.polarity {
            piton_analyze::claim::Polarity::Affirmative => "",
            piton_analyze::claim::Polarity::Negative => "not ",
        };
        println!(
            "  {}.{}  [{}]",
            statement.anchor_name,
            statement.site.property,
            claim.modality.as_str()
        );
        println!("    {}", statement.sentence.text);
        println!(
            "    -> subject: {} | property: {} | value: {negation}{}",
            claim.subject.head_with_modifiers(),
            claim.property.as_deref().unwrap_or("identity"),
            claim.value.head_with_modifiers()
        );
    }

    let files: std::collections::BTreeSet<&std::path::PathBuf> =
        report.statements.iter().map(|s| &s.path).collect();
    eprintln!(
        "{} {} from {} {}",
        report.statements.len(),
        report::plural(report.statements.len(), "claim", "claims"),
        files.len(),
        report::plural(files.len(), "file", "files")
    );
    let _ = compilation;
    EXIT_SUCCESS
}

/// Turns command-line targets into analysis scopes.
fn resolve_scopes(
    compilation: &piton_compile::Compilation,
    targets: &[String],
) -> Result<Vec<Scope>, String> {
    if targets.is_empty() {
        return Ok(vec![Scope::Project]);
    }
    let mut scopes = Vec::new();
    for target in targets {
        let path = project::canonical_target(Path::new(target));
        if path.is_file() {
            match compilation.graph().id_for(&path) {
                Some(module) => {
                    scopes.push(Scope::Module(module));
                    continue;
                }
                None => {
                    return Err(format!(
                        "`{target}` is not part of the project; nothing imports it"
                    ))
                }
            }
        }
        match compilation.find_anchor(target) {
            Some(anchor) => scopes.push(Scope::Anchor(anchor)),
            None => return Err(format!("no file or anchor named `{target}`")),
        }
    }
    Ok(scopes)
}

/// Prints a finding with both statements, their locations, and why they were
/// considered related.
fn print_finding(
    finding: &Finding,
    compilation: &piton_compile::Compilation,
    root: &std::path::Path,
    explain: bool,
) {
    let label = match finding.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "information",
    };
    eprintln!(
        "{label}: {} and {} describe `{}` incompatibly [contradiction]",
        finding.left.anchor_name,
        finding.right.anchor_name,
        finding.left.claim.subject.head_with_modifiers()
    );

    for statement in [&finding.left, &finding.right] {
        let location = locate(compilation, statement, root);
        eprintln!("  {location}");
        eprintln!("    {}", statement.sentence.text);
        eprintln!(
            "    claim: {}",
            statement.claim.render().replace('\n', ", ")
        );
    }

    if explain {
        for line in &finding.explanation {
            eprintln!("  note: {line}");
        }
    } else {
        eprintln!(
            "  confidence: semantic {:.2}, structural {:.2}, contradiction {:.2} ({})",
            finding.confidence.semantic,
            finding.confidence.structural,
            finding.confidence.contradiction,
            finding.relation.as_str()
        );
    }
    eprintln!();
}

/// Renders a statement's location as `path:line:col`, pointing at the sentence.
fn locate(
    compilation: &piton_compile::Compilation,
    statement: &piton_analyze::Statement,
    root: &std::path::Path,
) -> String {
    let display = project::display(&statement.path, root);
    let property = &statement.site.property;
    match compilation.source_of(&statement.path) {
        Some(source) => {
            let index = LineIndex::new(source);
            let (line, column) = index.line_col(statement.span().start);
            format!(
                "{display}:{}:{}  {}.{property}",
                line + 1,
                column + 1,
                statement.anchor_name
            )
        }
        None => format!("{display}  {}.{property}", statement.anchor_name),
    }
}

fn merge_coverage(into: &mut Coverage, from: Coverage) {
    into.files += from.files;
    into.anchors += from.anchors;
    into.properties += from.properties;
    into.prose_values += from.prose_values;
    into.sentences += from.sentences;
    into.claims += from.claims;
    into.mentioned += from.mentioned;
    into.fragments += from.fragments;
    for (reason, count) in from.gaps {
        *into.gaps.entry(reason).or_default() += count;
    }
    for (word, count) in from.unknown_verbs {
        match into.unknown_verbs.iter_mut().find(|(seen, _)| *seen == word) {
            Some(entry) => entry.1 += count,
            None => into.unknown_verbs.push((word, count)),
        }
    }
    into.unknown_verbs
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
}

/// How many unknown verbs to name before summarizing the rest.
const VERBS_SHOWN: usize = 15;

/// Prints how much of the specbase the analysis could read.
fn print_coverage(report: &Report) -> u8 {
    let coverage = &report.coverage;
    let share = |count: usize| -> String {
        if coverage.sentences == 0 {
            return "  0.0%".to_string();
        }
        format!("{:5.1}%", count as f64 / coverage.sentences as f64 * 100.0)
    };

    println!("Specbase");
    println!("  {:>6}  files", coverage.files);
    println!("  {:>6}  anchors", coverage.anchors);
    println!("  {:>6}  properties", coverage.properties);
    println!(
        "  {:>6}  prose values, counting nested ones",
        coverage.prose_values
    );
    println!("  {:>6}  prose sentences", coverage.sentences);
    if coverage.fragments > 0 {
        println!(
            "  {:>6}  values that are not prose (a list item, a name, a flag)",
            coverage.fragments
        );
    }

    let unread = coverage.sentences.saturating_sub(coverage.claims);
    println!();
    println!("Understood");
    println!(
        "  {:>6}  {}  sentences that produced a claim",
        coverage.claims,
        share(coverage.claims)
    );
    println!(
        "  {:>6}  {}  sentences that did not",
        unread,
        share(unread)
    );

    if !coverage.gaps.is_empty() {
        println!();
        println!("Not understood, by reason");
        // Largest first, so the biggest gap is the first thing read.
        let mut gaps: Vec<(&String, &usize)> = coverage.gaps.iter().collect();
        gaps.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (reason, count) in gaps {
            println!("  {:>6}  {}  {reason}", count, share(*count));
        }
    }

    if coverage.mentioned > 0 {
        println!();
        println!(
            "  {:>6}         sentences were quoted rather than asserted, and set aside",
            coverage.mentioned
        );
    }

    if !coverage.unknown_verbs.is_empty() {
        println!();
        println!("Words used as verbs that the lexicon does not know");
        println!("  Adding these to the relation lexicon is what would widen coverage.");
        for (word, count) in coverage.unknown_verbs.iter().take(VERBS_SHOWN) {
            println!("  {count:>6}  {word}");
        }
        let rest = coverage.unknown_verbs.len().saturating_sub(VERBS_SHOWN);
        if rest > 0 {
            println!("  {:>6}  more", rest);
        }
    }

    println!();
    println!(
        "{:.1}% of the specbase was read",
        coverage.understood() * 100.0
    );
    EXIT_SUCCESS
}

/// Prints a restatement of what the analysis understood.
///
/// A broken pipe is not an error here: `piton analyze --format=interpretation
/// | head` is the obvious way to look at it.
fn print_interpretation(
    report: &Report,
    compilation: &piton_compile::Compilation,
    root: &Path,
) -> u8 {
    use std::io::Write;
    let document = interpretation(report, compilation, root);
    let stdout = std::io::stdout();
    let _ = stdout.lock().write_all(document.as_bytes());
    EXIT_SUCCESS
}

/// Builds the restatement, as a document.
///
/// This is written for a coding agent rather than for a person reading the
/// specification: the source already says what it says, and says it better.
/// What an agent cannot get from the source is which parts of it were read
/// unambiguously enough to act on, so that is what this is -- each claim
/// rewritten as a flat sentence, grouped by the anchor it constrains, with the
/// line it came from so the agent can go back and check.
///
/// The header states the share of the prose that produced a claim, because a
/// restatement that silently covered half a specification would be worse than
/// none at all.
fn interpretation(
    report: &Report,
    compilation: &piton_compile::Compilation,
    root: &Path,
) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let coverage = &report.coverage;

    let _ = writeln!(out, "# Specification interpretation\n");
    let _ = writeln!(
        out,
        "Generated from {} {} across {} {}. Every sentence below is rebuilt from \
         what the analysis extracted rather than copied from the specification, \
         so a restatement that reads oddly is a claim that was read oddly. Verbs \
         are written as the relation they were understood as, which is why `emits` \
         may come back as `produces`.\n",
        coverage.claims,
        report::plural(coverage.claims, "claim", "claims"),
        coverage.anchors,
        report::plural(coverage.anchors, "anchor", "anchors"),
    );
    let _ = writeln!(
        out,
        "**This is a partial reading.** {:.0}% of the specification's {} prose {} \
         produced a claim; the rest could not be read and is not represented here. \
         The source is authoritative and this is an index into it.",
        coverage.understood() * 100.0,
        coverage.sentences,
        report::plural(coverage.sentences, "sentence", "sentences"),
    );

    if !report.findings.is_empty() {
        let _ = writeln!(out, "\n## Conflicts\n");
        let _ = writeln!(
            out,
            "The specification contradicts itself in {} {}. Resolve {} before \
             relying on the sections below.\n",
            report.findings.len(),
            report::plural(report.findings.len(), "place", "places"),
            if report.findings.len() == 1 { "it" } else { "them" },
        );
        for finding in &report.findings {
            let _ = writeln!(
                out,
                "- `{}` against `{}` — {} {}",
                finding.left.anchor_name,
                finding.right.anchor_name,
                piton_analyze::nlg::realize(&finding.left.claim),
                piton_analyze::nlg::realize(&finding.right.claim),
            );
        }
    }

    // Statements arrive in source order, so anchors keep the order the
    // specification introduces them in.
    let mut current = String::new();
    let mut said: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for statement in &report.statements {
        if statement.anchor_name != current {
            current = statement.anchor_name.clone();
            said.clear();
            let _ = writeln!(out, "\n## {}\n", statement.anchor_name);
            let _ = writeln!(out, "`{}`\n", project::display(&statement.path, root));
        }
        let sentence = piton_analyze::nlg::realize(&statement.claim);
        // One sentence can be written in several properties of one anchor, and
        // restating it twice says nothing the first did not.
        if !said.insert(sentence.clone()) {
            continue;
        }
        match compilation.source_of(&statement.path) {
            Some(source) => {
                let (line, _) = LineIndex::new(source).line_col(statement.span().start);
                let _ = writeln!(
                    out,
                    "- {sentence} _({}:{})_",
                    project::display(&statement.path, root),
                    line + 1
                );
            }
            None => {
                let _ = writeln!(out, "- {sentence}");
            }
        }
    }
    out
}
