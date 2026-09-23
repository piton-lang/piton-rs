//! What the project configuration says beyond [`BelayConfig`].
//!
//! [`BelayConfig`] carries the roots and the adapters' target ids. Belay also
//! needs the adapter anchors themselves -- they state each target's paths and
//! settings -- and the deployment choices a `belay-config` can make. Both live
//! in `piton.config.pi`, so it is compiled again here, the same way the
//! configuration loader compiles it, and read directly.

use std::path::{Path, PathBuf};

use piton_compile::{eval, module, resolve, BelayConfig, Compilation, Symbol};
use piton_core::{AnchorId, Diagnostic, Properties, Span, Value};

use crate::adapter::{Adapter, Target};

/// The deployment choice for skills one adapter writes and another discovers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossDiscovery {
    /// Accept that a platform also offers the skills other adapters wrote.
    Allow,
    /// The generated trees are deployed separately, so no platform sees
    /// another adapter's skills.
    Separate,
}

/// A place in the configuration file, for diagnostics.
#[derive(Debug, Clone)]
pub struct Site {
    pub file: PathBuf,
    pub span: Span,
}

/// The resolved configuration Belay plans against.
#[derive(Debug, Clone)]
pub struct Options {
    pub targets: Vec<Target>,
    pub cross_discovery: Option<CrossDiscovery>,
    pub instruction_byte_limit: Option<u64>,
    /// The `belay-config` anchor.
    pub config: Site,
    /// The `adapters` property.
    pub adapters: Site,
    /// The `crossDiscovery` property, or the anchor when it is absent.
    pub cross_discovery_site: Site,
    pub diagnostics: Vec<Diagnostic>,
}

impl Options {
    /// Reads the options for `config` from the project's configuration file.
    ///
    /// Without a configuration file -- a project assembled in code -- every
    /// target is the built-in adapter with the configured id.
    pub fn load(compilation: &Compilation, config: &BelayConfig) -> Options {
        let file = compilation
            .project
            .config_path
            .clone()
            .unwrap_or_else(|| compilation.project.root.clone());
        let fallback = Site {
            file: file.clone(),
            span: Span::default(),
        };
        let mut options = Options {
            targets: Vec::new(),
            cross_discovery: None,
            instruction_byte_limit: None,
            config: fallback.clone(),
            adapters: fallback.clone(),
            cross_discovery_site: fallback,
            diagnostics: Vec::new(),
        };

        let anchors = match compilation.project.config_path.as_deref() {
            Some(path) if path.is_file() => read_config(path, config, &mut options),
            _ => None,
        };

        match anchors {
            Some(adapters) => {
                for (anchor_name, target_id, properties, site) in adapters {
                    match Adapter::by_target(&target_id) {
                        Some(base) => options.targets.push(Target::from_anchor(
                            base,
                            &anchor_name,
                            &properties,
                        )),
                        None => options.diagnostics.push(unknown_adapter(&target_id, site)),
                    }
                }
            }
            None => {
                for target_id in &config.adapters {
                    match Adapter::by_target(target_id) {
                        Some(base) => options.targets.push(Target::builtin(base)),
                        None => options
                            .diagnostics
                            .push(unknown_adapter(target_id, options.adapters.clone())),
                    }
                }
            }
        }

        if let Some(limit) = options.instruction_byte_limit {
            for target in &mut options.targets {
                if target.instruction_byte_limit.is_some() {
                    target.instruction_byte_limit = Some(limit);
                }
            }
        }
        options
    }

    pub fn target(&self, id: &str) -> Option<&Target> {
        self.targets.iter().find(|target| target.id == id)
    }
}

