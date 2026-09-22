//! Project configuration.
//!
//! A `piton.config.pi` in the working directory configures a project: where the
//! source root is, which file is the entry point, and which frameworks are
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

/// Belay's per-project settings.
#[derive(Debug, Clone)]
pub struct BelayConfig {
    /// Root of the application's source code.
    pub code_root: PathBuf,
    /// Root of the architectural instruction tree, when configured.
    pub shape_root: Option<PathBuf>,
    /// Target ids of the enabled adapters, in configuration order.
    pub adapters: Vec<String>,
}

/// A framework enabled by the project configuration.
#[derive(Debug, Clone)]
pub enum Framework {
    Belay(BelayConfig),
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
    pub config_path: Option<PathBuf>,
    pub frameworks: Vec<Framework>,
    /// Packages this project publishes, in declaration order.
    pub packages: Vec<PackageDecl>,
    /// Dependencies the project itself requires, with any version pins.
    pub dependencies: Vec<Dependency>,
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
            source_root: directory,
            entry: path.to_path_buf(),
            config_path: None,
            frameworks: Vec::new(),
            packages: Vec::new(),
            dependencies: Vec::new(),
        }
    }

    /// The same project, compiled from a different file.
    ///
    /// A command given one file still wants the project's roots, frameworks and
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

    pub fn belay(&self) -> Option<&BelayConfig> {
        self.frameworks.iter().find_map(|framework| match framework {
            Framework::Belay(config) => Some(config),
        })
    }
}

/// Searches `start` and its ancestors for a configuration file.
pub fn find_config(start: &Path) -> Option<PathBuf> {
    let mut directory = if start.is_dir() {
        Some(start.to_path_buf())
    } else {
        start.parent().map(Path::to_path_buf)
    };
    while let Some(current) = directory {
        let candidate = current.join(CONFIG_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        directory = current.parent().map(Path::to_path_buf);
    }
    None
}

/// Loads a project from an explicit configuration path, or by searching upward
/// from `start`.
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
                entry: root,
                config_path: None,
                frameworks: Vec::new(),
                packages: Vec::new(),
                dependencies: Vec::new(),
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
                (def.keyword == "piton-config").then_some(*anchor)
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

    let mut frameworks = Vec::new();
    if let Some(list) = properties.get("frameworks") {
        for item in list.as_list_items() {
            if let Value::Anchor(anchor) = item {
                if let Some(framework) =
                    read_framework(&resolution, &outcome, anchor, &root, &mut diagnostics)
                {
                    frameworks.push(framework);
                }
            }
        }
    }

    let dependencies = properties
        .get("dependencies")
        .map(|value| {
            packages::read_dependencies(value, &config_path, Span::default(), &mut diagnostics)
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

    (
        Project {
            root,
            source_root,
            entry,
            config_path: Some(config_path),
            frameworks,
            packages: declared,
            dependencies,
        },
        diagnostics,
    )
}

/// Reads one `package` anchor from the configuration's `packages` list.
///
/// A package names a directory of this project that is published on its own,
/// and the dependencies that directory needs. The name it installs under is
/// its `name` when it has one and its anchor name otherwise, which is what lets
/// a scoped package reach `tethers/MyScope/package`.
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
    if def.keyword != "package" {
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
        .map(|value| packages::read_dependencies(value, &file, def.name_span, diagnostics))
        .unwrap_or_default();

    Some(PackageDecl {
        name,
        root: module::normalize(&root.join(package_root)),
        dependencies,
    })
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
        config_path: Some(config_path.to_path_buf()),
        frameworks: Vec::new(),
        packages: Vec::new(),
        dependencies: Vec::new(),
    }
}

fn read_framework(
    resolution: &resolve::Resolution,
    outcome: &eval::Outcome,
    anchor: AnchorId,
    root: &Path,
    diagnostics: &mut DiagnosticSink,
) -> Option<Framework> {
    let def = resolution.store.anchor(anchor);
    let properties = outcome.anchors.get(&anchor)?;
    if def.keyword != "belay-config" {
        diagnostics.push(Diagnostic::warning(
            "unknown-framework",
            format!("`{}` is not a framework configuration", def.name),
            resolution.graph.get(def.module).path.clone(),
            def.name_span,
        ));
        return None;
    }

    let code_root = properties
        .get("codeRoot")
        .and_then(text_of)
        .map(|value| module::normalize(&root.join(value)))
        .unwrap_or_else(|| root.join("src"));
    let shape_root = properties
        .get("shapeRoot")
        .and_then(text_of)
        .map(|value| module::normalize(&root.join(value)));

    let mut adapters = Vec::new();
    if let Some(list) = properties.get("adapters") {
        for item in list.as_list_items() {
            match item {
                Value::Anchor(adapter) => {
                    let target = outcome
                        .anchors
                        .get(&adapter)
                        .and_then(|props| props.get("targetId"))
                        .and_then(text_of);
                    match target {
                        Some(target) => adapters.push(target),
                        None => {
                            let adapter_def = resolution.store.anchor(adapter);
                            diagnostics.push(Diagnostic::error(
                                "invalid-adapter",
                                format!("`{}` has no targetId", adapter_def.name),
                                resolution.graph.get(adapter_def.module).path.clone(),
                                adapter_def.name_span,
                            ));
                        }
                    }
                }
                Value::Str(text) => {
                    if let Some(name) = text.as_plain() {
                        adapters.push(name.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    Some(Framework::Belay(BelayConfig {
        code_root,
        shape_root,
        adapters,
    }))
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

    #[test]
    fn a_file_without_configuration_is_its_own_project() {
        let project = Project::for_file(Path::new("/tmp/example/Thing.pi"));
        assert_eq!(project.source_root, Path::new("/tmp/example"));
        assert_eq!(project.entry, Path::new("/tmp/example/Thing.pi"));
        assert!(project.belay().is_none());
    }
}
