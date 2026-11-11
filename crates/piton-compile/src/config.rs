//! Project configuration.
//!
//! A `piton.config.pi` in the working directory configures a project: where the
//! source root is, which file is the entry point, and which plugins are
//! enabled. The configuration is itself Piton, so it is compiled the same way
//! everything else is.

use std::path::{Path, PathBuf};

use piton_core::{AnchorId, Diagnostic, DiagnosticSink, Span, Value};

use crate::eval;
use crate::module;
use crate::packages::{self, Dependency, PackageDecl};
use crate::resolve;
use crate::store::Symbol;

/// The name of the configuration file the compiler looks for.
pub const CONFIG_FILE: &str = "piton.config.pi";

/// Where build output goes when the configuration does not say, relative to
/// the configuration file.
pub const DEFAULT_OUTPUT: &str = "dist";

/// The renderer a build uses when the configuration does not say.
pub const DEFAULT_RENDERER: &str = "json";

/// The renderers a configuration may name, in their canonical spelling.
pub const RENDERERS: [&str; 3] = ["json", "yaml", "markdown"];

/// The keyword a configuration anchor is declared with.
pub const CONFIG_KEYWORD: &str = "piton-config";

/// The keyword a package declaration is written with.
pub const PACKAGE_KEYWORD: &str = "piton-package";

/// Every key a `piton-config` anchor understands. Anything else is most likely
/// a typo, and a typo in a configuration is silently ignored configuration.
pub const CONFIG_KEYS: [&str; 7] = [
    "root",
    "entry",
    "output",
    "renderer",
    "packages",
    "dependencies",
    "plugins",
];

/// Keys a configuration used to have. Each is an error that says what to
/// write instead, rather than a setting silently ignored.
const REMOVED_KEYS: [(&str, &str); 1] = [(
    "frameworks",
    "a framework is a plugin now: move each item under `plugins`, like `plugins:` with `- {BelayConfiguration}` beneath it",
)];

/// One anchor listed under `plugins` in the configuration.
///
/// The configuration doesn't know which plugins exist: that is the plugin
/// host's business, and some plugins are installed rather than bundled. So an
/// entry is recorded as plain data, with enough to classify it later: every
/// anchor in its inheritance chain and the module each is declared in, which
/// tells a `piton-plugin` declaration from the settings anchor of a plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginEntry {
    /// The anchor's name.
    pub name: String,
    /// The file the anchor is declared in.
    pub file: PathBuf,
    /// Where the anchor's name is, for a diagnostic about it.
    pub name_span: Span,
    /// Every anchor in its chain, itself last, as (module path, name).
    pub chain: Vec<(String, String)>,
    /// Its resolved properties, as JSON.
    pub properties: serde_json::Map<String, serde_json::Value>,
    /// Where each property the anchor writes itself is named.
    pub spans: Vec<(String, Span)>,
    /// Every anchor its properties name, like the adapters a belay-config
    /// lists, with its properties as JSON, so a plugin can read what a
    /// setting points at.
    pub anchors: Vec<(String, serde_json::Map<String, serde_json::Value>)>,
    /// Where each anchor in `anchors` is declared, in the same order: its
    /// file and the span of its name, so a plugin can point a diagnostic at
    /// a setting's target.
    pub anchor_sites: Vec<(PathBuf, Span)>,
}

impl PluginEntry {
    /// Whether `name`, declared in a module whose path starts with
    /// `module_root` when one is given, is in this entry's chain.
    pub fn extends(&self, module_root: Option<&str>, name: &str) -> bool {
        self.chain.iter().any(|(module, anchor)| {
            anchor == name && module_root.map_or(true, |root| module == root || module.starts_with(&format!("{root}/")))
        })
    }

    /// Where a property is named, or the anchor's name when it doesn't write
    /// that property itself.
    pub fn span_of(&self, property: &str) -> Span {
        self.spans
            .iter()
            .find(|(name, _)| name == property)
            .map(|(_, span)| *span)
            .unwrap_or(self.name_span)
    }
}

