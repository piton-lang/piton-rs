//! Does Belay's output conform to what the tools that read it require?
//!
//! The rules encoded here are the ones Claude Code enforces on the files it
//! loads, and the ones Markdown itself imposes:
//!
//! * Front matter must open the file and be closed, and its scalars must stay
//!   on one line or the YAML is malformed.
//! * A skill's `name` must be lowercase kebab-case and at most 64 characters,
//!   and must match the directory it lives in; its `description` is required.
//! * A sub-agent needs `name` and `description`; `tools` is a comma-separated
//!   list; a command's `allowed-tools` is the same shape.
//! * Markdown has six heading levels; anything deeper has to render another way.
//! * A reference that points at nothing is worse than no reference at all.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::{build, normalise, CONFIG};
use piton_core::framework::OutputFile;

/// Names Claude Code accepts for a skill, an agent, or a command.
fn is_kebab_case(name: &str) -> bool {
    !name.is_empty()
        && name
            .split('-')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()))
}

/// Split a Markdown file into its front matter and its body.
///
/// Returns `None` when the file has no front matter, which is itself a failure
/// for the files that need it.
fn front_matter(contents: &str) -> Option<(BTreeMap<String, String>, &str)> {
    let rest = contents.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    let (block, body) = rest.split_at(end);
    let mut fields = BTreeMap::new();
    for line in block.lines() {
        let (key, value) = line.split_once(':')?;
        // A raw newline inside a scalar would end the document early.
        assert!(!value.contains('\n'));
        fields.insert(key.trim().to_string(), value.trim().to_string());
    }
    Some((fields, body.strip_prefix("\n---\n").unwrap_or(body)))
}

