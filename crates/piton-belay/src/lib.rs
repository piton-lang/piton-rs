//! The Belay framework compiler.
//!
//! Belay resolves a project through the Piton compiler, builds a complete output
//! plan for every configured adapter, validates that plan, and only then writes.
//! Planning before writing is what lets it catch collisions, broken links,
//! unowned files and cross-target discovery problems while they are still cheap
//! to report.
//!
//! What is emitted is what the build command emits: the entry file's exports
//! and anything they reference. An exported construct becomes its target's
//! native artifact; any other exported anchor, and every anchor an emitted
//! artifact references, is compiled into the target's reference tree, one file
//! per source module.

pub mod adapter;
pub mod construct;
pub mod manifest;
pub mod options;
pub mod references;
pub mod render;
pub mod shape;
mod validate;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use piton_compile::{module, prelude, BelayConfig, Compilation};
use piton_core::{kebab_case, title_case, AnchorId, Diagnostic, Ref, Span, Value};
use piton_emit::markdown;

use adapter::{AgentFormat, CommandSupport, NativeOption, OptionRule, Target};
use construct::{Construct, ConstructKind};
use options::Options;
use references::{Closure, FileIndex, Links, Unrepresentable};
use render::{FieldValue, Fields};

pub use manifest::MANIFEST;

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
    /// The source anchor this came from, named in diagnostics. A file holding
    /// several anchors -- a module's reference document, a scope's combined
    /// guidance -- names them all, comma separated.
    pub origin: String,
    /// The `.pi` files that contributed to this output, for provenance.
    pub sources: Vec<PathBuf>,
}

/// A complete, validated output plan.
#[derive(Debug, Default)]
pub struct Plan {
    pub files: Vec<OutputFile>,
    pub diagnostics: Vec<Diagnostic>,
    /// For each target, the date its documentation was last checked: the
    /// version the generated files were validated against. The manifest
    /// records it.
    pub targets: Vec<(String, String)>,
}

/// A planned file with the anchors whose content it carries, in order, so a
/// diagnostic about the file can point at its source.
#[derive(Debug, Clone)]
pub(crate) struct Planned {
    pub file: OutputFile,
    pub anchors: Vec<AnchorId>,
}

impl std::ops::Deref for Planned {
    type Target = OutputFile;
    fn deref(&self) -> &OutputFile {
        &self.file
    }
}

impl std::ops::DerefMut for Planned {
    fn deref_mut(&mut self) -> &mut OutputFile {
        &mut self.file
    }
}

/// The plan while it is being built.
#[derive(Debug, Default)]
pub(crate) struct Draft {
    pub files: Vec<Planned>,
    pub diagnostics: Vec<Diagnostic>,
    /// The links rendering wrote, each with the directory it is relative
    /// to. Only these are checked against the plan; a link an author wrote,
    /// to an image or a page of their own, is theirs.
    pub generated_links: HashSet<(PathBuf, String)>,
}

impl Draft {
    fn push(&mut self, file: OutputFile, anchors: Vec<AnchorId>) {
        self.files.push(Planned { file, anchors });
    }
}

impl Plan {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(Diagnostic::is_error)
    }

    /// The paths this plan owns, sorted, for the build manifest.
    pub fn manifest(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.files.iter().map(|file| slash(&file.path)).collect();
        paths.sort();
        paths.dedup();
        paths
    }
}

/// Builds the output plan for a compiled project.
pub fn plan(compilation: &Compilation, config: &BelayConfig) -> Plan {
    let options = Options::load(compilation, config);
    plan_with(compilation, config, &options)
}