/// Canonical spelling of a renderer name, or `None` when it names none.
pub fn canonical_renderer(text: &str) -> Option<&'static str> {
    match text.trim().to_ascii_lowercase().as_str() {
        "json" => Some("json"),
        "yaml" | "yml" => Some("yaml"),
        "markdown" | "md" => Some("markdown"),
        _ => None,
    }
}

/// A resolved project.
#[derive(Debug, Clone)]
pub struct Project {
    /// Directory the configuration was found in; output paths are relative to
    /// it.
    pub root: PathBuf,
    /// Root that absolute module paths resolve against.
    pub source_root: PathBuf,
    /// The file compilation starts from.
    pub entry: PathBuf,
    /// Where `piton build` writes the renderer's output, absolute. `output` in
    /// the configuration, relative to it; `./dist` when left out.
    pub output_dir: PathBuf,
    /// The renderer `piton build` writes with: `json`, `yaml`, or `markdown`.
    /// `renderer` in the configuration; `json` when left out.
    pub renderer: String,
    /// Whether the configuration asks for the renderer's output by setting
    /// `output` or `renderer`. A project whose plugins build something gets
    /// their output instead of the renderer's, unless it asks; a project
    /// without one always gets the renderer's.
    pub renders: bool,
    pub config_path: Option<PathBuf>,
    /// Packages this project publishes, in declaration order.
    pub packages: Vec<PackageDecl>,
    /// Dependencies the project itself requires, with any version pins.
    pub dependencies: Vec<Dependency>,
    /// The anchors listed under `plugins`: plugin declarations and plugin
    /// settings, in the order written.
    pub plugins: Vec<PluginEntry>,
}

impl Project {
    /// Builds a project for a single file, with no configuration.
    pub fn for_file(path: &Path) -> Project {
        let directory = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Project {
            root: directory.clone(),
            output_dir: directory.join(DEFAULT_OUTPUT),
            source_root: directory,
            entry: path.to_path_buf(),
            renderer: DEFAULT_RENDERER.to_string(),
            renders: true,
            config_path: None,
            packages: Vec::new(),
            dependencies: Vec::new(),
            plugins: Vec::new(),
        }
    }

    /// The same project, compiled from a different file.
    ///
    /// A command given one file still wants the project's roots, plugins and
    /// packages: the file is part of that project, and resolving its imports
    /// without them would answer a different question than the build does.
    pub fn with_entry(&self, entry: &Path) -> Project {
        Project {
            entry: entry.to_path_buf(),
            ..self.clone()
        }
    }

    /// The roots module resolution needs: where absolute imports point, and
    /// where installed packages live.
    pub fn roots(&self) -> module::Roots<'_> {
        module::Roots {
            source_root: &self.source_root,
            project_root: &self.root,
        }
    }
}

/// Finds the configuration file in `start` (or the directory of `start` when
/// it names a file). The compiler reads a `piton.config.pi` in the current
/// working directory; it does not look in the directories above it.
pub fn find_config(start: &Path) -> Option<PathBuf> {
    let directory = if start.is_dir() {
        start.to_path_buf()
    } else {
        start.parent().map(Path::to_path_buf)?
    };
    let candidate = directory.join(CONFIG_FILE);
    candidate.is_file().then_some(candidate)
}

