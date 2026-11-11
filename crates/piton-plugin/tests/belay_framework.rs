//! Checks that the bundled `@piton/belay` package is the specification.
//!
//! The four constructs are not written in Rust. `plugins/belay/pi/*.pi`
//! is embedded into the compiler at build time and served from a virtual
//! filesystem, so the specification's description of a skill is the skill a
//! project actually gets.
//!
//! These tests hold that arrangement in place. One checks the shape the
//! compiler serves against the shape the file on disk declares; the other checks
//! that each construct arrives through its keyword, since a construct with no
//! alias cannot be used whatever its shape.

use std::path::{Path, PathBuf};

use piton_compile::{Compilation, Project, Symbol};
use piton_core::AnchorId;

/// The four agentic constructs, plus the base they share.
const CONSTRUCTS: &[&str] = &["Construct", "Instruction", "Skill", "Command", "Agent"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf()
}

struct Sandbox {
    dir: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Sandbox {
    fn new() -> Sandbox {
        // The thread id keeps parallel tests in this binary from sharing a
        // directory and deleting it out from under each other.
        let dir = std::env::temp_dir().join(format!(
            "piton-framework-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Sandbox { dir }
    }
}

/// Compiles a file that pulls every construct out of the bundled package.
fn bundled(sandbox: &Sandbox) -> Compilation {
    let entry = sandbox.dir.join("main.pi");
    std::fs::write(
        &entry,
        format!("from @piton/belay import\n    {}\n", CONSTRUCTS.join(",\n    ")),
    )
    .expect("write");
    let mut project = Project::for_file(&entry);
    project.source_root = sandbox.dir.clone();
    project.root = sandbox.dir.clone();
    Compilation::build(project)
}

/// Compiles the specification's own description of one construct.
fn specified(name: &str) -> Compilation {
    let root = repo_root();
    let entry = root.join(format!("plugins/belay/pi/{name}.pi"));
    let mut project = Project::for_file(&entry);
    project.source_root = root.join("spec");
    project.root = root;
    Compilation::build(project)
}

/// A comparable rendering of an anchor's declared shape.
///
/// Values are left out on purpose: an abstract construct declares a shape, and
/// it is the shape that has to agree.
fn shape(compilation: &Compilation, anchor: AnchorId) -> String {
    let def = compilation.store().anchor(anchor);
    let mut out = String::new();
    if def.is_abstract {
        out.push_str("abstract ");
    }
    out.push_str(&def.name);
    if let Some(alias) = &def.alias {
        out.push_str(&format!(" as {alias}"));
    }
    let bases: Vec<&str> = def
        .bases
        .iter()
        .map(|base| compilation.store().anchor(*base).name.as_str())
        .collect();
    if !bases.is_empty() {
        out.push_str(&format!(" extends {}", bases.join(", ")));
    }
    out.push('\n');

    for (name, slot) in &def.slots {
        let constraints: Vec<String> = slot
            .constraints
            .iter()
            .map(piton_compile::eval::constraint_label)
            .collect();
        out.push_str(&format!(
            "    {name}:: {}{}\n",
            if constraints.is_empty() {
                "any".to_string()
            } else {
                constraints.join(":: ")
            },
            if slot.has_value { " = <value>" } else { "" }
        ));
    }
    out
}

fn find(compilation: &Compilation, name: &str) -> AnchorId {
    compilation
        .find_anchor(name)
        .unwrap_or_else(|| panic!("`{name}` not found"))
}

#[test]
fn the_bundled_package_matches_the_specification() {
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    assert!(
        !bundled.has_errors(),
        "the bundled package must compile: {:#?}",
        bundled.diagnostics.as_slice()
    );

    let mut differences = Vec::new();
    for name in CONSTRUCTS {
        let described = specified(name);
        assert!(
            !described.has_errors(),
            "`plugins/belay/pi/{name}.pi` must compile: {:#?}",
            described.diagnostics.as_slice()
        );

        let shipped = shape(&bundled, find(&bundled, name));
        let documented = shape(&described, find(&described, name));
        if shipped != documented {
            differences.push(format!(
                "{name}\n  compiler serves:\n{}\n  the file declares:\n{}",
                indent(&shipped),
                indent(&documented)
            ));
        }
    }

    assert!(
        differences.is_empty(),
        "the bundled `@piton/belay` no longer matches the files it embeds:\n\n{}",
        differences.join("\n\n")
    );
}

#[test]
fn the_constructs_are_served_from_the_specification_files() {
    // The shape matching above would still pass if someone reintroduced a
    // hand-written copy. This is what makes that impossible: the module a
    // construct arrives in has to be the embedded specification file, and its
    // text has to be what is on disk.
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    let root = repo_root();

    for name in CONSTRUCTS {
        let anchor = find(&bundled, name);
        let module = bundled.anchor_module_path(anchor);
        assert_eq!(
            module,
            PathBuf::from(format!("@piton/belay/{name}")),
            "`{name}` must come from the embedded specification file"
        );

        let served = bundled
            .source_of(&module)
            .unwrap_or_else(|| panic!("no source for {name}"));
        let on_disk = std::fs::read_to_string(
            root.join(format!("plugins/belay/pi/{name}.pi")),
        )
        .expect("readable");
        assert_eq!(
            served, on_disk,
            "`{name}` is served from a copy rather than from the file itself"
        );
    }
}

#[test]
fn the_package_exports_the_framework_and_not_its_documentation() {
    // The construct files also hold the prose that documents them. Those
    // anchors are reachable inside the package but must not leak out of it, or
    // `from @piton/belay import SkillBehavior` would start working.
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    let module = bundled
        .graph()
        .id_for(Path::new("@piton/belay"))
        .expect("the package is loaded");

    let exported = bundled.resolution.exported_names(module);
    for name in CONSTRUCTS {
        assert!(exported.contains(&name.to_string()), "{name} is not exported");
    }
    for documentation in [
        "SkillBehavior",
        "AgentBehavior",
        "CommandBehavior",
        "InstructionBehavior",
        "FrameworkSpecification",
    ] {
        assert!(
            !exported.contains(&documentation.to_string()),
            "`{documentation}` is documentation and must stay inside the package"
        );
    }
}

#[test]
fn every_construct_is_reachable_through_its_keyword() {
    let sandbox = Sandbox::new();
    let bundled = bundled(&sandbox);
    let module = bundled
        .graph()
        .id_for(Path::new("@piton/belay"))
        .expect("the package is loaded");

    // A construct is used by writing its keyword, so a missing alias makes the
    // construct unusable no matter what its shape is.
    for (name, keyword) in [
        ("Instruction", "instruction"),
        ("Skill", "skill"),
        ("Command", "command"),
        ("Agent", "agent"),
    ] {
        let Some(Symbol::Anchor(anchor)) =
            bundled
                .resolution
                .lookup_export(module, name, &mut Default::default())
        else {
            panic!("`{name}` is not exported by @piton/belay");
        };
        assert_eq!(
            bundled.store().anchor(anchor).alias.as_deref(),
            Some(keyword),
            "`{name}` must be usable as `{keyword}`"
        );
    }
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("    {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_adapters_are_served_from_the_specification_files() {
    // Like the constructs, the adapter anchors a project lists are the
    // specification's own files, not a hand-written copy.
    let sandbox = Sandbox::new();
    let entry = sandbox.dir.join("main.pi");
    std::fs::write(
        &entry,
        "from @piton/belay import ClaudeCodeAdapter, CodexAdapter, OpenCodeAdapter\n",
    )
    .expect("write");
    let mut project = Project::for_file(&entry);
    project.source_root = sandbox.dir.clone();
    project.root = sandbox.dir.clone();
    let bundled = Compilation::build(project);
    assert!(
        !bundled.has_errors(),
        "{:#?}",
        bundled.diagnostics.as_slice()
    );
    let root = repo_root();

    for (name, file) in [
        ("ClaudeCodeAdapter", "ClaudeCode"),
        ("CodexAdapter", "Codex"),
        ("OpenCodeAdapter", "OpenCode"),
    ] {
        let anchor = find(&bundled, name);
        let module = bundled.anchor_module_path(anchor);
        assert_eq!(module, PathBuf::from(format!("@piton/belay/adapters/{file}")));
        let served = bundled.source_of(&module).expect("source");
        let on_disk = std::fs::read_to_string(
            root.join(format!("plugins/belay/pi/adapters/{file}.pi")),
        )
        .expect("readable");
        assert_eq!(served, on_disk, "`{name}` is served from a copy");
    }
}

/// The facts the plugin compares between an adapter anchor and its built-in
/// target, as in `targetFromAnchor` and `builtinTarget` in
/// `plugins/belay/src/adapters.ts`.
#[derive(Debug, PartialEq)]
struct Facts {
    root: String,
    instruction_file: String,
    reference_root: String,
    skill_output: String,
    command_output: String,
    policy_output: Option<String>,
    agent_output: String,
    command_support: String,
    agent_format: String,
    default_mode: Option<String>,
    documentation_checked: String,
}

/// The built-in adapters of the plugin, read from the object literals in
/// `adapters.ts`, keyed by export name: each field the literal states as a
/// string.
fn builtin_adapters() -> Vec<std::collections::BTreeMap<String, String>> {
    let source = std::fs::read_to_string(repo_root().join("plugins/belay/src/adapters.ts")).expect("adapters.ts");
    let mut adapters = Vec::new();
    let mut current: Option<std::collections::BTreeMap<String, String>> = None;
    for line in source.lines() {
        if line.starts_with("const ") && line.ends_with(": Adapter = {") {
            current = Some(Default::default());
        } else if line == "};" {
            if let Some(adapter) = current.take() {
                adapters.push(adapter);
            }
        } else if let Some(adapter) = current.as_mut() {
            let field = line.strip_prefix("  ").filter(|rest| !rest.starts_with(' '));
            if let Some((key, value)) = field.and_then(|rest| rest.split_once(": ")) {
                if let Some(text) = value.strip_suffix(',').and_then(|v| v.strip_prefix('"')).and_then(|v| v.strip_suffix('"')) {
                    adapter.insert(key.to_string(), text.to_string());
                }
            }
        }
    }
    adapters
}

fn builtin_facts(adapter: &std::collections::BTreeMap<String, String>) -> Facts {
    let field = |key: &str| adapter.get(key).cloned().unwrap_or_else(|| panic!("no `{key}` in {adapter:?}"));
    let support = field("commandSupport");
    let format = field("agentFormat");
    let command_root = field("commandRoot");
    Facts {
        root: field("root"),
        instruction_file: field("instructionFile"),
        reference_root: field("referenceRoot"),
        skill_output: format!("{}/<name>/SKILL.md", field("skillRoot")),
        command_output: if support == "native" {
            format!("{command_root}/x-<name>.md")
        } else {
            format!("{command_root}/x-<name>/SKILL.md")
        },
        policy_output: (support == "translated-skill-with-policy")
            .then(|| format!("{command_root}/x-<name>/agents/openai.yaml")),
        agent_output: format!(
            "{}/<name>.{}",
            field("agentRoot"),
            if format == "toml" { "toml" } else { "md" }
        ),
        default_mode: (format == "markdown-filename-identity").then(|| "subagent".to_string()),
        command_support: support,
        agent_format: format,
        documentation_checked: field("documentationChecked"),
    }
}

fn lookup<'a>(properties: &'a piton_core::Properties, path: &[&str]) -> Option<&'a piton_core::Value> {
    let (first, rest) = path.split_first()?;
    let mut value = properties.get(*first)?;
    for key in rest {
        value = match value {
            piton_core::Value::Dict(map) => map.get(*key)?,
            piton_core::Value::Mixed(mixed) => mixed.get(key)?,
            _ => return None,
        };
    }
    Some(value)
}

/// The facts as the plugin reads them from an adapter anchor, starting from
/// the built-in ones.
fn anchor_facts(builtin: Facts, properties: &piton_core::Properties) -> Facts {
    let text = |path: &[&str]| {
        lookup(properties, path).and_then(|value| match value {
            piton_core::Value::Str(text) => text.as_plain().map(|t| t.trim().to_string()),
            _ => None,
        })
    };
    let template = |path: &[&str]| text(path).filter(|output| output.contains("<name>"));
    let mut facts = builtin;
    if let Some(file) = text(&["instructionFile"]) {
        facts.instruction_file = file;
    }
    if let Some(root) = text(&["referenceRoot"]) {
        let root = root.trim_end_matches('/').to_string();
        facts.root = root.rsplit_once('/').map_or(root.clone(), |(parent, _)| parent.to_string());
        facts.reference_root = root;
    }
    if let Some(checked) = text(&["documentationChecked"]) {
        facts.documentation_checked = checked;
    }
    if let Some(output) = template(&["skill", "output"]) {
        facts.skill_output = output;
    }
    if let Some(output) = template(&["command", "output"]) {
        facts.command_output = output;
    }
    if let Some(output) = template(&["command", "policyOutput"]) {
        facts.policy_output = Some(output);
    }
    if let Some(output) = template(&["agent", "output"]) {
        facts.agent_output = output;
    }
    if let Some(support) = text(&["command", "support"]) {
        facts.command_support = if support == "native" {
            "native"
        } else if facts.policy_output.is_some() {
            "translated-skill-with-policy"
        } else {
            "translated-skill"
        }
        .to_string();
    }
    if facts.agent_output.ends_with(".toml") {
        facts.agent_format = "toml".to_string();
    } else if let Some(metadata) = lookup(properties, &["agent", "metadata"]) {
        let named = match metadata {
            piton_core::Value::Dict(map) => map.contains_key("name"),
            piton_core::Value::Mixed(mixed) => mixed.get("name").is_some(),
            _ => false,
        };
        facts.agent_format = if named { "markdown-named" } else { "markdown-filename-identity" }.to_string();
    }
    if let Some(mode) = text(&["agent", "defaultMode"]) {
        facts.default_mode = Some(mode);
    }
    facts
}

#[test]
fn the_adapter_anchors_agree_with_the_built_in_targets() {
    // The plugin reads each target's paths and settings from the anchor a
    // project configures, and falls back to the built-in tables in
    // adapters.ts without one. The two must say the same thing, or a project
    // would build differently depending on whether its configuration could be
    // read.
    let adapters = builtin_adapters();
    assert_eq!(adapters.len(), 4, "{adapters:#?}");
    let names: Vec<&str> = adapters.iter().map(|a| a["exportName"].as_str()).collect();

    let sandbox = Sandbox::new();
    let entry = sandbox.dir.join("main.pi");
    std::fs::write(&entry, format!("from @piton/belay import {}\n", names.join(", "))).expect("write");
    let mut project = Project::for_file(&entry);
    project.source_root = sandbox.dir.clone();
    project.root = sandbox.dir.clone();
    let bundled = Compilation::build(project);

    for adapter in &adapters {
        let anchor = find(&bundled, &adapter["exportName"]);
        let properties = &bundled.store().anchor(anchor).properties;
        let builtin = builtin_facts(adapter);
        let read = anchor_facts(builtin_facts(adapter), properties);
        assert_eq!(read, builtin, "{}", adapter["targetId"]);
        let target_id = properties.get("targetId").and_then(|v| match v {
            piton_core::Value::Str(t) => t.as_plain().map(str::to_string),
            _ => None,
        });
        assert_eq!(target_id.as_deref(), Some(adapter["targetId"].as_str()));
    }
}
