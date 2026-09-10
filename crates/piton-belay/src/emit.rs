//! Producing Belay's output files.
//!
//! Agents, skills, and commands each become one Markdown file in the agent
//! directory. Instructions are different: they mirror the shape tree onto the
//! code tree, concatenating everything written at one scope into the
//! `AGENTS.md` for the matching code directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use piton_core::compile::Compilation;
use piton_core::framework::OutputFile;
use piton_core::value::{AnchorId, Dict, Value};

use crate::markdown::{block, front_matter, humanize, kebab, remainder};
use crate::module;
use crate::settings::{reference_path, relative_to, Adapter, Settings};

/// The kinds of thing Belay knows how to emit.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Agent,
    Skill,
    Command,
    Instruction,
}

impl Kind {
    fn anchor_name(self) -> &'static str {
        match self {
            Kind::Agent => "Agent",
            Kind::Skill => "Skill",
            Kind::Command => "Command",
            Kind::Instruction => "Instruction",
        }
    }
}

/// Everything Belay found to emit, resolved once per build.
pub struct Plan {
    pub agents: Vec<AnchorId>,
    pub skills: Vec<AnchorId>,
    pub commands: Vec<AnchorId>,
    pub instructions: Vec<AnchorId>,
}

impl Plan {
    /// True when the project declares anything for Belay to emit.
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
            && self.skills.is_empty()
            && self.commands.is_empty()
            && self.instructions.is_empty()
    }

    pub fn discover(compilation: &Compilation) -> Plan {
        let of = |kind: Kind| {
            compilation
                .lookup(module::MODULE, kind.anchor_name())
                .map(|id| compilation.implementors(id))
                .unwrap_or_default()
        };
        Plan {
            agents: of(Kind::Agent),
            skills: of(Kind::Skill),
            commands: of(Kind::Command),
            instructions: of(Kind::Instruction),
        }
    }
}

/// Emit every file for one adapter.
pub fn emit_adapter(
    compilation: &Compilation,
    settings: &Settings,
    adapter: &Adapter,
    plan: &Plan,
    referenced: &[AnchorId],
) -> Vec<OutputFile> {
    let mut files = Vec::new();
    let base = &settings.base;

    for id in &plan.agents {
        if let Some(anchor) = compilation.anchor(*id) {
            files.push(OutputFile {
                path: base.join(&adapter.directory).join("agents").join(format!(
                    "{}.md",
                    kebab(&anchor.name)
                )),
                contents: agent_file(&anchor.name, &anchor.props),
            });
        }
    }
    for id in &plan.skills {
        if let Some(anchor) = compilation.anchor(*id) {
            files.push(OutputFile {
                path: base
                    .join(&adapter.directory)
                    .join("skills")
                    .join(kebab(&anchor.name))
                    .join("SKILL.md"),
                contents: skill_file(&anchor.name, &anchor.props),
            });
        }
    }
    for id in &plan.commands {
        if let Some(anchor) = compilation.anchor(*id) {
            files.push(OutputFile {
                // Every Belay command is prefixed so it cannot collide.
                path: base.join(&adapter.directory).join("commands").join(format!(
                    "x-{}.md",
                    kebab(&anchor.name)
                )),
                contents: command_file(&anchor.props),
            });
        }
    }

    files.extend(instruction_files(compilation, settings, adapter, plan));
    files.extend(reference_files(compilation, settings, adapter, plan, referenced));

    // `@{}` produced project-relative links, because evaluation happens before
    // anyone knows which files the text lands in. Now that it is known, each
    // reference is rewritten relative to its own document.
    for file in &mut files {
        file.contents = localise_references(&file.contents, &file.path, settings, adapter);
    }
    files
}

/// Said once per document, because no agent follows a link on its own.
///
/// Claude Code's `@import` is the only inlining syntax any of these tools has,
/// and it works in memory files alone. Everywhere else a path is inert text, so
/// the document has to ask for the read.
const REFERENCE_NOTE: &str = "Links in this document point at reference files. \
Read one when the work touches what it describes.";