fn unknown_adapter(target: &str, site: Site) -> Diagnostic {
    Diagnostic::error(
        "unknown-adapter",
        format!("`{target}` is not a known Belay adapter"),
        site.file,
        site.span,
    )
    .with_help(format!(
        "available adapters: {}",
        Adapter::all()
            .iter()
            .map(|a| format!("{} ({})", a.export_name, a.target_id))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

type ConfiguredAdapter = (String, String, Properties, Site);

/// Compiles the configuration file and reads the `belay-config` anchor.
/// Returns the configured adapters, or `None` when no such anchor is found.
fn read_config(
    path: &Path,
    config: &BelayConfig,
    options: &mut Options,
) -> Option<Vec<ConfiguredAdapter>> {
    let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let resolution = resolve::resolve(path, module::Roots::flat(&root));
    let outcome = eval::evaluate(&resolution);
    let config_module = resolution.graph.id_for(path)?;

    let candidates: Vec<AnchorId> = resolution
        .scope(config_module)
        .declarations
        .values()
        .filter_map(|symbol| match symbol {
            Symbol::Anchor(anchor) => {
                let def = resolution.store.anchor(*anchor);
                (def.keyword == "belay-config" && !def.is_abstract).then_some(*anchor)
            }
            _ => None,
        })
        .collect();

    let target_ids = |anchor: AnchorId| -> Vec<String> {
        adapter_anchors(outcome.anchors.get(&anchor))
            .into_iter()
            .filter_map(|adapter| {
                outcome
                    .anchors
                    .get(&adapter)
                    .and_then(|p| p.get("targetId"))
                    .and_then(plain)
            })
            .collect()
    };
    // Several configurations may be declared; the one the project enabled is
    // the one whose adapters match.
    let chosen = candidates
        .iter()
        .copied()
        .find(|anchor| target_ids(*anchor) == config.adapters)
        .or_else(|| candidates.first().copied())?;

    let def = resolution.store.anchor(chosen);
    let site = |key: &str| -> Site {
        let owner_path = |slot: &piton_compile::store::Slot| {
            resolution
                .graph
                .get(resolution.store.anchor(slot.owner).module)
                .path
                .clone()
        };
        match def.slots.get(key) {
            // A default inherited from the bundled package is not where the
            // project said anything; point at the configuration anchor then.
            Some(slot) if slot.has_value && !owner_path(slot).to_string_lossy().starts_with('@') => Site {
                file: owner_path(slot),
                span: slot.span,
            },
            _ => Site {
                file: path.to_path_buf(),
                span: def.name_span,
            },
        }
    };
    options.config = Site {
        file: path.to_path_buf(),
        span: def.name_span,
    };
    options.adapters = site("adapters");
    options.cross_discovery_site = site("crossDiscovery");

    let properties = outcome.anchors.get(&chosen).cloned().unwrap_or_default();
    match properties.get("crossDiscovery") {
        None | Some(Value::Null) => {}
        Some(value) => match plain(value).as_deref() {
            Some("allow") => options.cross_discovery = Some(CrossDiscovery::Allow),
            Some("separate") => options.cross_discovery = Some(CrossDiscovery::Separate),
            _ => {
                let at = site("crossDiscovery");
                options.diagnostics.push(
                    Diagnostic::error(
                        "invalid-cross-discovery",
                        "`crossDiscovery` must be `allow` or `separate`",
                        at.file,
                        at.span,
                    )
                    .with_origin(format!("{}.crossDiscovery", def.name)),
                );
            }
        },
    }
    match properties.get("instructionByteLimit") {
        None | Some(Value::Null) => {}
        Some(Value::Number(n)) if *n >= 1.0 && n.fract() == 0.0 => {
            options.instruction_byte_limit = Some(*n as u64);
        }
        Some(_) => {
            let at = site("instructionByteLimit");
            options.diagnostics.push(
                Diagnostic::error(
                    "invalid-instruction-byte-limit",
                    "`instructionByteLimit` must be a positive whole number of bytes",
                    at.file,
                    at.span,
                )
                .with_origin(format!("{}.instructionByteLimit", def.name)),
            );
        }
    }

    let mut adapters = Vec::new();
    for adapter in adapter_anchors(Some(&properties)) {
        let adapter_def = resolution.store.anchor(adapter);
        let props = outcome.anchors.get(&adapter).cloned().unwrap_or_default();
        let Some(target_id) = props.get("targetId").and_then(plain) else {
            // The configuration loader reports an adapter without a targetId.
            continue;
        };
        let at = Site {
            file: resolution.graph.get(adapter_def.module).path.clone(),
            span: adapter_def.name_span,
        };
        adapters.push((adapter_def.name.clone(), target_id, props, at));
    }
    // Adapters given by bare target id rather than by anchor.
    if adapters.len() < config.adapters.len() {
        for id in &config.adapters {
            if !adapters.iter().any(|(_, target, _, _)| target == id) {
                let base = Adapter::by_target(id);
                adapters.push((
                    base.map(|b| b.export_name.to_string())
                        .unwrap_or_else(|| id.clone()),
                    id.clone(),
                    Properties::new(),
                    site("adapters"),
                ));
            }
        }
    }
    Some(adapters)
}

fn adapter_anchors(properties: Option<&Properties>) -> Vec<AnchorId> {
    let Some(list) = properties.and_then(|p| p.get("adapters")) else {
        return Vec::new();
    };
    list.as_list_items()
        .into_iter()
        .filter_map(|item| match item {
            Value::Anchor(anchor) => Some(anchor),
            _ => None,
        })
        .collect()
}

fn plain(value: &Value) -> Option<String> {
    match value {
        Value::Str(text) => text.as_plain().map(|t| t.trim().to_string()),
        _ => None,
    }
}