/// Builds the output plan against already-loaded options.
pub fn plan_with(compilation: &Compilation, config: &BelayConfig, options: &Options) -> Plan {
    let mut plan = Draft::default();
    plan.diagnostics.extend(options.diagnostics.iter().cloned());
    if options.targets.is_empty() {
        return Plan {
            files: Vec::new(),
            diagnostics: plan.diagnostics,
            targets: Vec::new(),
        };
    }

    let emission = Emission::collect(compilation, config, &mut plan);
    let closure = references::close(compilation, &emission.roots());
    report_references(compilation, &emission, &closure, &mut plan);

    for target in &options.targets {
        build_target(compilation, config, target, &emission, &closure, &mut plan);
    }

    validate::validate(compilation, options, &mut plan);
    validate::ownership(compilation, options, &mut plan);
    plan.files
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.target.cmp(b.target)));
    Plan {
        files: plan.files.into_iter().map(|planned| planned.file).collect(),
        diagnostics: plan.diagnostics,
        targets: options
            .targets
            .iter()
            .map(|target| (target.id.to_string(), target.documentation_checked.clone()))
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Diagnostic locations
// ---------------------------------------------------------------------------

/// Where an anchor is declared.
pub(crate) fn anchor_site(compilation: &Compilation, anchor: AnchorId) -> (PathBuf, Span) {
    let def = compilation.store().anchor(anchor);
    (compilation.anchor_module_path(anchor), def.name_span)
}

/// Where the winning declaration of an anchor's property is, falling back to
/// the anchor.
pub(crate) fn property_site(
    compilation: &Compilation,
    anchor: AnchorId,
    key: &str,
) -> (PathBuf, Span) {
    let def = compilation.store().anchor(anchor);
    match def.slots.get(key) {
        Some(slot) if slot.span != Span::default() => {
            (compilation.anchor_module_path(slot.owner), slot.span)
        }
        _ => anchor_site(compilation, anchor),
    }
}

fn error_at(code: &str, message: String, (file, span): (PathBuf, Span)) -> Diagnostic {
    Diagnostic::error(code, message, file, span)
}

fn warning_at(code: &str, message: String, (file, span): (PathBuf, Span)) -> Diagnostic {
    Diagnostic::warning(code, message, file, span)
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

// ---------------------------------------------------------------------------
// What is emitted
// ---------------------------------------------------------------------------

/// The entry's exports, sorted into what they compile to.
struct Emission {
    /// Exported concrete constructs, which become native artifacts.
    constructs: Vec<Construct>,
    /// Exported concrete anchors that are not constructs, which become
    /// reference documents.
    documents: Vec<AnchorId>,
    /// Exported instructions under `shapeRoot`, preserved in the compiled
    /// shape tree as well as placed as scoped guidance.
    shape_instructions: BTreeSet<AnchorId>,
    /// Each placeable instruction's scope directory, relative to the project
    /// root.
    placements: HashMap<AnchorId, PathBuf>,
}

impl Emission {
    fn collect(compilation: &Compilation, config: &BelayConfig, plan: &mut Draft) -> Emission {
        let exports: Vec<AnchorId> = compilation
            .entry_exports()
            .into_iter()
            .filter(|anchor| !compilation.store().anchor(*anchor).is_abstract)
            .collect();
        let constructs = construct::collect(compilation, &exports);
        let construct_ids: BTreeSet<AnchorId> = constructs.iter().map(|c| c.anchor).collect();
        let mut seen = BTreeSet::new();
        let documents = exports
            .iter()
            .copied()
            .filter(|anchor| !construct_ids.contains(anchor))
            .filter(|anchor| seen.insert(*anchor))
            .collect();

        let project_root = &compilation.project.root;
        let mut shape_instructions = BTreeSet::new();
        let mut placements = HashMap::new();
        for item in constructs.iter().filter(|c| c.kind == ConstructKind::Instruction) {
            let source = compilation.anchor_module_path(item.anchor);
            let site = anchor_site(compilation, item.anchor);
            let Some(shape_root) = config
                .shape_root
                .as_ref()
                .filter(|root| shape::is_shape_source(&source, root))
            else {
                // Placement outside shapeRoot is an open decision in the
                // specification; tooling says so rather than guessing.
                plan.diagnostics.push(
                    error_at(
                        "instruction-placement-unspecified",
                        format!(
                            "`{}` is not under the configured shapeRoot, and where such an instruction goes is not specified",
                            item.name
                        ),
                        site,
                    )
                    .with_origin(item.name.clone())
                    .with_help(
                        "move it under shapeRoot (and configure one), or make it a skill; see OpenDecisions.instructionScope"
                            .to_string(),
                    ),
                );
                continue;
            };
            shape_instructions.insert(item.anchor);
            let exists = |path: &Path| path.is_dir();
            match shape::place(&source, shape_root, &config.code_root, &exists) {
                Some(placement) => {
                    let scope = placement
                        .scope
                        .strip_prefix(project_root)
                        .map(Path::to_path_buf)
                        .unwrap_or(placement.scope);
                    placements.insert(item.anchor, scope);
                }
                None => plan.diagnostics.push(
                    error_at(
                        "unplaceable-instruction",
                        format!(
                            "`{}` has no scope to attach to; `{}` does not exist",
                            item.name,
                            config.code_root.display()
                        ),
                        site,
                    )
                    .with_origin(item.name.clone()),
                ),
            }
        }

        Emission {
            constructs,
            documents,
            shape_instructions,
            placements,
        }
    }

    /// The anchors whose content is emitted directly.
    fn roots(&self) -> Vec<AnchorId> {
        let mut roots: Vec<AnchorId> = self.constructs.iter().map(|c| c.anchor).collect();
        roots.extend(self.documents.iter().copied());
        roots
    }

    /// The anchors that get a document in the reference tree, before
    /// references are added.
    fn documented(&self) -> BTreeSet<AnchorId> {
        self.documents
            .iter()
            .chain(self.shape_instructions.iter())
            .copied()
            .collect()
    }

}

/// Reports references that cannot be compiled, references in metadata, and
/// anchors that would compile to more than one output.
fn report_references(
    compilation: &Compilation,
    emission: &Emission,
    closure: &Closure,
    plan: &mut Draft,
) {
    let mut reported: BTreeSet<(AnchorId, AnchorId, String)> = BTreeSet::new();
    for occurrence in &closure.occurrences {
        let target = &occurrence.target;
        let key = (target.anchor, occurrence.from, occurrence.key.clone());
        if !reported.insert(key) {
            continue;
        }
        let site = property_site(compilation, occurrence.from, &occurrence.key);
        let origin = format!(
            "{}.{}",
            compilation.store().anchor(occurrence.from).name,
            occurrence.key
        );
        let shown = target.display(compilation);
        if let Some(reason) = closure.unrepresentable.get(&target.anchor) {
            let why = match reason {
                Unrepresentable::Abstract => {
                    "an abstract anchor, which is never compiled".to_string()
                }
                Unrepresentable::Package => format!(
                    "`{}` from a bundled package, which has no compiled representation in this project",
                    compilation.store().anchor(target.anchor).name
                ),
                Unrepresentable::OutsideProject => {
                    "an anchor outside the project, which no output location covers".to_string()
                }
            };
            plan.diagnostics.push(
                error_at(
                    "unrepresentable-reference",
                    format!("`@{{{shown}}}` points at {why}, so the link would have nowhere to go"),
                    site,
                )
                .with_origin(origin)
                .with_help("reference a concrete anchor of this project, or write the name as text".to_string()),
            );
        }
    }

    // Discovery metadata is plain text: a link there would not be read as one.
    for item in &emission.constructs {
        for key in item.kind.metadata_properties() {
            let Some(Value::Str(text)) = item.properties.get(*key) else {
                continue;
            };
            if text.is_plain() {
                continue;
            }
            plan.diagnostics.push(
                error_at(
                    "unrepresentable-reference",
                    format!(
                        "`{}.{key}` is emitted as target metadata, which has no representation for a reference",
                        item.name
                    ),
                    property_site(compilation, item.anchor, key),
                )
                .with_origin(format!("{}.{key}", item.name))
                .with_help("write the name as plain text, or move the reference into the prompt".to_string()),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Locations
// ---------------------------------------------------------------------------

/// Where one target compiles each anchor, for a tool that has to cite the
/// compiled output instead of the source, like `piton slice --adapter`.
///
/// It is the same layout a build writes: the same files, and fragments that
/// land on the same headings.
pub struct Locations<'a> {
    compilation: &'a Compilation,
    locations: HashMap<AnchorId, PathBuf>,
    index: HashMap<PathBuf, FileIndex>,
}

/// Lays out `target` the way a build would, without rendering or writing
/// anything past what finding the headings takes.
///
/// `target` is a target id, like `claude-code`, and has to be one the
/// project builds: a link into output nothing writes would go nowhere.
pub fn locations<'a>(
    compilation: &'a Compilation,
    config: &BelayConfig,
    target: &str,
) -> Result<Locations<'a>, String> {
    let options = Options::load(compilation, config);
    let Some(found) = options.targets.iter().find(|candidate| candidate.id == target) else {
        let built: Vec<String> = options
            .targets
            .iter()
            .map(|candidate| format!("`{}`", candidate.id))
            .collect();
        return Err(if built.is_empty() {
            format!("`{target}` is not a target this project builds; it builds none")
        } else {
            format!(
                "`{target}` is not a target this project builds; it builds {}",
                built.join(", ")
            )
        });
    };
    let mut draft = Draft::default();
    let emission = Emission::collect(compilation, config, &mut draft);
    let closure = references::close(compilation, &emission.roots());
    let Layout { context, .. } = layout(compilation, config, found, &emission, &closure);
    Ok(Locations {
        compilation,
        locations: context.locations,
        index: context.index,
    })
}

impl markdown::LinkResolver for Locations<'_> {
    /// The project-relative file an anchor compiles into, with the fragment
    /// of the heading a reference to it, or to one of its properties, lands
    /// on: `.claude/reference/ui.md#color`.
    fn link(&self, target: &Ref) -> Option<String> {
        let location = self.locations.get(&target.anchor)?;
        let file = slash(location);
        match self.index.get(location) {
            Some(headings) => {
                let fragment = headings
                    .fragment(target, self.compilation)
                    .unwrap_or_else(|| piton_emit::reference_fragment(self.compilation, target));
                Some(format!("{file}#{fragment}"))
            }
            // A skill, command, or agent's own output has no headings to
            // land on, so the link is to the file.
            None => Some(file),
        }
    }
}

// ---------------------------------------------------------------------------
// One target
// ---------------------------------------------------------------------------

/// What rendering needs for one target.
struct TargetContext<'a> {
    compilation: &'a Compilation,
    target: &'a Target,
    locations: HashMap<AnchorId, PathBuf>,
    index: HashMap<PathBuf, FileIndex>,
    misses: RefCell<Vec<Ref>>,
    generated: RefCell<HashSet<(PathBuf, String)>>,
}

impl TargetContext<'_> {
    fn links(&self, path: &Path, indexed: bool) -> Links<'_> {
        Links {
            from_directory: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            anchors: self.compilation,
            locations: &self.locations,
            index: indexed.then_some(&self.index),
            misses: &self.misses,
            generated: &self.generated,
        }
    }
}