/// Point every reference at this adapter's copy, relative to `file`.
///
/// Evaluation produced one link per reference, using whichever adapter happened
/// to be first, because it runs before anyone knows which files the text lands
/// in. Every adapter writes its own copy of everything, so the leading agent
/// directory is swapped for this one before the path is made relative.
///
/// A reference is a Markdown link rather than Claude Code's `@` import. The
/// import is a memory file feature that no other agent implements, and even
/// where it works it pulls the whole reachable reference tree into context at
/// launch, four hops deep and silently truncated past that — the opposite of
/// what publishing references as separate files is for.
fn localise_references(
    contents: &str,
    file: &Path,
    settings: &Settings,
    adapter: &Adapter,
) -> String {
    let Some(directory) = file.parent() else { return contents.to_string() };
    let agent_directories: Vec<String> = settings
        .adapters
        .iter()
        .map(|it| format!("{}/", it.directory.display()))
        .collect();

    let mut out = String::with_capacity(contents.len());
    let mut rest = contents;
    let mut linked = false;
    while let Some(open) = rest.find("](") {
        let (head, tail) = rest.split_at(open + 2);
        out.push_str(&localise_shape(head, directory, settings, adapter));
        // Link syntax delimits itself, so unlike a bare path there is no
        // trailing punctuation to guess at.
        let Some(close) = tail.find(')') else { break };
        let target = &tail[..close];
        let within =
            agent_directories.iter().find_map(|prefix| target.strip_prefix(prefix.as_str()));
        match within {
            Some(inside) => {
                let target = settings.base.join(&adapter.directory).join(inside);
                out.push_str(&relative_to(directory, &target));
                linked = true;
            }
            // Not one of ours — a link someone wrote by hand — so it is left
            // exactly as written.
            None => out.push_str(target),
        }
        rest = &tail[close..];
    }
    out.push_str(&localise_shape(rest, directory, settings, adapter));

    if linked {
        while out.ends_with('\n') {
            out.pop();
        }
        out.push_str(&format!("\n\n{REFERENCE_NOTE}\n"));
    }
    out
}

/// Point every mention of the compiled shape directory at this adapter's copy,
/// relative to `directory`.
///
/// `__BELAY_SHAPE__` is a bare path rather than a link, and it is specified to
/// be relative to the file using it. Evaluation cannot know which file that is,
/// so it binds the project-relative directory and the rewrite happens here.
/// Only prose is passed in: a link's target is already being made relative by
/// the caller, and a relative target still spells the shape directory inside
/// itself, so rewriting it again would corrupt it.
fn localise_shape(
    prose: &str,
    directory: &Path,
    settings: &Settings,
    adapter: &Adapter,
) -> String {
    let target = settings.base.join(&adapter.directory).join("reference").join("shape");
    let here = relative_to(directory, &target);
    let mut out = prose.to_string();
    for other in &settings.adapters {
        let written = other.directory.join("reference").join("shape");
        out = out.replace(&written.display().to_string(), &here);
    }
    out
}

// ---- individual documents ------------------------------------------------

fn agent_file(name: &str, props: &Dict) -> String {
    let mut out = front_matter(&[
        ("name", Some(kebab(name))),
        ("description", text(props, "description")),
        ("tools", text(props, "tools")),
        ("model", text(props, "model")),
    ]);
    out.push('\n');
    if let Some(role) = text(props, "role") {
        out.push_str(&format!("You are a {role}\n\n"));
    }
    push_body(&mut out, props, &["description", "role", "prompt", "tools", "model", "name"], 1);
    out
}

fn skill_file(name: &str, props: &Dict) -> String {
    let description = match (text(props, "description"), text(props, "useWhen")) {
        (Some(description), Some(when)) => Some(format!("{description} Use when {when}")),
        (Some(description), None) => Some(description),
        (None, Some(when)) => Some(format!("Use when {when}")),
        (None, None) => None,
    };
    let mut out =
        front_matter(&[("name", Some(kebab(name))), ("description", description)]);
    out.push('\n');
    push_body(&mut out, props, &["description", "useWhen", "prompt", "name"], 1);
    out
}

fn command_file(props: &Dict) -> String {
    let mut out = front_matter(&[
        ("description", text(props, "description")),
        ("allowed-tools", text(props, "allowedTools").or_else(|| text(props, "allowed-tools"))),
        ("model", text(props, "model")),
    ]);
    out.push('\n');
    push_body(
        &mut out,
        props,
        &["description", "prompt", "allowedTools", "allowed-tools", "model"],
        1,
    );
    out
}

