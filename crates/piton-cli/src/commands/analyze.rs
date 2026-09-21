//! `piton analyze` — run semantic analysis over the project.
//!
//! Analysis reads what the prose says and combines it with what the Piton
//! structure knows, then reports where the two together suggest the specbase
//! contradicts itself.

use std::path::PathBuf;

use piton_analyze::{Finding, Report, Scope, Settings};
use piton_core::{Severity, LineIndex};

use crate::{project, report, EXIT_ERRORS, EXIT_SUCCESS};

pub fn run(targets: &[String], explain: bool, minimum: Option<&str>) -> u8 {
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
        combined.claims += part.claims;
        combined.sentences += part.sentences;
        combined.compared += part.compared;
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
    eprintln!("{errors} errors, {warnings} warnings, {information} informational");

    if errors > 0 {
        EXIT_ERRORS
    } else {
        EXIT_SUCCESS
    }
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
        let path = PathBuf::from(target);
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