/// Loads a project from an explicit configuration path, or from the
/// configuration in `start`.
pub fn load(start: &Path, explicit: Option<&Path>) -> (Project, DiagnosticSink) {
    let mut diagnostics = DiagnosticSink::new();
    let config_path = match explicit {
        Some(path) if path.is_dir() => find_config(path),
        Some(path) => Some(path.to_path_buf()),
        None => find_config(start),
    };

    let Some(config_path) = config_path else {
        // Without configuration the working directory is the project.
        let root = if start.is_dir() {
            start.to_path_buf()
        } else {
            start.parent().unwrap_or(Path::new(".")).to_path_buf()
        };
        return (
            Project {
                root: root.clone(),
                source_root: root.clone(),
                output_dir: root.join(DEFAULT_OUTPUT),
                renderer: DEFAULT_RENDERER.to_string(),
                renders: true,
                entry: root,
                config_path: None,
                    packages: Vec::new(),
                dependencies: Vec::new(),
                plugins: Vec::new(),
            },
            diagnostics,
        );
    };

    let root = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // The configuration is compiled on its own, rooted at its own directory.
    let resolution = resolve::resolve(&config_path, module::Roots::flat(&root));
    let outcome = eval::evaluate(&resolution);
    diagnostics.extend(resolution.diagnostics.iter().cloned());
    diagnostics.extend(outcome.diagnostics.iter().cloned());

    let Some(config_module) = resolution.graph.id_for(&config_path) else {
        return (default_project(&root, &config_path), diagnostics);
    };

    let config_anchor = resolution
        .scope(config_module)
        .declarations
        .values()
        .find_map(|symbol| match symbol {
            Symbol::Anchor(anchor) => {
                let def = resolution.store.anchor(*anchor);
                (def.keyword == CONFIG_KEYWORD).then_some(*anchor)
            }
            _ => None,
        });

    let Some(config_anchor) = config_anchor else {
        diagnostics.push(
            Diagnostic::error(
                "missing-config",
                "no `piton-config` anchor found",
                &config_path,
                Span::default(),
            )
            .with_help(
                "declare one with `use @piton/config` and `export piton-config MyConfig:`"
                    .to_string(),
            ),
        );
        return (default_project(&root, &config_path), diagnostics);
    };

    let properties = outcome
        .anchors
        .get(&config_anchor)
        .cloned()
        .unwrap_or_default();

    let written = written_properties(&resolution, config_anchor);
    warn_unknown_keys(&written, &config_path, &mut diagnostics);

    let output_dir = properties
        .get("output")
        .and_then(text_of)
        .filter(|value| !value.is_empty())
        .map(|value| module::normalize(&root.join(value)))
        .unwrap_or_else(|| root.join(DEFAULT_OUTPUT));

    let renderer = match properties.get("renderer").and_then(text_of) {
        None => DEFAULT_RENDERER.to_string(),
        Some(text) if text.is_empty() => DEFAULT_RENDERER.to_string(),
        Some(text) => match canonical_renderer(&text) {
            Some(renderer) => renderer.to_string(),
            None => {
                let span = written
                    .iter()
                    .find(|property| property.name == "renderer")
                    .map(|property| property.name_span)
                    .unwrap_or_default();
                diagnostics.push(
                    Diagnostic::error(
                        "unknown-renderer",
                        format!("`{text}` is not a renderer"),
                        &config_path,
                        span,
                    )
                    .with_help(format!("the renderers are {}", RENDERERS.join(", "))),
                );
                DEFAULT_RENDERER.to_string()
            }
        },
    };

    let source_root = properties
        .get("root")
        .and_then(text_of)
        .map(|value| module::normalize(&root.join(value)))
        .unwrap_or_else(|| root.clone());

    // The entry is optional and defaults to the root. A root is a directory,
    // and a directory is a module through its `index.pi` -- the same rule an
    // import written against a directory follows, so a project and an import
    // agree about what a directory means.
    let entry = module_entry(
        properties
            .get("entry")
            .and_then(text_of)
            .map(|value| module::normalize(&root.join(value)))
            .unwrap_or_else(|| source_root.clone()),
    );

    let dependencies = properties
        .get("dependencies")
        .map(|value| {
            let (literals, span) = dependency_literals(&resolution, config_anchor);
            packages::read_dependencies(value, &literals, &config_path, span, &mut diagnostics)
        })
        .unwrap_or_default();

    let mut declared = Vec::new();
    if let Some(list) = properties.get("packages") {
        for item in list.as_list_items() {
            if let Value::Anchor(anchor) = item {
                if let Some(package) =
                    read_package(&resolution, &outcome, anchor, &root, &mut diagnostics)
                {
                    declared.push(package);
                }
            }
        }
    }

    let mut plugins = Vec::new();
    if let Some(list) = properties.get("plugins").filter(|list| !matches!(list, Value::Null)) {
        for item in list.as_list_items() {
            match item {
                Value::Anchor(anchor) => plugins.push(plugin_entry(&resolution, &outcome, anchor)),
                _ => {
                    let span = written
                        .iter()
                        .find(|property| property.name == "plugins")
                        .map(|property| property.name_span)
                        .unwrap_or_default();
                    diagnostics.push(
                        Diagnostic::warning(
                            "unknown-plugin",
                            "every item under `plugins` has to be an anchor",
                            &config_path,
                            span,
                        )
                        .with_help(
                            "list a plugin's declaration or its settings anchor, like `- {Tests}`"
                                .to_string(),
                        ),
                    );
                }
            }
        }
    }

    // A framework decides where its own output goes, so the renderer's copy
    // is only written when the configuration asks for it by name.
    let renders = written
        .iter()
        .any(|property| property.name == "output" || property.name == "renderer");

    (
        Project {
            root,
            source_root,
            entry,
            output_dir,
            renderer,
            renders,
            config_path: Some(config_path),
            packages: declared,
            dependencies,
            plugins,
        },
        diagnostics,
    )
}