/// One project exercising every Belay construct at once.
fn everything() -> (std::path::PathBuf, Vec<OutputFile>) {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/components/button/.keep", ""),
        ("spec/index.pi", "from ./Definitions export *\nfrom ./shape export *\n"),
        (
            "spec/Definitions.pi",
            "\
use @piton/belay

from ./shape/components/button/Button import ButtonDesign

export agent ReviewBot:
    description: Reviews a change against the recorded design intent
    role: careful reviewer
    prompt:
        Read the diff, then read the shape document for anything you touch.
    tools: Read, Grep, Bash
    model: opus

    checklist:
        - Does it match the intent?
        - Are all the states handled?

    deeply:
        nested:
            further:
                down:
                    again:
                        and:
                            once:
                                - a list keeps every level from being flat

export skill BuildComponent:
    description: Implements a component from its shape document
    useWhen: the user asks to build or change a component
    prompt: Follow the design at @{ButtonDesign}.

export command Ship:
    description: Runs the checks before anything leaves the branch
    allowedTools: Bash, Read
    model: sonnet
    prompt: Run the tests, then report.
",
        ),
        ("spec/shape/index.pi", "from ./components/button/Button export *\n"),
        (
            "spec/shape/components/button/Button.pi",
            "\
use @piton/belay

export anchor ButtonTokens:
    blue: The one blue, defined once

// A reference inside a reference: the chain has to resolve and both ends have
// to be published, which is what an `@` import could not promise past its
// fourth hop.
export anchor ButtonDesign:
    surface: Contrasting text on @{ButtonTokens}

    states:
        - Default
        - Hover

export instruction ButtonComponent:
    description: The clickable button component
    prompt:
        It should be clickable, and it should have a hover state.

        For the design, read @{ButtonDesign}.
",
        ),
        // Not imported by anything: a self-instruction is compiled regardless,
        // and its reference has to resolve from inside the specification.
        (
            "spec/scope/Guide.pi",
            "\
use @piton/belay

from ../shape/components/button/Button import ButtonDesign

export self-instruction ScopeGuide:
    description: How scope documents are written
    prompt: Point at @{ButtonDesign} rather than restating it.
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    (root, outputs)
}

fn relative(output: &OutputFile, root: &Path) -> String {
    output.path.strip_prefix(root).unwrap_or(&output.path).display().to_string()
}

#[test]
fn skills_conform_to_what_claude_code_loads() {
    let (root, outputs) = everything();
    let skills: Vec<&OutputFile> =
        outputs.iter().filter(|it| relative(it, &root).starts_with(".claude/skills/")).collect();
    assert!(!skills.is_empty(), "the fixture declares a skill");

    for skill in skills {
        let path = relative(skill, &root);
        assert!(path.ends_with("/SKILL.md"), "a skill lives in SKILL.md, not {path}");

        let (fields, body) = front_matter(&skill.contents)
            .unwrap_or_else(|| panic!("{path} has no front matter:\n{}", skill.contents));
        let name = fields.get("name").unwrap_or_else(|| panic!("{path} has no name"));
        assert!(is_kebab_case(name), "{path}: `{name}` is not lowercase kebab-case");
        assert!(name.len() <= 64, "{path}: `{name}` is longer than 64 characters");

        // The directory has to agree with the name, or the skill will not load.
        let directory = Path::new(&path).parent().unwrap().file_name().unwrap();
        assert_eq!(directory.to_string_lossy(), *name, "{path}: directory and name disagree");

        let description =
            fields.get("description").unwrap_or_else(|| panic!("{path} has no description"));
        assert!(!description.is_empty(), "{path}: the description is required");
        assert!(description.len() <= 1024, "{path}: the description is over 1024 characters");
        // The specification asks for `{description} Use when {useWhen}`.
        assert!(description.contains("Use when"), "{path}: {description}");
        assert!(!body.trim().is_empty(), "{path} has no body");
    }
}

#[test]
fn agents_conform_to_what_claude_code_loads() {
    let (root, outputs) = everything();
    let agents: Vec<&OutputFile> =
        outputs.iter().filter(|it| relative(it, &root).starts_with(".claude/agents/")).collect();
    assert!(!agents.is_empty());

    for agent in agents {
        let path = relative(agent, &root);
        let (fields, body) = front_matter(&agent.contents)
            .unwrap_or_else(|| panic!("{path} has no front matter"));

        let name = fields.get("name").unwrap_or_else(|| panic!("{path} has no name"));
        assert!(is_kebab_case(name), "{path}: `{name}` is not lowercase kebab-case");
        // The file name is how the agent is addressed.
        let stem = Path::new(&path).file_stem().unwrap().to_string_lossy();
        assert_eq!(stem, *name, "{path}: file name and `name` disagree");

        assert!(fields.contains_key("description"), "{path}: a sub-agent needs a description");
        if let Some(tools) = fields.get("tools") {
            for tool in tools.split(',') {
                assert!(!tool.trim().is_empty(), "{path}: empty entry in `tools`");
            }
        }
        // The role becomes the opening line, per the specification's shape.
        assert!(body.contains("You are a"), "{path}:\n{body}");
        assert!(!body.trim().is_empty());
    }
}

#[test]
fn commands_are_prefixed_and_carry_the_right_front_matter() {
    let (root, outputs) = everything();
    let commands: Vec<&OutputFile> =
        outputs.iter().filter(|it| relative(it, &root).starts_with(".claude/commands/")).collect();
    assert!(!commands.is_empty());

    for command in commands {
        let path = relative(command, &root);
        let stem = Path::new(&path).file_stem().unwrap().to_string_lossy().to_string();
        assert!(stem.starts_with("x-"), "{path}: every Belay command is prefixed `x-`");
        assert!(is_kebab_case(&stem), "{path}: `{stem}` is not lowercase kebab-case");

        let (fields, body) = front_matter(&command.contents)
            .unwrap_or_else(|| panic!("{path} has no front matter"));
        assert!(fields.contains_key("description"), "{path}");
        // Claude Code spells this key with a hyphen.
        assert!(!fields.contains_key("allowedTools"), "{path}: the key is `allowed-tools`");
        if fields.contains_key("allowed-tools") {
            assert!(!fields["allowed-tools"].is_empty());
        }
        // A command is addressed by its file name, so `name` is not a key.
        assert!(!fields.contains_key("name"), "{path}: commands take their name from the file");
        assert!(!body.trim().is_empty());
    }
}

/// Every generated `AGENTS.md` under `prefix` has a `CLAUDE.md` beside it that
/// imports it, and nothing else is written there.
///
/// Claude Code reads `CLAUDE.md`, not `AGENTS.md`, and resolves a relative
/// import against the importing file, so a bare `@AGENTS.md` is the sibling.
fn assert_paired_with_claude_files(outputs: &[OutputFile], root: &Path, prefix: &str) {
    let under: Vec<&OutputFile> =
        outputs.iter().filter(|it| relative(it, root).starts_with(prefix)).collect();
    assert!(!under.is_empty(), "nothing written under {prefix}");

    for file in under {
        let path = relative(file, root);
        let sibling = |name: &str| file.path.parent().unwrap().join(name);
        if path.ends_with("/CLAUDE.md") {
            assert_eq!(file.contents, "@AGENTS.md\n", "{path}");
            assert!(
                outputs.iter().any(|it| it.path == sibling("AGENTS.md")),
                "{path} imports an AGENTS.md that is never written"
            );
        } else {
            assert!(path.ends_with("/AGENTS.md"), "{path}: only AGENTS.md and CLAUDE.md belong here");
            assert!(!file.contents.trim().is_empty(), "{path}");
            assert!(
                outputs.iter().any(|it| it.path == sibling("CLAUDE.md")),
                "{path} has no CLAUDE.md, so Claude Code never reads it"
            );
        }
    }
}

#[test]
fn instructions_pair_each_agents_file_with_a_claude_import() {
    let (root, outputs) = everything();
    assert_paired_with_claude_files(&outputs, &root, "src/");
}

#[test]
fn self_instructions_compile_into_the_specification_as_agents_files() {
    let (root, outputs) = everything();
    assert_paired_with_claude_files(&outputs, &root, "spec/");
    let guide = outputs
        .iter()
        .find(|it| relative(it, &root) == "spec/scope/AGENTS.md")
        .expect("a self-instruction writes beside itself");
    assert!(guide.contents.contains("[ButtonDesign](../../.claude/reference/shape/"), "{}", guide.contents);
}

#[test]
fn every_output_is_well_formed_markdown() {
    let (root, outputs) = everything();
    for output in &outputs {
        let path = relative(output, &root);
        let body = match front_matter(&output.contents) {
            Some((_, body)) => body,
            None => output.contents.as_str(),
        };
        assert!(!output.contents.is_empty(), "{path} is empty");
        assert!(output.contents.ends_with('\n'), "{path} has no final newline");

        for line in body.lines() {
            if let Some(hashes) = line.strip_suffix(line.trim_start_matches('#')) {
                if !hashes.is_empty() {
                    assert!(
                        hashes.len() <= 6,
                        "{path}: Markdown has six heading levels, not {}: {line}",
                        hashes.len()
                    );
                    assert!(line.starts_with(&format!("{hashes} ")), "{path}: {line}");
                }
            }
        }
    }
}

#[test]
fn nothing_unrendered_reaches_the_output() {
    let (root, outputs) = everything();
    for output in &outputs {
        let path = relative(output, &root);
        for leftover in ["${", "@{", "reference{"] {
            assert!(
                !output.contents.contains(leftover),
                "{path} still contains {leftover}:\n{}",
                output.contents
            );
        }
        // A bare `{name}` expression should have been evaluated away.
        assert!(
            !output.contents.contains("{__BELAY"),
            "{path} leaked a builtin variable:\n{}",
            output.contents
        );
    }
}

#[test]
fn every_reference_points_at_a_file_that_is_written() {
    let (root, outputs) = everything();
    let written: Vec<std::path::PathBuf> =
        outputs.iter().map(|output| normalise(&output.path)).collect();
    let mut checked = 0;

    for output in &outputs {
        let directory = output.path.parent().unwrap();
        for target in links(&output.contents) {
            checked += 1;
            let target = normalise(&directory.join(&target));
            assert!(
                written.contains(&target),
                "{} points at {}, which is never written",
                relative(output, &root),
                target.display()
            );
        }
    }
    assert!(checked >= 4, "the fixture should exercise several references, saw {checked}");
}

/// The target of every `[text](target)` link in a document.
fn links(contents: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = contents;
    while let Some(open) = rest.find("](") {
        rest = &rest[open + 2..];
        let Some(close) = rest.find(')') else { break };
        targets.push(rest[..close].to_string());
        rest = &rest[close + 1..];
    }
    targets
}

/// `@` survives in exactly one place: the import a `CLAUDE.md` is made of.
///
/// Everywhere else it would be dead text, because no agent but Claude Code
/// implements it and Claude Code implements it only in memory files.
#[test]
fn no_output_points_at_a_file_with_an_import() {
    let (root, outputs) = everything();
    for output in &outputs {
        let path = relative(output, &root);
        if path.ends_with("CLAUDE.md") {
            continue;
        }
        for word in output.contents.split_whitespace() {
            assert!(
                !word.starts_with('@'),
                "{path} uses an `@` import, which only resolves in a memory file: {word}"
            );
        }
    }
}

#[test]
fn deep_nesting_falls_back_to_bold_instead_of_a_seventh_heading() {
    let (root, outputs) = everything();
    let agent = outputs
        .iter()
        .find(|it| relative(it, &root).ends_with("review-bot.md"))
        .expect("the agent is written");
    // The fixture nests seven levels deep on purpose.
    assert!(agent.contents.contains("###### "), "expected a sixth-level heading:\n{}", agent.contents);
    assert!(agent.contents.contains("**"), "expected a bold fallback:\n{}", agent.contents);
}