/// Where one target puts every document and native artifact, and the
/// headings each reference document will have.
struct Layout<'a> {
    context: TargetContext<'a>,
    /// Each reference document and the anchors it holds, in order.
    files: BTreeMap<PathBuf, Vec<AnchorId>>,
    compiled_shape: String,
}

fn layout<'a>(
    compilation: &'a Compilation,
    config: &BelayConfig,
    target: &'a Target,
    emission: &Emission,
    closure: &Closure,
) -> Layout<'a> {
    let project_root = &compilation.project.root;

    // Reference destinations are needed before anything renders, because a
    // reference link has to point at a planned output.
    //
    // A reference to a skill, command, or agent links to that construct's own
    // output, so those are not copied into the reference directory.
    let mut natives: HashMap<AnchorId, PathBuf> = HashMap::new();
    for item in &emission.constructs {
        let name = kebab_case(&item.name);
        let path = match item.kind {
            ConstructKind::Skill => target.skill_path(&name),
            ConstructKind::Command => target.command_path(&name),
            ConstructKind::Agent => target.agent_path(&name),
            ConstructKind::Instruction => continue,
        };
        natives.insert(item.anchor, PathBuf::from(path));
    }
    let mut documented = emission.documented();
    documented.extend(closure.referenced.iter().copied());
    documented.retain(|anchor| !natives.contains_key(anchor));
    let compiled_shape = target.shape_root();
    let mut files: BTreeMap<PathBuf, Vec<AnchorId>> = BTreeMap::new();
    let mut locations = HashMap::new();
    for anchor in &documented {
        let source = compilation.anchor_module_path(*anchor);
        if source.to_string_lossy().starts_with('@') {
            continue;
        }
        let path = references::document_path(
            &source,
            &compilation.project.source_root,
            project_root,
            config.shape_root.as_deref(),
            &target.reference_root,
            &compiled_shape,
        );
        locations.insert(*anchor, path.clone());
        files.entry(path).or_default().push(*anchor);
    }
    // Anchors keep their declaration order within their module's file.
    for anchors in files.values_mut() {
        anchors.sort_by_key(|anchor| compilation.store().anchor(*anchor).item);
    }
    locations.extend(natives);

    let mut context = TargetContext {
        compilation,
        target,
        locations,
        index: HashMap::new(),
        misses: RefCell::new(Vec::new()),
        generated: RefCell::new(HashSet::new()),
    };

    // Headings do not depend on links, so a first rendering finds every
    // file's headings and the second writes links that land on them.
    let mut index = HashMap::new();
    for (path, anchors) in &files {
        let parts = render_documents(&context, path, anchors, false);
        let borrowed: Vec<(AnchorId, &str)> =
            parts.iter().map(|(a, text)| (*a, text.as_str())).collect();
        index.insert(path.clone(), FileIndex::build(&borrowed));
    }
    context.index = index;
    context.misses.borrow_mut().clear();
    context.generated.borrow_mut().clear();

    Layout {
        context,
        files,
        compiled_shape,
    }
}

