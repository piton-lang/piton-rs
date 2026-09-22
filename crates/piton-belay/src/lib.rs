//! The Belay framework compiler.
//!
//! Belay resolves a project through the Piton compiler, builds a complete output
//! plan for every configured adapter, validates that plan, and only then writes.
//! Planning before writing is what lets it catch collisions, broken links, and
//! cross-target discovery problems while they are still cheap to report.

pub mod adapter;
pub mod construct;
pub mod render;
pub mod shape;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_compile::{module, prelude, reach, BelayConfig, Compilation};
use piton_core::{
    is_valid_artifact_name, kebab_case, AnchorId, Diagnostic, MixedItem, Span, Value,
};
use piton_emit::markdown;

use adapter::{Adapter, AgentFormat, CommandSupport};
use construct::{Construct, ConstructKind};
use render::{FieldValue, Fields};

/// What produced an output file, used for diagnostics and for cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputKind {
    Instruction,
    Skill,
    Command,
    CommandPolicy,
    Agent,
    Reference,
    ShapeReference,
}

impl OutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OutputKind::Instruction => "instruction",
            OutputKind::Skill => "skill",
            OutputKind::Command => "command",
            OutputKind::CommandPolicy => "command policy",
            OutputKind::Agent => "agent",
            OutputKind::Reference => "reference",
            OutputKind::ShapeReference => "shape reference",
        }
    }
}

/// One planned file.
#[derive(Debug, Clone)]
pub struct OutputFile {
    /// Path relative to the project root.
    pub path: PathBuf,
    pub contents: String,
    pub kind: OutputKind,
    pub target: &'static str,
    /// The source anchor this came from, named in diagnostics.
    pub origin: String,
    /// The `.pi` files that contributed to this output, for provenance.
    pub sources: Vec<PathBuf>,
}

/// A complete, validated output plan.
#[derive(Debug, Default)]
pub struct Plan {
    pub files: Vec<OutputFile>,
    pub diagnostics: Vec<Diagnostic>,
}

impl Plan {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// The paths this plan owns, sorted, for the build manifest.
    pub fn manifest(&self) -> Vec<String> {
        let mut paths: Vec<String> = self
            .files
            .iter()
            .map(|file| file.path.to_string_lossy().replace('\\', "/"))
            .collect();
        paths.sort();
        paths.dedup();
        paths
    }
}

/// Builds the output plan for a compiled project.
pub fn plan(compilation: &Compilation, config: &BelayConfig) -> Plan {
    let mut plan = Plan::default();

    let adapters: Vec<&'static Adapter> = config
        .adapters
        .iter()
        .filter_map(|target| match Adapter::by_target(target) {
            Some(adapter) => Some(adapter),
            None => {
                plan.diagnostics.push(
                    Diagnostic::error(
                        "unknown-adapter",
                        format!("`{target}` is not a known Belay adapter"),
                        compilation
                            .project
                            .config_path
                            .clone()
                            .unwrap_or_else(|| compilation.project.root.clone()),
                        Span::default(),
                    )
                    .with_help(format!(
                        "available adapters: {}",
                        Adapter::all()
                            .iter()
                            .map(|a| a.target_id)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                );
                None
            }
        })
        .collect();

    if adapters.is_empty() {
        return plan;
    }

    let reachability = reach::from_entry(compilation);
    let mut reachable: Vec<AnchorId> = reachability.reached.iter().map(|r| r.anchor).collect();
    reachable.sort();

    let constructs = construct::collect(compilation, &reachable);
    let references = referenced_anchors(compilation, &reachable);

    for adapter in &adapters {
        build_target(compilation, config, adapter, &constructs, &references, &mut plan);
    }

    validate(compilation, &adapters, &mut plan);
    plan.files.sort_by(|a, b| a.path.cmp(&b.path).then(a.target.cmp(b.target)));
    plan
}

/// Every anchor reached through a reference, transitively, so the reference
/// tree is closed: a link never points at a file that was not generated.
fn referenced_anchors(compilation: &Compilation, reachable: &[AnchorId]) -> BTreeSet<AnchorId> {
    let mut found = BTreeSet::new();
    let mut queue: Vec<AnchorId> = Vec::new();

    for anchor in reachable {
        let def = compilation.store().anchor(*anchor);
        for value in def.properties.values() {
            collect_references(value, &mut queue);
        }
    }

    while let Some(anchor) = queue.pop() {
        if !found.insert(anchor) {
            continue;
        }
        let def = compilation.store().anchor(anchor);
        for value in def.properties.values() {
            collect_references(value, &mut queue);
        }
    }
    found
}

fn collect_references(value: &Value, out: &mut Vec<AnchorId>) {
    match value {
        Value::Reference(id) => out.push(*id),
        Value::Str(text) => out.extend(text.references()),
        Value::List(items) => items.iter().for_each(|item| collect_references(item, out)),
        Value::Dict(map) => map.values().for_each(|item| collect_references(item, out)),
        Value::Mixed(mixed) => {
            for item in &mixed.items {
                match item {
                    MixedItem::Text(text) => out.extend(text.references()),
                    MixedItem::List(items) => {
                        items.iter().for_each(|item| collect_references(item, out))
                    }
                    MixedItem::Entry(_, value) => collect_references(value, out),
                }
            }
        }
        // An anchor embedded by value is inlined, but its own references still
        // have to resolve from wherever it is inlined.
        Value::Anchor(_) => {}
        _ => {}
    }
}

/// Maps anchors to their compiled location for one target.
struct Locations {
    reference: HashMap<AnchorId, PathBuf>,
}

impl Locations {
    fn get(&self, anchor: AnchorId) -> Option<&PathBuf> {
        self.reference.get(&anchor)
    }
}

/// Resolves links relative to the file currently being written.
struct Links<'a> {
    from_directory: PathBuf,
    locations: &'a Locations,
}

impl markdown::LinkResolver for Links<'_> {
    fn link(&self, anchor: AnchorId) -> Option<String> {
        let target = self.locations.get(anchor)?;
        Some(piton_emit::relative_link(&self.from_directory, target))
    }
}