/// Records one anchor listed under `plugins`.
fn plugin_entry(
    resolution: &resolve::Resolution,
    outcome: &eval::Outcome,
    anchor: AnchorId,
) -> PluginEntry {
    let def = resolution.store.anchor(anchor);
    let view = OutcomeView { resolution, outcome };
    let chain = resolution
        .store
        .base_chain(anchor)
        .into_iter()
        .map(|id| {
            let base = resolution.store.anchor(id);
            (
                resolution.graph.get(base.module).path.to_string_lossy().to_string(),
                base.name.clone(),
            )
        })
        .collect();
    PluginEntry {
        name: def.name.clone(),
        file: resolution.graph.get(def.module).path.clone(),
        name_span: def.name_span,
        chain,
        properties: outcome
            .anchors
            .get(&anchor)
            .map(|properties| crate::json::properties_map(properties, &view))
            .unwrap_or_default(),
        anchor_sites: named_anchors(outcome.anchors.get(&anchor))
            .into_iter()
            .map(|named| {
                let def = resolution.store.anchor(named);
                (resolution.graph.get(def.module).path.clone(), def.name_span)
            })
            .collect(),
        spans: written_properties(resolution, anchor)
            .iter()
            .map(|property| (property.name.clone(), property.name_span))
            .collect(),
        anchors: named_anchors(outcome.anchors.get(&anchor))
            .into_iter()
            .map(|named| {
                (
                    resolution.store.anchor(named).name.clone(),
                    outcome
                        .anchors
                        .get(&named)
                        .map(|properties| crate::json::properties_map(properties, &view))
                        .unwrap_or_default(),
                )
            })
            .collect(),
    }
}

