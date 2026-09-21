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
            },
            diagnostics,
        );
    };

    let root = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // The configuration is compiled on its own, rooted at its own directory.
    let resolution = resolve::resolve(&config_path, &root);
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

    let entry = properties
        .get("entry")
        .and_then(text_of)
        .map(|value| module::normalize(&root.join(value)))
        .unwrap_or_else(|| source_root.clone());

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

    (
        Project {
            root,
            source_root,
            entry,
            config_path: Some(config_path),
            frameworks,
        },
        diagnostics,
    )
}

fn default_project(root: &Path, config_path: &Path) -> Project {
    Project {
        root: root.to_path_buf(),
        source_root: root.to_path_buf(),
        entry: root.to_path_buf(),
        config_path: Some(config_path.to_path_buf()),
        frameworks: Vec::new(),
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
    fn a_file_without_configuration_is_its_own_project() {
        let project = Project::for_file(Path::new("/tmp/example/Thing.pi"));
        assert_eq!(project.source_root, Path::new("/tmp/example"));
        assert_eq!(project.entry, Path::new("/tmp/example/Thing.pi"));
        assert!(project.belay().is_none());
    }
}