fn build_target(
    compilation: &Compilation,
    config: &BelayConfig,
    adapter: &'static Adapter,
    constructs: &[Construct],
    references: &BTreeSet<AnchorId>,
    plan: &mut Plan,
) {
    let project_root = &compilation.project.root;
    let source_root = &compilation.project.source_root;
    let first_file = plan.files.len();

    // Reference destinations are needed before anything renders, because a
    // reference link has to point at a planned output.
    let mut locations = Locations {
        reference: HashMap::new(),
    };

    // A shape instruction is preserved in the compiled shape tree whether or
    // not anything references it: the shape tree is a view of the architecture,
    // not just a link target. The scoped guidance file is a separate output.
    let shape_instructions: BTreeSet<AnchorId> = constructs
        .iter()
        .filter(|item| item.kind == ConstructKind::Instruction)
        .map(|item| item.anchor)
        .filter(|anchor| {
            config.shape_root.as_ref().is_some_and(|root| {
                shape::is_shape_source(compilation.anchor_module_path(*anchor).as_path(), root)
            })
        })
        .collect();

    let documented: BTreeSet<AnchorId> = references
        .union(&shape_instructions)
        .copied()
        .collect();

    for anchor in &documented {
        let def = compilation.store().anchor(*anchor);
        let source = compilation.graph().get(def.module).path.clone();
        if source.to_string_lossy().starts_with('@') {
            // Bundled package anchors have no source tree to mirror.
            continue;
        }
        let directory = source.parent().unwrap_or(Path::new(""));
        let under_shape = config
            .shape_root
            .as_ref()
            .is_some_and(|root| shape::is_shape_source(&source, root));
        // A shape document keeps its position relative to shapeRoot, so the
        // compiled shape tree mirrors the architecture rather than repeating the
        // directory that held it.
        let (base, relative) = if under_shape {
            let shape_root = config.shape_root.as_ref().expect("checked above");
            (
                PathBuf::from(adapter.shape_root()),
                directory.strip_prefix(shape_root).unwrap_or(directory),
            )
        } else {
            (
                PathBuf::from(adapter.reference_root),
                directory.strip_prefix(source_root).unwrap_or(directory),
            )
        };
        let path = module::normalize(&base.join(relative).join(format!("{}.md", def.name)));
        locations.reference.insert(*anchor, path);
    }

    // Reference and shape documents.
    for anchor in &documented {
        let Some(path) = locations.get(*anchor).cloned() else {
            continue;
        };
        let def = compilation.store().anchor(*anchor);
        let links = Links {
            from_directory: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            locations: &locations,
        };
        let context = markdown::Context {
            anchors: compilation,
            links: &links,
        };
        let contents = markdown::document(*anchor, &context);
        let kind = if config
            .shape_root
            .as_ref()
            .is_some_and(|root| shape::is_shape_source(compilation.anchor_module_path(*anchor).as_path(), root))
        {
            OutputKind::ShapeReference
        } else {
            OutputKind::Reference
        };
        plan.files.push(OutputFile {
            contents,
            path,
            kind,
            target: adapter.target_id,
            origin: def.name.clone(),
            sources: vec![compilation.anchor_module_path(*anchor)],
        });
    }

    // Constructs.
    let mut instructions: BTreeMap<PathBuf, Vec<(String, String, PathBuf)>> = BTreeMap::new();
    for item in constructs {
        match item.kind {
            ConstructKind::Skill => {
                render_skill(compilation, adapter, item, &locations, plan)
            }
            ConstructKind::Command => {
                render_command(compilation, adapter, item, &locations, plan)
            }
            ConstructKind::Agent => render_agent(compilation, adapter, item, &locations, plan),
            ConstructKind::Instruction => {
                collect_instruction(compilation, config, adapter, item, &locations, plan, &mut instructions)
            }
        }
    }

    // Guidance for one scope is combined into a single file per target, in a
    // stable order so identical input produces identical bytes.
    for (path, sections) in instructions {
        let mut contents = String::new();
        for (index, (title, body, _)) in sections.iter().enumerate() {
            if index > 0 {
                contents.push_str("\n");
            }
            contents.push_str(&format!("# {title}\n\n{}\n", body.trim_end()));
        }
        let origin = sections
            .iter()
            .map(|(title, _, _)| title.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let sources = sections.iter().map(|(_, _, source)| source.clone()).collect();
        plan.files.push(OutputFile {
            contents,
            path,
            kind: OutputKind::Instruction,
            target: adapter.target_id,
            origin,
            sources,
        });
    }

    let _ = project_root;

    // Every file this target planned, whatever produced it, gets its location
    // markers resolved. Doing it here rather than at each construction is what
    // keeps a skill and a reference document agreeing about where `.claude` is.
    let locations = target_locations(compilation, config, adapter);
    for file in &mut plan.files[first_file..] {
        file.contents = resolve_location_markers(&file.contents, &locations, &file.path);
    }
}

/// The directory each location export names, relative to the project root.
///
/// `shapeRoot` and `codeRoot` are configured paths, so they are stored absolute
/// and made project-relative here. Both fall back to the project root, which is
/// what the specification says an unconfigured root resolves to.
fn target_locations(
    compilation: &Compilation,
    config: &BelayConfig,
    adapter: &Adapter,
) -> Vec<(&'static str, PathBuf)> {
    let project_root = &compilation.project.root;
    let under_project = |path: &Path| -> PathBuf {
        path.strip_prefix(project_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf())
    };

    vec![
        (
            prelude::BELAY_SHAPE_TOKEN,
            PathBuf::from(adapter.shape_root()),
        ),
        (prelude::BELAY_AGENT_ROOT_TOKEN, PathBuf::from(adapter.root)),
        (prelude::BELAY_PROJECT_ROOT_TOKEN, PathBuf::new()),
        (
            prelude::BELAY_SHAPE_ROOT_TOKEN,
            config
                .shape_root
                .as_deref()
                .map(under_project)
                .unwrap_or_default(),
        ),
        (
            prelude::BELAY_CODE_ROOT_TOKEN,
            under_project(&config.code_root),
        ),
    ]
}

/// Rewrites each location marker as a path from the file that carries it to the
/// directory it names.
fn resolve_location_markers(
    contents: &str,
    locations: &[(&'static str, PathBuf)],
    path: &Path,
) -> String {
    if !contents.contains('\u{e000}') {
        return contents.to_string();
    }
    let from = path.parent().unwrap_or(Path::new(""));
    let mut out = contents.to_string();
    for (marker, target) in locations {
        if !out.contains(marker) {
            continue;
        }
        out = out.replace(marker, &piton_emit::relative_link(from, target));
    }
    out
}

fn context_for<'a>(
    compilation: &'a Compilation,
    locations: &'a Locations,
    path: &Path,
    links: &'a mut Option<Links<'a>>,
) -> markdown::Context<'a> {
    *links = Some(Links {
        from_directory: path.parent().unwrap_or(Path::new("")).to_path_buf(),
        locations,
    });
    markdown::Context {
        anchors: compilation,
        links: links.as_ref().expect("just set"),
    }
}

fn render_skill(
    compilation: &Compilation,
    adapter: &'static Adapter,
    item: &Construct,
    locations: &Locations,
    plan: &mut Plan,
) {
    let name = kebab_case(&item.name);
    let path = PathBuf::from(format!("{}/{name}/SKILL.md", adapter.skill_root));
    let mut holder = None;
    let context = context_for(compilation, locations, &path, &mut holder);

    let description = item.metadata_text("description", compilation).unwrap_or_default();
    let use_when = item.metadata_text("useWhen", compilation).unwrap_or_default();
    let prompt = item.text("prompt", &context).unwrap_or_default();

    let mut fields = Fields::new();
    fields.insert("name".into(), FieldValue::Text(name.clone()));
    fields.insert(
        "description".into(),
        FieldValue::Text(render::discovery_description(&description, &use_when)),
    );
    let consumed = native_options(compilation, item, adapter.skill_options, &mut fields, plan);

    let remainder = render::remainder(&item.remainder(&consumed), &context);
    let contents = format!(
        "{}\n{}",
        render::frontmatter(&fields),
        render::body(&prompt, &remainder)
    );

    plan.files.push(OutputFile {
        contents,
        path,
        kind: OutputKind::Skill,
        target: adapter.target_id,
        origin: item.name.clone(),
        sources: vec![compilation.anchor_module_path(item.anchor)],
    });
}

fn render_command(
    compilation: &Compilation,
    adapter: &'static Adapter,
    item: &Construct,
    locations: &Locations,
    plan: &mut Plan,
) {
    // The `x-` prefix distinguishes a command from a skill wherever the two
    // share a namespace, and it survives target-name normalization.
    let name = format!("x-{}", kebab_case(&item.name));
    let description = item
        .metadata_text("description", compilation)
        .unwrap_or_default();

    let path = match adapter.command_support {
        CommandSupport::Native => {
            PathBuf::from(format!("{}/{name}.md", adapter.command_root))
        }
        _ => PathBuf::from(format!("{}/{name}/SKILL.md", adapter.command_root)),
    };
    let mut holder = None;
    let context = context_for(compilation, locations, &path, &mut holder);
    let prompt = item.text("prompt", &context).unwrap_or_default();

    let mut fields = Fields::new();
    match adapter.command_support {
        CommandSupport::TranslatedSkill => {
            fields.insert("name".into(), FieldValue::Text(name.clone()));
            fields.insert("description".into(), FieldValue::Text(description.clone()));
            // A command is invoked deliberately; turning it into an
            // automatically selected skill would change what it means.
            fields.insert("disable-model-invocation".into(), FieldValue::Bool(true));
        }
        CommandSupport::TranslatedSkillWithPolicy => {
            fields.insert("name".into(), FieldValue::Text(name.clone()));
            fields.insert("description".into(), FieldValue::Text(description.clone()));
        }
        CommandSupport::Native => {
            // Identity comes from the filename here, so it is not repeated.
            fields.insert("description".into(), FieldValue::Text(description.clone()));
        }
    }
    let consumed = native_options(compilation, item, adapter.command_options, &mut fields, plan);

    let remainder = render::remainder(&item.remainder(&consumed), &context);
    let contents = format!(
        "{}\n{}",
        render::frontmatter(&fields),
        render::body(&prompt, &remainder)
    );

    plan.files.push(OutputFile {
        contents,
        path: path.clone(),
        kind: OutputKind::Command,
        target: adapter.target_id,
        origin: item.name.clone(),
        sources: vec![compilation.anchor_module_path(item.anchor)],
    });

    if adapter.command_support == CommandSupport::TranslatedSkillWithPolicy {
        let policy_path =
            PathBuf::from(format!("{}/{name}/agents/openai.yaml", adapter.command_root));
        let mut policy = Fields::new();
        let mut inner = indexmap::IndexMap::new();
        inner.insert(
            "allow_implicit_invocation".to_string(),
            FieldValue::Bool(false),
        );
        policy.insert("policy".into(), FieldValue::Map(inner));
        let mut contents = String::new();
        render_yaml_document(&policy, &mut contents);
        plan.files.push(OutputFile {
            path: policy_path,
            contents,
            kind: OutputKind::CommandPolicy,
            target: adapter.target_id,
            origin: item.name.clone(),
            sources: vec![compilation.anchor_module_path(item.anchor)],
        });
    }
}

fn render_yaml_document(fields: &Fields, out: &mut String) {
    let block = render::frontmatter(fields);
    let body = block
        .trim_start_matches("---\n")
        .trim_end_matches("---\n");
    out.push_str(body);
}

fn render_agent(
    compilation: &Compilation,
    adapter: &'static Adapter,
    item: &Construct,
    locations: &Locations,
    plan: &mut Plan,
) {
    let name = kebab_case(&item.name);
    let extension = if adapter.agent_format == AgentFormat::Toml {
        "toml"
    } else {
        "md"
    };
    let path = PathBuf::from(format!("{}/{name}.{extension}", adapter.agent_root));
    let mut holder = None;
    let context = context_for(compilation, locations, &path, &mut holder);

    let description = item
        .metadata_text("description", compilation)
        .unwrap_or_default();
    let role = item.metadata_text("role", compilation).unwrap_or_default();
    let prompt = item.text("prompt", &context).unwrap_or_default();

    let mut fields = Fields::new();
    match adapter.agent_format {
        AgentFormat::MarkdownNamed => {
            fields.insert("name".into(), FieldValue::Text(name.clone()));
            fields.insert("description".into(), FieldValue::Text(description.clone()));
        }
        AgentFormat::MarkdownFilenameIdentity => {
            fields.insert("description".into(), FieldValue::Text(description.clone()));
            if !item.properties.contains_key("mode") {
                // Activation has to be explicit; a missing mode would leave the
                // platform to guess whether this is a primary agent.
                fields.insert("mode".into(), FieldValue::Text("subagent".into()));
            }
        }
        AgentFormat::Toml => {
            fields.insert("name".into(), FieldValue::Text(name.clone()));
            fields.insert("description".into(), FieldValue::Text(description.clone()));
        }
    }
    let consumed = native_options(compilation, item, adapter.agent_options, &mut fields, plan);

    let introduction = render::role_introduction(&role);
    let primary = format!("{introduction}\n\n{}", prompt.trim());
    let remainder = render::remainder(&item.remainder(&consumed), &context);
    let assembled = render::body(&primary, &remainder);

    let contents = match adapter.agent_format {
        AgentFormat::Toml => {
            // The whole instruction body becomes one TOML string field.
            fields.insert(
                "developer_instructions".into(),
                FieldValue::Text(assembled.trim_end().to_string()),
            );
            render::toml_document(&fields)
        }
        _ => format!("{}\n{assembled}", render::frontmatter(&fields)),
    };

    plan.files.push(OutputFile {
        contents,
        path,
        kind: OutputKind::Agent,
        target: adapter.target_id,
        origin: item.name.clone(),
        sources: vec![compilation.anchor_module_path(item.anchor)],
    });
}

#[allow(clippy::too_many_arguments)]
fn collect_instruction(
    compilation: &Compilation,
    config: &BelayConfig,
    adapter: &'static Adapter,
    item: &Construct,
    locations: &Locations,
    plan: &mut Plan,
    out: &mut BTreeMap<PathBuf, Vec<(String, String, PathBuf)>>,
) {
    let source = compilation.anchor_module_path(item.anchor);
    let project_root = &compilation.project.root;

    let scope = match config.shape_root.as_ref() {
        Some(shape_root) if shape::is_shape_source(&source, shape_root) => {
            let exists = |path: &Path| path.is_dir();
            match shape::place(&source, shape_root, &config.code_root, &exists) {
                Some(placement) => placement.scope,
                None => {
                    plan.diagnostics.push(
                        Diagnostic::error(
                            "unplaceable-instruction",
                            format!(
                                "`{}` has no scope to attach to; `{}` does not exist",
                                item.name,
                                config.code_root.display()
                            ),
                            source.clone(),
                            Span::default(),
                        )
                        .with_origin(item.name.clone()),
                    );
                    return;
                }
            }
        }
        _ => {
            // Placement for instructions outside shapeRoot is an open question
            // in the specification. Attaching them to the code root is the
            // conservative reading; say so rather than deciding silently.
            plan.diagnostics.push(
                Diagnostic::warning(
                    "instruction-outside-shape",
                    format!(
                        "`{}` is not under a configured shapeRoot; attaching it to the code root",
                        item.name
                    ),
                    source.clone(),
                    Span::default(),
                )
                .with_origin(item.name.clone()),
            );
            config.code_root.clone()
        }
    };

    let relative_scope = scope
        .strip_prefix(project_root)
        .unwrap_or(&scope)
        .to_path_buf();
    let path = module::normalize(&relative_scope.join(adapter.instruction_file));

    let mut holder = None;
    let context = context_for(compilation, locations, &path, &mut holder);
    let prompt = item.text("prompt", &context).unwrap_or_default();
    let remainder = render::remainder(&item.remainder(&[]), &context);
    let body = render::body(&prompt, &remainder);

    out.entry(path)
        .or_default()
        .push((piton_core::title_case(&item.name), body, source));
}

/// Copies explicitly configured native options into the target's metadata, and
/// reports the ones this target cannot represent.
fn native_options(
    compilation: &Compilation,
    item: &Construct,
    supported: &[&str],
    fields: &mut Fields,
    plan: &mut Plan,
) -> Vec<&'static str> {
    // Names a target might carry natively, across all targets. Anything in this
    // set is metadata rather than prose, so it never falls into the remainder.
    const NATIVE: &[&str] = &[
        "tools",
        "model",
        "allowed-tools",
        "allowedTools",
        "mode",
        "permission",
        "agent",
        "model_reasoning_effort",
        "sandbox_mode",
    ];

    let mut consumed: Vec<&'static str> = Vec::new();
    for (name, value) in &item.properties {
        let Some(canonical) = NATIVE.iter().find(|candidate| *candidate == name) else {
            continue;
        };
        consumed.push(canonical);
        let normalized = if *canonical == "allowedTools" {
            "allowed-tools"
        } else {
            canonical
        };
        if !supported.contains(&normalized) {
            // A prompt that asks for restraint is not an enforced restriction;
            // claiming otherwise would be worse than refusing to emit it.
            plan.diagnostics.push(
                Diagnostic::warning(
                    "unsupported-option",
                    format!(
                        "`{}` is not a native option for this target and will not be enforced",
                        name
                    ),
                    compilation.anchor_module_path(item.anchor),
                    Span::default(),
                )
                .with_origin(format!("{}.{name}", item.name)),
            );
            continue;
        }
        match FieldValue::from_value(value, compilation) {
            Some(field) => {
                fields.insert(normalized.to_string(), field);
            }
            None => plan.diagnostics.push(
                Diagnostic::error(
                    "invalid-option",
                    format!("`{name}` cannot be represented as target metadata"),
                    compilation.anchor_module_path(item.anchor),
                    Span::default(),
                )
                .with_origin(format!("{}.{name}", item.name)),
            ),
        }
    }
    consumed
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

fn validate(compilation: &Compilation, adapters: &[&'static Adapter], plan: &mut Plan) {
    let config_path = compilation
        .project
        .config_path
        .clone()
        .unwrap_or_else(|| compilation.project.root.clone());

    // Two artifacts at one path are only acceptable when they are identical.
    let mut by_path: HashMap<PathBuf, Vec<usize>> = HashMap::new();
    for (index, file) in plan.files.iter().enumerate() {
        by_path.entry(file.path.clone()).or_default().push(index);
    }
    let mut collisions: Vec<(PathBuf, String)> = Vec::new();
    for (path, indices) in &by_path {
        if indices.len() < 2 {
            continue;
        }
        let first = &plan.files[indices[0]];
        let identical = indices
            .iter()
            .all(|index| plan.files[*index].contents == first.contents);
        if !identical {
            let origins: Vec<String> = indices
                .iter()
                .map(|index| {
                    format!(
                        "{} ({})",
                        plan.files[*index].origin, plan.files[*index].target
                    )
                })
                .collect();
            collisions.push((path.clone(), origins.join(", ")));
        }
    }
    collisions.sort();
    for (path, origins) in collisions {
        plan.diagnostics.push(
            Diagnostic::error(
                "output-collision",
                format!(
                    "`{}` would be written with different content by: {origins}",
                    path.display()
                ),
                config_path.clone(),
                Span::default(),
            )
            .with_help(
                "shared output is only coalesced when the artifacts are identical in content, reference resolution, and activation behaviour"
                    .to_string(),
            ),
        );
    }
    // Identical shared guidance is written once.
    let mut seen_paths = HashSet::new();
    plan.files.retain(|file| seen_paths.insert(file.path.clone()));

    // Generated names must satisfy the identity rules every target shares.
    for file in &plan.files {
        let Some(name) = artifact_name(file) else {
            continue;
        };
        if !is_valid_artifact_name(&name) {
            plan.diagnostics.push(
                Diagnostic::error(
                    "invalid-artifact-name",
                    format!(
                        "`{name}` is not a valid {} name; use 1 to 64 lowercase alphanumerics with single hyphen separators",
                        file.kind.as_str()
                    ),
                    config_path.clone(),
                    Span::default(),
                )
                .with_origin(file.origin.clone()),
            );
        }
        if file.kind == OutputKind::Command && !name.starts_with("x-") {
            plan.diagnostics.push(Diagnostic::error(
                "missing-command-prefix",
                format!("command `{name}` lost its `x-` prefix during normalization"),
                config_path.clone(),
                Span::default(),
            ));
        }
    }

    // Every relative link must point at a file this plan will write.
    let planned: HashSet<String> = plan
        .files
        .iter()
        .map(|file| file.path.to_string_lossy().replace('\\', "/"))
        .collect();
    let mut broken: Vec<(String, String, String)> = Vec::new();
    for file in &plan.files {
        let directory = file.path.parent().unwrap_or(Path::new(""));
        for target in markdown_links(&file.contents) {
            if target.starts_with("http://") || target.starts_with("https://") {
                continue;
            }
            let resolved = module::normalize(&directory.join(&target));
            let key = resolved.to_string_lossy().replace('\\', "/");
            if !planned.contains(&key) {
                broken.push((
                    file.path.to_string_lossy().to_string(),
                    target,
                    file.origin.clone(),
                ));
            }
        }
    }
    broken.sort();
    broken.dedup();
    for (file, target, origin) in broken {
        plan.diagnostics.push(
            Diagnostic::error(
                "broken-reference-link",
                format!("`{file}` links to `{target}`, which is not a generated file"),
                config_path.clone(),
                Span::default(),
            )
            .with_origin(origin),
        );
    }

    // Descriptions have to fit the discovery metadata every target accepts.
    for file in &plan.files {
        if !matches!(file.kind, OutputKind::Skill | OutputKind::Command) {
            continue;
        }
        if let Some(description) = frontmatter_field(&file.contents, "description") {
            if description.chars().count() > 1024 {
                plan.diagnostics.push(
                    Diagnostic::error(
                        "description-too-long",
                        format!(
                            "`{}` has a {}-character description; the supported range is 1 to 1024",
                            file.path.display(),
                            description.chars().count()
                        ),
                        config_path.clone(),
                        Span::default(),
                    )
                    .with_origin(file.origin.clone()),
                );
            }
        }
    }

    // File separation alone does not guarantee target isolation: OpenCode also
    // discovers the other adapters' skill directories.
    if adapters.iter().any(|a| a.discovers_foreign_skills) && adapters.len() > 1 {
        let mut identities: HashMap<String, BTreeSet<&'static str>> = HashMap::new();
        for file in &plan.files {
            if !matches!(file.kind, OutputKind::Skill | OutputKind::Command) {
                continue;
            }
            if let Some(name) = artifact_name(file) {
                identities.entry(name).or_default().insert(file.target);
            }
        }
        let mut duplicates: Vec<(String, Vec<&str>)> = identities
            .into_iter()
            .filter(|(_, targets)| targets.len() > 1)
            .map(|(name, targets)| (name, targets.into_iter().collect()))
            .collect();
        duplicates.sort();
        for (name, targets) in duplicates {
            plan.diagnostics.push(
                Diagnostic::warning(
                    "cross-target-discovery",
                    format!(
                        "skill `{name}` is generated for {}, and OpenCode discovers the other adapters' skill directories",
                        targets.join(" and ")
                    ),
                    config_path.clone(),
                    Span::default(),
                )
                .with_help(
                    "choose one deployment target, or accept that the same skill will be offered more than once"
                        .to_string(),
                ),
            );
        }
    }

    // Nothing may escape the project root.
    for file in &plan.files {
        if file.path.is_absolute() || file.path.components().any(|c| c.as_os_str() == "..") {
            plan.diagnostics.push(Diagnostic::error(
                "output-outside-project",
                format!("`{}` resolves outside the project root", file.path.display()),
                config_path.clone(),
                Span::default(),
            ));
        }
    }
}

/// The identity a skill, command, or agent file carries.
fn artifact_name(file: &OutputFile) -> Option<String> {
    match file.kind {
        OutputKind::Skill | OutputKind::Command => {
            let path = file.path.to_string_lossy();
            if path.ends_with("/SKILL.md") {
                file.path
                    .parent()?
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
            } else {
                file.path
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
            }
        }
        OutputKind::Agent => file
            .path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string()),
        _ => None,
    }
}

/// Extracts a scalar field from a YAML frontmatter block.
fn frontmatter_field(contents: &str, field: &str) -> Option<String> {
    let rest = contents.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    for line in rest[..end].lines() {
        if let Some(value) = line.strip_prefix(&format!("{field}: ")) {
            let value = value.trim();
            let unquoted = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            return Some(unquoted.to_string());
        }
    }
    None
}

/// Finds the targets of inline Markdown links.
fn markdown_links(contents: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes: Vec<char> = contents.chars().collect();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == ']' && bytes.get(i + 1) == Some(&'(') {
            let mut j = i + 2;
            let mut target = String::new();
            while j < bytes.len() && bytes[j] != ')' && bytes[j] != '\n' {
                target.push(bytes[j]);
                j += 1;
            }
            if j < bytes.len() && bytes[j] == ')' && !target.is_empty() {
                out.push(target);
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Writes a plan to disk, returning the paths written.
pub fn write(plan: &Plan, root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut written = Vec::new();
    for file in &plan.files {
        let destination = root.join(&file.path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Skip rewriting identical content so timestamps stay meaningful.
        let unchanged = std::fs::read_to_string(&destination)
            .map(|existing| existing == file.contents)
            .unwrap_or(false);
        if !unchanged {
            std::fs::write(&destination, &file.contents)?;
        }
        written.push(file.path.clone());
    }
    Ok(written)
}

/// Removes files a previous build generated that this one no longer produces.
///
/// Only paths recorded in the previous manifest are considered, so cleanup can
/// never delete a file Belay did not write.
pub fn clean_stale(previous: &[String], plan: &Plan, root: &Path) -> Vec<PathBuf> {
    let current: HashSet<String> = plan.manifest().into_iter().collect();
    let mut removed = Vec::new();
    for path in previous {
        if current.contains(path) {
            continue;
        }
        let target = root.join(path);
        if target.is_file() && std::fs::remove_file(&target).is_ok() {
            removed.push(PathBuf::from(path));
        }
    }
    removed
}

/// A build manifest, so a later build knows what it owns.
pub fn manifest_document(plan: &Plan) -> String {
    let mut out = String::from("{\n  \"generated\": [\n");
    let paths = plan.manifest();
    for (index, path) in paths.iter().enumerate() {
        out.push_str("    ");
        out.push_str(&piton_emit::json::quote(path));
        if index + 1 < paths.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]\n}\n");
    out
}

/// Reads the paths recorded in a manifest document.
pub fn manifest_paths(document: &str) -> Vec<String> {
    document
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim().trim_end_matches(',');
            let unquoted = trimmed.strip_prefix('"')?.strip_suffix('"')?;
            Some(unquoted.to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_links_are_found() {
        let links = markdown_links("see [A](a/b.md) and [B](../c.md) plus [C](https://x)");
        assert_eq!(links, vec!["a/b.md", "../c.md", "https://x"]);
    }

    #[test]
    fn frontmatter_fields_are_readable() {
        let contents = "---\nname: x-thing\ndescription: \"a: b\"\n---\n\nbody";
        assert_eq!(
            frontmatter_field(contents, "description"),
            Some("a: b".to_string())
        );
        assert_eq!(
            frontmatter_field(contents, "name"),
            Some("x-thing".to_string())
        );
    }

    #[test]
    fn manifests_round_trip() {
        let plan = Plan {
            files: vec![
                OutputFile {
                    path: PathBuf::from("b.md"),
                    contents: String::new(),
                    kind: OutputKind::Reference,
                    target: "claude-code",
                    origin: "B".into(),
                    sources: Vec::new(),
                },
                OutputFile {
                    path: PathBuf::from("a.md"),
                    contents: String::new(),
                    kind: OutputKind::Reference,
                    target: "claude-code",
                    origin: "A".into(),
                    sources: Vec::new(),
                },
            ],
            diagnostics: Vec::new(),
        };
        let document = manifest_document(&plan);
        assert_eq!(manifest_paths(&document), vec!["a.md", "b.md"]);
    }

    #[test]
    fn artifact_names_come_from_the_path() {
        let skill = OutputFile {
            path: PathBuf::from(".claude/skills/build-tooling/SKILL.md"),
            contents: String::new(),
            kind: OutputKind::Skill,
            target: "claude-code",
            origin: "BuildTooling".into(),
            sources: Vec::new(),
        };
        assert_eq!(artifact_name(&skill).as_deref(), Some("build-tooling"));

        let command = OutputFile {
            path: PathBuf::from(".opencode/commands/x-release.md"),
            contents: String::new(),
            kind: OutputKind::Command,
            target: "opencode",
            origin: "Release".into(),
            sources: Vec::new(),
        };
        assert_eq!(artifact_name(&command).as_deref(), Some("x-release"));
    }
}