/// The prompt, then everything the front matter did not already say.
fn push_body(out: &mut String, props: &Dict, consumed: &[&str], depth: usize) {
    if let Some(prompt) = props.get("prompt") {
        let text = block(prompt, depth);
        if !text.trim().is_empty() {
            out.push_str(text.trim_end());
            out.push_str("\n\n");
        }
    }
    let rest = remainder(props, consumed, depth);
    if !rest.trim().is_empty() {
        out.push_str(rest.trim_end());
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
}

fn instruction_body(name: &str, props: &Dict) -> String {
    let mut out = format!("# {}\n\n", humanize(name));
    if let Some(description) = text(props, "description") {
        out.push_str(&format!("{description}\n\n"));
    }
    // The anchor's own name is the level-one heading, so its sections nest.
    push_body(&mut out, props, &["description", "prompt"], 2);
    out
}

// ---- instructions --------------------------------------------------------

/// Mirror the shape tree onto the code tree.
fn instruction_files(
    compilation: &Compilation,
    settings: &Settings,
    adapter: &Adapter,
    plan: &Plan,
) -> Vec<OutputFile> {
    let mut by_target: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
    let mut files = Vec::new();

    for id in &plan.instructions {
        let Some(anchor) = compilation.anchor(*id) else { continue };
        let body = instruction_body(&anchor.name, &anchor.props);
        let source = compilation.source_path(*id).map(Path::to_path_buf);

        let relative = source
            .as_ref()
            .zip(settings.shape_root.as_ref())
            .and_then(|(source, root)| source.strip_prefix(root).ok())
            .map(Path::to_path_buf);
        let scope = relative
            .as_ref()
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .unwrap_or_default();
        by_target.entry(nearest_existing(&settings.code_root, &scope)).or_default().push(body.clone());

        // Every instruction is also published under the agent directory.
        if let Some(source) = source {
            files.push(OutputFile {
                path: settings
                    .base
                    .join(reference_path(settings, adapter, &source, &anchor.name)),
                contents: body,
            });
        }
    }

    for (directory, bodies) in by_target {
        files.push(OutputFile {
            path: directory.join("AGENTS.md"),
            contents: format!("{}\n", bodies.join("\n\n").trim_end()),
        });
    }
    files
}

/// Walk up from a mirrored path until a real code directory is found.
fn nearest_existing(code_root: &Path, scope: &Path) -> PathBuf {
    let mut candidate = scope.to_path_buf();
    loop {
        let target = code_root.join(&candidate);
        if target.is_dir() {
            return target;
        }
        match candidate.parent() {
            Some(parent) if !candidate.as_os_str().is_empty() => candidate = parent.to_path_buf(),
            _ => return code_root.to_path_buf(),
        }
    }
}

// ---- references -----------------------------------------------------------

/// Publish every anchor that the output points at but does not itself emit.
fn reference_files(
    compilation: &Compilation,
    settings: &Settings,
    adapter: &Adapter,
    plan: &Plan,
    referenced: &[AnchorId],
) -> Vec<OutputFile> {
    let emitted: Vec<AnchorId> = plan
        .agents
        .iter()
        .chain(&plan.skills)
        .chain(&plan.commands)
        .chain(&plan.instructions)
        .copied()
        .collect();
    let mut files = Vec::new();
    let mut seen = Vec::new();
    for id in referenced {
        if emitted.contains(id) || seen.contains(id) {
            continue;
        }
        seen.push(*id);
        let Some(anchor) = compilation.anchor(*id) else { continue };
        let Some(source) = compilation.source_path(*id) else { continue };
        files.push(OutputFile {
            path: settings
                .base
                .join(reference_path(settings, adapter, source, &anchor.name)),
            contents: format!(
                "# {}\n\n{}\n",
                humanize(&anchor.name),
                block(&Value::Anchor(anchor.clone()), 2).trim_end()
            ),
        });
    }
    files
}

fn text(props: &Dict, key: &str) -> Option<String> {
    let value = props.get(key)?;
    let text = match value {
        Value::Str(text) => text.clone(),
        other if other.is_simple() => other.to_literal(),
        other => block(other, 1),
    };
    (!text.trim().is_empty()).then_some(text)
}