/// The anchors a set of properties names, at any depth, each once, in order.
fn named_anchors(properties: Option<&piton_core::Properties>) -> Vec<AnchorId> {
    fn walk(value: &Value, out: &mut Vec<AnchorId>) {
        match value {
            Value::Anchor(anchor) if !out.contains(anchor) => out.push(*anchor),
            Value::List(items) => items.iter().for_each(|item| walk(item, out)),
            Value::Dict(map) => map.values().for_each(|item| walk(item, out)),
            Value::Mixed(_) => value.as_list_items().iter().for_each(|item| walk(item, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for value in properties.into_iter().flat_map(|properties| properties.values()) {
        walk(value, &mut out);
    }
    out
}

/// The configuration's anchors, as the JSON view reads them.
struct OutcomeView<'a> {
    resolution: &'a resolve::Resolution,
    outcome: &'a eval::Outcome,
}

impl piton_core::AnchorView for OutcomeView<'_> {
    fn name(&self, id: AnchorId) -> &str {
        &self.resolution.store.anchor(id).name
    }

    fn properties(&self, id: AnchorId) -> &piton_core::Properties {
        static EMPTY: std::sync::OnceLock<piton_core::Properties> = std::sync::OnceLock::new();
        self.outcome
            .anchors
            .get(&id)
            .unwrap_or_else(|| EMPTY.get_or_init(piton_core::Properties::new))
    }

    fn source_path(&self, id: AnchorId) -> &Path {
        let def = self.resolution.store.anchor(id);
        &self.resolution.graph.get(def.module).path
    }

    fn is_abstract(&self, id: AnchorId) -> bool {
        self.resolution.store.anchor(id).is_abstract
    }
}

/// Reads one `piton-package` anchor from the configuration's `packages` list.
///
/// A package names a directory of this project that is published on its own,
/// and the dependencies that directory needs. The name it installs under is
/// its `name` when it has one and its anchor name otherwise, which is what lets
/// a package be called something an anchor cannot, like `my-package`.
fn read_package(
    resolution: &resolve::Resolution,
    outcome: &eval::Outcome,
    anchor: AnchorId,
    root: &Path,
    diagnostics: &mut DiagnosticSink,
) -> Option<PackageDecl> {
    let def = resolution.store.anchor(anchor);
    let file = resolution.graph.get(def.module).path.clone();
    let properties = outcome.anchors.get(&anchor)?;
    if def.keyword != PACKAGE_KEYWORD {
        diagnostics.push(Diagnostic::warning(
            "unknown-package",
            format!("`{}` is not a package declaration", def.name),
            file,
            def.name_span,
        ));
        return None;
    }

    let name = properties
        .get("name")
        .and_then(text_of)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| def.name.clone());

    if let Some(problem) = packages::validate_name(&name) {
        let span = written_properties(resolution, anchor)
            .iter()
            .find(|property| property.name == "name")
            .map(|property| property.name_span)
            .unwrap_or(def.name_span);
        diagnostics.push(
            Diagnostic::error("invalid-package-name", problem, file, span).with_help(
                "a package installs as one directory under tethers/, so use a plain name like `my-package`"
                    .to_string(),
            ),
        );
        return None;
    }

    let Some(package_root) = properties.get("root").and_then(text_of) else {
        diagnostics.push(
            Diagnostic::error(
                "package-without-root",
                format!("package `{name}` does not say which directory it publishes"),
                file,
                def.name_span,
            )
            .with_help("give it a `root`, as in `root: ./spec`".to_string()),
        );
        return None;
    };

    let dependencies = properties
        .get("dependencies")
        .map(|value| {
            let (literals, span) = dependency_literals(resolution, anchor);
            packages::read_dependencies(value, &literals, &file, span, diagnostics)
        })
        .unwrap_or_default();

    Some(PackageDecl {
        name,
        root: module::normalize(&root.join(package_root)),
        dependencies,
    })
}

/// The properties an anchor's own declaration writes, as the syntax tree has
/// them. Inherited properties are not included, which is the point: these are
/// the ones with a place in the source to point a diagnostic at.
fn written_properties(
    resolution: &resolve::Resolution,
    anchor: AnchorId,
) -> Vec<piton_syntax::ast::Property> {
    let def = resolution.store.anchor(anchor);
    match resolution.graph.get(def.module).ast().items.get(def.item) {
        Some(piton_syntax::ast::Item::Anchor(decl)) => decl.body.properties().cloned().collect(),
        _ => Vec::new(),
    }
}

/// Warns about keys a `piton-config` anchor does not understand.
fn warn_unknown_keys(
    written: &[piton_syntax::ast::Property],
    config_path: &Path,
    diagnostics: &mut DiagnosticSink,
) {
    for property in written {
        if CONFIG_KEYS.contains(&property.name.as_str()) {
            continue;
        }
        if let Some((key, help)) = REMOVED_KEYS.iter().find(|(key, _)| *key == property.name) {
            diagnostics.push(
                Diagnostic::error(
                    format!("{key}-removed"),
                    format!("`{key}` is no longer a piton-config setting"),
                    config_path,
                    property.name_span,
                )
                .with_help(help.to_string()),
            );
            continue;
        }
        diagnostics.push(
            Diagnostic::warning(
                "unknown-config-key",
                format!("`{}` is not a piton-config setting", property.name),
                config_path,
                property.name_span,
            )
            .with_help(format!("the settings are {}", CONFIG_KEYS.join(", "))),
        );
    }
}

/// The pins an anchor's `dependencies` are written with, and where the list is.
fn dependency_literals(
    resolution: &resolve::Resolution,
    anchor: AnchorId,
) -> (Vec<packages::PinLiteral>, Span) {
    let def = resolution.store.anchor(anchor);
    let source = &resolution.graph.get(def.module).source;
    written_properties(resolution, anchor)
        .iter()
        .find(|property| property.name == "dependencies")
        .map(|property| {
            (
                packages::pin_literals(&property.value, source),
                property.name_span,
            )
        })
        .unwrap_or_else(|| (Vec::new(), def.name_span))
}

/// Resolves a configured path to the file compilation starts from.
fn module_entry(path: PathBuf) -> PathBuf {
    if path.is_dir() {
        return path.join("index.pi");
    }
    path
}

fn default_project(root: &Path, config_path: &Path) -> Project {
    Project {
        root: root.to_path_buf(),
        source_root: root.to_path_buf(),
        entry: root.to_path_buf(),
        output_dir: root.join(DEFAULT_OUTPUT),
        renderer: DEFAULT_RENDERER.to_string(),
        renders: true,
        config_path: Some(config_path.to_path_buf()),
        packages: Vec::new(),
        dependencies: Vec::new(),
        plugins: Vec::new(),
    }
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::Str(text) => text.as_plain().map(|t| t.trim().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packages::Pin;

    #[test]
    fn a_directory_entry_resolves_to_its_index() {
        let directory = std::env::temp_dir().join(format!(
            "piton-entry-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("temp dir");
        assert_eq!(
            module_entry(directory.clone()),
            directory.join("index.pi"),
            "a root with no entry compiles from its index"
        );

        let file = directory.join("Thing.pi");
        std::fs::write(&file, "a: 1\n").expect("write");
        assert_eq!(module_entry(file.clone()), file);
        let _ = std::fs::remove_dir_all(&directory);
    }

    fn project_from(name: &str, config: &str) -> (Project, DiagnosticSink, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "piton-config-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("spec")).expect("temp dir");
        std::fs::write(directory.join(CONFIG_FILE), config).expect("write");
        std::fs::write(directory.join("spec/index.pi"), "export anchor A:\n    x: 1\n")
            .expect("write");
        let (project, diagnostics) = load(&directory, None);
        (project, diagnostics, directory)
    }

    fn codes(diagnostics: &DiagnosticSink) -> Vec<String> {
        diagnostics.iter().map(|d| d.code.to_string()).collect()
    }

    #[test]
    fn frameworks_is_an_error_that_says_what_to_write() {
        let (_, diagnostics, directory) = project_from(
            "frameworks",
            "use @piton/config\nuse @piton/belay\n\nexport piton-config App:\n    root: ./spec\n    frameworks:\n        - {Belay}\n\nbelay-config Belay:\n    codeRoot: ./src\n    adapters:\n        - {ClaudeCodeAdapter}\n\nfrom @piton/belay import ClaudeCodeAdapter\n",
        );
        let removed: Vec<_> = diagnostics.iter().filter(|d| d.code == "frameworks-removed").collect();
        assert_eq!(removed.len(), 1, "{:?}", codes(&diagnostics));
        assert!(removed[0].is_error());
        assert!(removed[0].help.as_deref().unwrap_or("").contains("move each item under `plugins`"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn plugins_are_recorded_with_their_chain() {
        let (project, diagnostics, directory) = project_from(
            "plugins",
            "use @piton/config\nuse @piton/test\n\nexport piton-config App:\n    root: ./spec\n    plugins:\n        - {Tests}\n\ntest-config Tests:\n    agent: codex\n    jobs: 8\n",
        );
        assert!(codes(&diagnostics).is_empty(), "{:?}", codes(&diagnostics));
        let [entry] = project.plugins.as_slice() else {
            panic!("{:?}", project.plugins)
        };
        assert_eq!(entry.name, "Tests");
        assert!(entry.extends(Some("@piton/test"), "TestConfig"), "{:?}", entry.chain);
        assert!(!entry.extends(Some("@piton/belay"), "TestConfig"));
        assert_eq!(entry.properties.get("agent"), Some(&serde_json::json!("codex")));
        assert_eq!(entry.properties.get("jobs"), Some(&serde_json::json!(8)));
        assert_ne!(entry.span_of("jobs"), entry.name_span);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn output_and_renderer_default_to_dist_and_json() {
        let (project, diagnostics, directory) = project_from(
            "defaults",
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n",
        );
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        assert_eq!(project.output_dir, directory.join("dist"));
        assert_eq!(project.renderer, "json");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn output_and_renderer_are_read_relative_to_the_config() {
        let (project, diagnostics, directory) = project_from(
            "custom",
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n    output: ./build/out\n    renderer: yaml\n",
        );
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        assert_eq!(project.output_dir, directory.join("build/out"));
        assert_eq!(project.renderer, "yaml");

        let (_, diagnostics, directory) = project_from(
            "bad-renderer",
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n    renderer: toml\n",
        );
        assert!(codes(&diagnostics).contains(&"unknown-renderer".to_string()));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_unknown_key_is_warned_about() {
        let (_, diagnostics, directory) = project_from(
            "unknown-key",
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n    entyr: ./spec/index.pi\n",
        );
        let found: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "unknown-config-key")
            .collect();
        assert_eq!(found.len(), 1, "{:?}", codes(&diagnostics));
        assert!(!found[0].is_error());
        assert!(found[0].message.contains("entyr"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_pin_keeps_the_text_it_was_written_with() {
        let (project, diagnostics, directory) = project_from(
            "pins",
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n\n    dependencies:\n        - https://example.test/a\n            tag: 1.0\n        - https://example.test/b\n            commit: 0123abc\n        - https://example.test/c\n        - https://example.test/d\n            branch: release-2\n",
        );
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        let pins: Vec<(String, Pin)> = project
            .dependencies
            .iter()
            .map(|dependency| (dependency.source.clone(), dependency.pin.clone()))
            .collect();
        assert_eq!(
            pins,
            vec![
                ("https://example.test/a".to_string(), Pin::Tag("1.0".to_string())),
                ("https://example.test/b".to_string(), Pin::Commit("0123abc".to_string())),
                ("https://example.test/c".to_string(), Pin::Default),
                ("https://example.test/d".to_string(), Pin::Branch("release-2".to_string())),
            ]
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn more_than_one_pin_is_an_error() {
        let (_, diagnostics, directory) = project_from(
            "two-pins",
            "use @piton/config\n\nexport piton-config App:\n    root: ./spec\n\n    dependencies:\n        - https://example.test/a\n            tag: 1.0\n            branch: main\n",
        );
        let found: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.code == "multiple-pins")
            .collect();
        assert_eq!(found.len(), 1, "{:?}", codes(&diagnostics));
        assert!(found[0].is_error());
        assert!(!codes(&diagnostics).contains(&"unquoted-pin".to_string()));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn packages_are_declared_with_piton_package() {
        let (project, diagnostics, directory) = project_from(
            "packages",
            "use @piton/config\nuse @piton/packaging\n\nexport piton-config App:\n    root: ./spec\n\n    packages:\n        - {Kit}\n        - {Plain}\n\npiton-package Kit:\n    name: ui-kit\n    root: ./spec\n\n    dependencies:\n        - https://example.test/base\n            tag: 2.0\n\npiton-package Plain:\n    root: ./spec\n",
        );
        assert!(diagnostics.is_empty(), "{:?}", codes(&diagnostics));
        let names: Vec<&str> = project.packages.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["ui-kit", "Plain"]);
        assert_eq!(
            project.packages[0].dependencies[0].pin,
            Pin::Tag("2.0".to_string())
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_package_name_with_a_slash_or_an_at_is_an_error() {
        for (index, name) in ["MyScope/package", "@mine/kit"].iter().enumerate() {
            let (project, diagnostics, directory) = project_from(
                &format!("bad-name-{index}"),
                &format!(
                    "use @piton/config\nuse @piton/packaging\n\nexport piton-config App:\n    root: ./spec\n\n    packages:\n        - {{Kit}}\n\npiton-package Kit:\n    name: {name}\n    root: ./spec\n"
                ),
            );
            assert!(
                codes(&diagnostics).contains(&"invalid-package-name".to_string()),
                "{name}: {:?}",
                codes(&diagnostics)
            );
            assert!(project.packages.is_empty());
            let _ = std::fs::remove_dir_all(&directory);
        }
    }

    #[test]
    fn a_file_without_configuration_is_its_own_project() {
        let project = Project::for_file(Path::new("/tmp/example/Thing.pi"));
        assert_eq!(project.source_root, Path::new("/tmp/example"));
        assert_eq!(project.entry, Path::new("/tmp/example/Thing.pi"));
        assert!(project.plugins.is_empty());
    }
}