fn build_target(
    compilation: &Compilation,
    config: &BelayConfig,
    target: &Target,
    emission: &Emission,
    closure: &Closure,
    plan: &mut Draft,
) {
    let first_file = plan.files.len();
    let Layout {
        context,
        files,
        compiled_shape,
    } = layout(compilation, config, target, emission, closure);

    for (path, anchors) in &files {
        let parts = render_documents(&context, path, anchors, true);
        let contents = parts
            .iter()
            .map(|(_, text)| text.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n\n")
            + "\n";
        let kind = if path.starts_with(&compiled_shape) {
            OutputKind::ShapeReference
        } else {
            OutputKind::Reference
        };
        let mut sources: Vec<PathBuf> = anchors
            .iter()
            .map(|anchor| compilation.anchor_module_path(*anchor))
            .collect();
        sources.dedup();
        plan.push(
            OutputFile {
                contents,
                path: path.clone(),
                kind,
                target: target.id,
                origin: anchors
                    .iter()
                    .map(|anchor| compilation.store().anchor(*anchor).name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                sources,
            },
            anchors.clone(),
        );
    }

    // Constructs.
    let mut instructions: BTreeMap<PathBuf, Vec<InstructionSection>> = BTreeMap::new();
    for item in &emission.constructs {
        match item.kind {
            ConstructKind::Skill => render_skill(&context, item, plan),
            ConstructKind::Command => render_command(&context, item, plan),
            ConstructKind::Agent => render_agent(&context, item, plan),
            ConstructKind::Instruction => {
                if let Some(scope) = emission.placements.get(&item.anchor) {
                    let path = module::normalize(&scope.join(&target.instruction_file));
                    let section = instruction_section(&context, item, &path);
                    instructions.entry(path).or_default().push(section);
                }
            }
        }
    }

    // Guidance for one scope is combined into a single file per target, in a
    // stable order -- source path, then declaration order -- so identical
    // input produces identical bytes.
    for (path, mut sections) in instructions {
        sections.sort_by(|a, b| a.source.cmp(&b.source).then(a.item.cmp(&b.item)));
        let contents = sections
            .iter()
            .map(|section| section.text.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n\n")
            + "\n";
        plan.push(
            OutputFile {
                contents,
                path,
                kind: OutputKind::Instruction,
                target: target.id,
                origin: sections
                    .iter()
                    .map(|s| s.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
                sources: sections.iter().map(|s| s.source.clone()).collect(),
            },
            sections.iter().map(|s| s.anchor).collect(),
        );
    }

    // A reference that found no location was reported when the closure was
    // built; anything else would be a planning gap, and is reported here
    // rather than rendered as a bare name.
    plan.generated_links
        .extend(std::mem::take(&mut *context.generated.borrow_mut()));
    let misses = std::mem::take(&mut *context.misses.borrow_mut());
    let mut missing: BTreeSet<AnchorId> = BTreeSet::new();
    for miss in misses {
        if closure.unrepresentable.contains_key(&miss.anchor) || !missing.insert(miss.anchor) {
            continue;
        }
        plan.diagnostics.push(
            error_at(
                "missing-reference-target",
                format!(
                    "`@{{{}}}` has no planned location for {}",
                    miss.display(compilation),
                    target.id
                ),
                anchor_site(compilation, miss.anchor),
            )
            .with_origin(compilation.store().anchor(miss.anchor).name.clone()),
        );
    }

    // Every file this target planned, whatever produced it, gets its location
    // markers resolved. Doing it here rather than at each construction is what
    // keeps a skill and a reference document agreeing about where `.claude` is.
    let markers = target_locations(compilation, config, target);
    for file in &mut plan.files[first_file..] {
        file.file.contents = resolve_location_markers(&file.contents, &markers, &file.path);
    }

    validate::instruction_files(compilation, target, &plan.files[first_file..], &mut plan.diagnostics);
}

/// Renders each anchor of one reference file as its own document.
fn render_documents(
    context: &TargetContext<'_>,
    path: &Path,
    anchors: &[AnchorId],
    indexed: bool,
) -> Vec<(AnchorId, String)> {
    let links = context.links(path, indexed);
    let markdown = markdown::Context {
        anchors: context.compilation,
        links: &links,
    };
    let constructs = construct::ConstructAnchors::find(context.compilation);
    anchors
        .iter()
        .map(|anchor| {
            // A construct's description and prompt are optional, and a
            // missing one is left out of the output rather than written as
            // null.
            if constructs.classify(context.compilation, *anchor).is_some() {
                let mut properties = piton_core::AnchorView::properties(context.compilation, *anchor).clone();
                for field in ["description", "prompt"] {
                    if matches!(properties.get(field), Some(piton_core::Value::Null)) {
                        properties.shift_remove(field);
                    }
                }
                (*anchor, markdown::document_with(*anchor, &properties, &markdown))
            } else {
                (*anchor, markdown::document(*anchor, &markdown))
            }
        })
        .collect()
}

/// The directory each location export names, relative to the project root.
///
/// `shapeRoot` and `codeRoot` are configured paths, so they are stored absolute
/// and made project-relative here. Both fall back to the project root, which is
/// what the specification says an unconfigured root resolves to.
fn target_locations(
    compilation: &Compilation,
    config: &BelayConfig,
    target: &Target,
) -> Vec<(&'static str, PathBuf)> {
    let project_root = &compilation.project.root;
    let under_project = |path: &Path| -> PathBuf {
        path.strip_prefix(project_root)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| path.to_path_buf())
    };

    vec![
        (prelude::BELAY_SHAPE_TOKEN, PathBuf::from(target.shape_root())),
        (prelude::BELAY_AGENT_ROOT_TOKEN, PathBuf::from(&target.root)),
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

// ---------------------------------------------------------------------------
// Constructs
// ---------------------------------------------------------------------------

/// Reports a missing description the target requires.
fn require_description(
    context: &TargetContext<'_>,
    item: &Construct,
    description: &str,
    plan: &mut Draft,
) {
    if !description.trim().is_empty() || !context.target.requires_description(item.kind) {
        return;
    }
    plan.diagnostics.push(
        error_at(
            "missing-description",
            format!(
                "{} needs a description for the {} `{}`, and it has none",
                context.target.anchor_name,
                item.kind.as_str(),
                item.name
            ),
            anchor_site(context.compilation, item.anchor),
        )
        .with_origin(format!("{}.description", item.name))
        .with_help(format!(
            "add `description:` to `{}`; {} discovers a {} by it",
            item.name,
            context.target.id,
            item.kind.as_str()
        )),
    );
}

fn render_skill(context: &TargetContext<'_>, item: &Construct, plan: &mut Draft) {
    let compilation = context.compilation;
    let target = context.target;
    let name = kebab_case(&item.name);
    let path = PathBuf::from(target.skill_path(&name));
    let links = context.links(&path, true);
    let markdown = markdown::Context {
        anchors: compilation,
        links: &links,
    };

    let description = item.metadata_text("description", compilation).unwrap_or_default();
    let use_when = item.metadata_text("useWhen", compilation).unwrap_or_default();
    let prompt = item.text("prompt", &markdown).unwrap_or_default();
    require_description(context, item, &description, plan);

    let mut fields = Fields::new();
    fields.insert("name".into(), FieldValue::Text(name.clone()));
    let discovery = render::discovery_description(&description, &use_when);
    if !discovery.is_empty() {
        fields.insert("description".into(), FieldValue::Text(discovery));
    }
    let consumed = native_options(context, item, &mut fields, plan);

    let remainder = render::remainder(&item.remainder(&consumed), 1, &markdown);
    let contents = format!(
        "{}\n{}",
        render::frontmatter(&fields),
        render::body(&prompt, &remainder)
    );
    validate::emitted_once(compilation, item, &path, &contents, &[&prompt], 1, plan);

    plan.push(
        OutputFile {
            contents,
            path,
            kind: OutputKind::Skill,
            target: target.id,
            origin: item.name.clone(),
            sources: vec![compilation.anchor_module_path(item.anchor)],
        },
        vec![item.anchor],
    );
}

fn render_command(context: &TargetContext<'_>, item: &Construct, plan: &mut Draft) {
    let compilation = context.compilation;
    let target = context.target;
    // The `x-` prefix distinguishes a command from a skill wherever the two
    // share a namespace, and it survives target-name normalization.
    let normalized = kebab_case(&item.name);
    let name = format!("x-{normalized}");
    let description = item.metadata_text("description", compilation).unwrap_or_default();
    require_description(context, item, &description, plan);

    let path = PathBuf::from(target.command_path(&normalized));
    let links = context.links(&path, true);
    let markdown = markdown::Context {
        anchors: compilation,
        links: &links,
    };
    let prompt = item.text("prompt", &markdown).unwrap_or_default();

    let mut fields = Fields::new();
    if target.command_support != CommandSupport::Native {
        // Identity comes from the filename for a native command, so it is
        // only written for the skill a command is translated into.
        fields.insert("name".into(), FieldValue::Text(name.clone()));
    }
    if !description.trim().is_empty() {
        fields.insert("description".into(), FieldValue::Text(description.trim().to_string()));
    }
    if target.command_support == CommandSupport::TranslatedSkill {
        // A command is invoked deliberately; turning it into an automatically
        // selected skill would change what it means.
        fields.insert("disable-model-invocation".into(), FieldValue::Bool(true));
    }
    let consumed = native_options(context, item, &mut fields, plan);

    let remainder = render::remainder(&item.remainder(&consumed), 1, &markdown);
    let contents = format!(
        "{}\n{}",
        render::frontmatter(&fields),
        render::body(&prompt, &remainder)
    );
    validate::emitted_once(compilation, item, &path, &contents, &[&prompt], 1, plan);

    plan.push(
        OutputFile {
            contents,
            path,
            kind: OutputKind::Command,
            target: target.id,
            origin: item.name.clone(),
            sources: vec![compilation.anchor_module_path(item.anchor)],
        },
        vec![item.anchor],
    );

    if target.command_support == CommandSupport::TranslatedSkillWithPolicy {
        if let Some(policy_path) = target.policy_path(&normalized) {
            let mut policy = Fields::new();
            let mut inner = indexmap::IndexMap::new();
            inner.insert(
                "allow_implicit_invocation".to_string(),
                FieldValue::Bool(false),
            );
            policy.insert("policy".into(), FieldValue::Map(inner));
            plan.push(
                OutputFile {
                    path: PathBuf::from(policy_path),
                    contents: render::yaml_document(&policy),
                    kind: OutputKind::CommandPolicy,
                    target: target.id,
                    origin: item.name.clone(),
                    sources: vec![compilation.anchor_module_path(item.anchor)],
                },
                vec![item.anchor],
            );
        }
    }
}

fn render_agent(context: &TargetContext<'_>, item: &Construct, plan: &mut Draft) {
    let compilation = context.compilation;
    let target = context.target;
    let name = kebab_case(&item.name);
    let path = PathBuf::from(target.agent_path(&name));
    let links = context.links(&path, true);
    let markdown = markdown::Context {
        anchors: compilation,
        links: &links,
    };

    let description = item.metadata_text("description", compilation).unwrap_or_default();
    let role = item.text("role", &markdown).unwrap_or_default();
    let prompt = item.text("prompt", &markdown).unwrap_or_default();
    require_description(context, item, &description, plan);

    let mut fields = Fields::new();
    if target.agent_format != AgentFormat::MarkdownFilenameIdentity {
        fields.insert("name".into(), FieldValue::Text(name.clone()));
    }
    if !description.trim().is_empty() {
        fields.insert("description".into(), FieldValue::Text(description.trim().to_string()));
    }
    if target.agent_format == AgentFormat::MarkdownFilenameIdentity
        && !item.properties.contains_key("mode")
    {
        if let Some(mode) = &target.default_mode {
            // Activation has to be explicit; a missing mode would leave the
            // platform to guess whether this is a primary agent.
            fields.insert("mode".into(), FieldValue::Text(mode.clone()));
        }
    }
    let consumed = native_options(context, item, &mut fields, plan);

    let introduction = render::role_introduction(&role);
    let primary = if prompt.trim().is_empty() {
        introduction
    } else {
        format!("{introduction}\n\n{}", prompt.trim())
    };
    let remainder = render::remainder(&item.remainder(&consumed), 1, &markdown);
    let assembled = render::body(&primary, &remainder);

    let contents = match target.agent_format {
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
    if target.agent_format != AgentFormat::Toml {
        validate::emitted_once(compilation, item, &path, &contents, &[&prompt], 1, plan);
    }

    plan.push(
        OutputFile {
            contents,
            path,
            kind: OutputKind::Agent,
            target: target.id,
            origin: item.name.clone(),
            sources: vec![compilation.anchor_module_path(item.anchor)],
        },
        vec![item.anchor],
    );
}

/// One instruction's part of a scope's combined guidance file.
struct InstructionSection {
    anchor: AnchorId,
    name: String,
    source: PathBuf,
    item: usize,
    text: String,
}

/// An instruction renders as its title, then its description and prompt as
/// body text, then its other properties one level below its title.
fn instruction_section(
    context: &TargetContext<'_>,
    item: &Construct,
    path: &Path,
) -> InstructionSection {
    let compilation = context.compilation;
    let links = context.links(path, true);
    let markdown = markdown::Context {
        anchors: compilation,
        links: &links,
    };
    let mut primary = Vec::new();
    for key in ["description", "prompt"] {
        if let Some(text) = item.text(key, &markdown) {
            if !text.trim().is_empty() {
                primary.push(text.trim().to_string());
            }
        }
    }
    let remainder = render::remainder(&item.remainder(&[]), 2, &markdown);
    let body = render::body(&primary.join("\n\n"), &remainder);
    let title = title_case(&item.name);
    let text = if body.is_empty() {
        format!("# {title}\n")
    } else {
        format!("# {title}\n\n{body}")
    };
    InstructionSection {
        anchor: item.anchor,
        name: item.name.clone(),
        source: compilation.anchor_module_path(item.anchor),
        item: compilation.store().anchor(item.anchor).item,
        text,
    }
}

/// Property names a target might carry natively, across all targets. Anything
/// in this set is metadata rather than prose, so it never falls into the
/// serialized remainder.
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

/// Copies explicitly configured native options into the target's metadata.
///
/// An option the target cannot represent is an error, not a warning: a prompt
/// that asks for restraint is not an enforced restriction, and claiming
/// otherwise would be worse than refusing to build.
fn native_options(
    context: &TargetContext<'_>,
    item: &Construct,
    fields: &mut Fields,
    plan: &mut Draft,
) -> Vec<&'static str> {
    let compilation = context.compilation;
    let target = context.target;
    let supported = target.options(item.kind);
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
        let site = property_site(compilation, item.anchor, name);
        let origin = format!("{}.{name}", item.name);
        let Some(option) = supported.iter().find(|option| option.name == normalized) else {
            let help = if supported.is_empty() {
                format!(
                    "{} has no native options for a {}; remove `{name}` or build this {} only for adapters that support it",
                    target.id,
                    item.kind.as_str(),
                    item.kind.as_str()
                )
            } else {
                format!(
                    "{} accepts {} for a {}",
                    target.id,
                    supported
                        .iter()
                        .map(|o| format!("`{}`", o.name))
                        .collect::<Vec<_>>()
                        .join(", "),
                    item.kind.as_str()
                )
            };
            plan.diagnostics.push(
                error_at(
                    "unsupported-option",
                    format!(
                        "`{name}` on the {} `{}` cannot be represented by {}: it has no native `{normalized}` option, so it would not be enforced",
                        item.kind.as_str(),
                        item.name,
                        target.anchor_name
                    ),
                    site,
                )
                .with_origin(origin)
                .with_help(help),
            );
            continue;
        };
        match native_value(option, value, compilation) {
            Ok(field) => {
                fields.insert(normalized.to_string(), field);
            }
            Err(problem) => plan.diagnostics.push(
                error_at(
                    "invalid-option",
                    format!(
                        "`{name}` on `{}` is not a valid {} value: {problem}",
                        item.name, target.id
                    ),
                    site,
                )
                .with_origin(origin),
            ),
        }
    }
    consumed
}

/// Checks a native option's value against the target's rule and converts it.
fn native_value(
    option: &NativeOption,
    value: &Value,
    compilation: &Compilation,
) -> Result<FieldValue, String> {
    let text = |value: &Value| -> Option<String> {
        match value {
            Value::Str(text) if text.is_plain() => Some(text.render_plain(compilation).trim().to_string()),
            _ => None,
        }
    };
    const DECISIONS: &[&str] = &["allow", "ask", "deny"];
    match option.rule {
        OptionRule::Text => text(value).ok_or_else(|| "expected text".to_string()).map(FieldValue::Text),
        OptionRule::TextOrList => match value {
            Value::List(_) => match FieldValue::from_value(value, compilation) {
                Some(field @ FieldValue::List(_)) => Ok(field),
                _ => Err("expected a list of names".to_string()),
            },
            other => text(other)
                .map(FieldValue::Text)
                .ok_or_else(|| "expected a name or a list of names".to_string()),
        },
        OptionRule::OneOf(allowed) => match text(value) {
            Some(choice) if allowed.contains(&choice.as_str()) => Ok(FieldValue::Text(choice)),
            _ => Err(format!("expected one of {}", allowed.join(", "))),
        },
        OptionRule::ProviderModel => match text(value) {
            Some(model)
                if model
                    .split_once('/')
                    .is_some_and(|(provider, id)| !provider.is_empty() && !id.is_empty())
                    && !model.contains(char::is_whitespace) =>
            {
                Ok(FieldValue::Text(model))
            }
            _ => Err("expected a provider-qualified model such as `anthropic/claude-sonnet-4-5`".to_string()),
        },
        OptionRule::PermissionMap => {
            let entries: Vec<(String, Value)> = match value {
                Value::Dict(map) => map.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
                Value::Mixed(mixed) if mixed.entries().count() == mixed.items.len() => mixed
                    .entries()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
                _ => return Err("expected a map of tool names to allow, ask or deny".to_string()),
            };
            let mut out = indexmap::IndexMap::new();
            for (tool, decision) in entries {
                match &decision {
                    Value::Dict(patterns) => {
                        let mut inner = indexmap::IndexMap::new();
                        for (pattern, choice) in patterns {
                            match text(choice) {
                                Some(c) if DECISIONS.contains(&c.as_str()) => {
                                    inner.insert(pattern.clone(), FieldValue::Text(c));
                                }
                                _ => {
                                    return Err(format!(
                                        "`{tool}.{pattern}` must be allow, ask or deny"
                                    ))
                                }
                            }
                        }
                        out.insert(tool, FieldValue::Map(inner));
                    }
                    other => match text(other) {
                        Some(c) if DECISIONS.contains(&c.as_str()) => {
                            out.insert(tool, FieldValue::Text(c));
                        }
                        _ => return Err(format!("`{tool}` must be allow, ask or deny, or a map of patterns to those")),
                    },
                }
            }
            Ok(FieldValue::Map(out))
        }
    }
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

/// Writes a plan to disk, returning the paths written.
///
/// A file at a planned path that the previous build's manifest does not
/// record belongs to someone else: nothing is written, and the error names it.
/// [`plan`] reports the same thing as a diagnostic; this is the last guard.
/// After writing, the manifest records the written paths alongside the ones it
/// already held, so a later build still knows it owns them.
pub fn write(plan: &Plan, root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let previous = manifest::previous(root);
    let unowned: Vec<String> = plan
        .files
        .iter()
        .map(|file| slash(&file.path))
        .filter(|path| root.join(path).exists() && !previous.contains(path))
        .collect();
    if !unowned.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!(
                "refusing to overwrite files Belay did not generate: {}",
                unowned.join(", ")
            ),
        ));
    }

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

    let mut owned: BTreeSet<String> = previous;
    owned.extend(plan.manifest());
    let owned: Vec<String> = owned.into_iter().collect();
    let manifest_path = root.join(MANIFEST);
    if let Some(parent) = manifest_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(manifest_path, manifest::document_with(&owned, &plan.targets))?;
    Ok(written)
}

/// Removes files a previous build generated that this one no longer produces.
///
/// Only paths recorded in the previous manifest are considered, so cleanup can
/// never delete a file Belay did not write; a recorded path that would leave
/// the project is ignored. Directories the removal leaves empty go too.
pub fn clean_stale(previous: &[String], plan: &Plan, root: &Path) -> Vec<PathBuf> {
    let current: BTreeSet<String> = plan.manifest().into_iter().collect();
    let mut removed = Vec::new();
    for path in previous {
        if current.contains(path) || !manifest::is_safe(path) {
            continue;
        }
        let target = root.join(path);
        if target.is_file() && std::fs::remove_file(&target).is_ok() {
            manifest::prune_empty_parents(root, Path::new(path));
            removed.push(PathBuf::from(path));
        }
    }
    removed
}

/// A build manifest, so a later build knows what it owns.
pub fn manifest_document(plan: &Plan) -> String {
    manifest::document(plan)
}

/// Reads the paths recorded in a manifest document.
pub fn manifest_paths(document: &str) -> Vec<String> {
    manifest::paths(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, kind: OutputKind, target: &'static str, origin: &str) -> OutputFile {
        OutputFile {
            path: PathBuf::from(path),
            contents: String::new(),
            kind,
            target,
            origin: origin.into(),
            sources: Vec::new(),
        }
    }

    #[test]
    fn manifests_round_trip() {
        let plan = Plan {
            files: vec![
                file("b.md", OutputKind::Reference, "claude-code", "B"),
                file("a.md", OutputKind::Reference, "claude-code", "A"),
            ],
            diagnostics: Vec::new(),
            targets: vec![("claude-code".into(), "2026-09-21".into())],
        };
        let document = manifest_document(&plan);
        assert_eq!(manifest_paths(&document), vec!["a.md", "b.md"]);
    }

    #[test]
    fn artifact_names_come_from_the_path() {
        let skill = file(
            ".claude/skills/build-tooling/SKILL.md",
            OutputKind::Skill,
            "claude-code",
            "BuildTooling",
        );
        assert_eq!(validate::artifact_name(&skill).as_deref(), Some("build-tooling"));
        let command = file(
            ".opencode/commands/x-release.md",
            OutputKind::Command,
            "opencode",
            "Release",
        );
        assert_eq!(validate::artifact_name(&command).as_deref(), Some("x-release"));
    }
}
