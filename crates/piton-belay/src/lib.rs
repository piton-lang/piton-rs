//! The Belay framework.
//!
//! Belay is a Piton framework, not part of the compiler: it is registered into
//! the binary through [`piton_core::framework::Framework`]. It contributes the
//! `@piton/belay` module, the `@{}` reference sigil, and adapters that write
//! agentic Markdown.

pub mod emit;
pub mod markdown;
pub mod module;
pub mod settings;

use std::path::PathBuf;
use std::sync::Mutex;

use piton_core::compile::Compilation;
use piton_core::diag::Diagnostic;
use piton_core::framework::{Emitted, Framework, Interpolation, VirtualModule};
use piton_core::project::Project;
use piton_core::value::{AnchorId, Value};

use settings::Settings;

/// Belay found work to do but was never configured to do it.
fn unconfigured(compilation: &Compilation, project: &Project) -> Diagnostic {
    let file = compilation
        .analysis
        .db
        .files()
        .find(|file| Some(file.source.display()) == project.config_path.as_ref().map(|it| it.display().to_string()))
        .map(|file| file.id)
        .or_else(|| compilation.entries.first().copied())
        .unwrap_or(piton_core::FileId(0));
    Diagnostic::warning(
        "belay-unconfigured",
        file,
        Default::default(),
        "this project declares agents, skills, commands, or instructions, but Belay is not \
         configured, so nothing was written. List the `belay-config` anchor under `frameworks` \
         in piton.config.pi:\n    frameworks:\n        - {BelayConfiguration}",
    )
}

/// The Belay framework plugin.
pub struct Belay {
    settings: Settings,
    /// Anchors an `@{}` or `${}` reference pointed at during evaluation.
    referenced: Mutex<Vec<AnchorId>>,
}

impl Default for Belay {
    fn default() -> Belay {
        Belay::new()
    }
}

impl Belay {
    pub fn new() -> Belay {
        Belay { settings: Settings::default(), referenced: Mutex::new(Vec::new()) }
    }

    fn note_reference(&self, value: &Value) {
        if let Value::Anchor(anchor) = value {
            let mut referenced = self.referenced.lock().unwrap();
            if !referenced.contains(&anchor.id) {
                referenced.push(anchor.id);
            }
        }
    }
}

impl Framework for Belay {
    fn name(&self) -> &str {
        "belay"
    }

    fn modules(&self) -> Vec<VirtualModule> {
        let shape = if self.settings.adapters.is_empty() {
            PathBuf::from(".claude/reference/shape")
        } else {
            self.settings.shape_reference_dir()
        };
        vec![VirtualModule {
            name: module::MODULE.to_string(),
            source: module::source(&shape.display().to_string()),
        }]
    }

    fn sigils(&self) -> Vec<String> {
        vec!["@".to_string()]
    }

    /// `@{X}` points the agent at the compiled file; `${X}` names it.
    fn interpolate(&self, interpolation: &Interpolation<'_>) -> Option<Value> {
        match interpolation.sigil {
            "@" => {
                self.note_reference(interpolation.value);
                let Value::Anchor(anchor) = interpolation.value else { return None };
                // Without a configured adapter there is no compiled file to
                // point at, but a reference must still render as text: letting
                // it fall through would splice the whole anchor into the
                // document and break the `:: string` it sits in.
                let path = interpolation
                    .anchor_source
                    .as_ref()
                    .zip(self.settings.adapters.first())
                    .map(|(source, adapter)| {
                        settings::reference_path(&self.settings, adapter, source, &anchor.name)
                    });
                Some(Value::string(match path {
                    Some(path) => format!("@{}", path.display()),
                    None => anchor.name.clone(),
                }))
            }
            "$" if interpolation.value.is_complex() => {
                self.note_reference(interpolation.value);
                match interpolation.value {
                    Value::Anchor(anchor) => Some(Value::string(anchor.name.clone())),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn claims_config(&self, compilation: &Compilation, config: &Value) -> bool {
        let Value::Anchor(anchor) = config else { return false };
        let Some(base) = compilation.lookup(module::MODULE, module::CONFIG_ANCHOR) else {
            return false;
        };
        anchor.id == base || compilation.analysis.ancestors(anchor.id).contains(&base)
    }

    fn configure(
        &mut self,
        project: &Project,
        _compilation: &Compilation,
        config: &Value,
    ) -> Vec<String> {
        let (settings, messages) = Settings::from_config(&project.base, &project.root, config);
        self.settings = settings;
        messages
    }

    fn emit(&self, compilation: &Compilation, project: &Project) -> Emitted {
        let plan = emit::Plan::discover(compilation);
        let referenced = self.referenced.lock().unwrap().clone();
        let mut emitted = Emitted::default();

        // Declaring a `belay-config` without listing it under `frameworks`
        // leaves Belay unconfigured, which used to mean a silent `wrote 0
        // files`. Say so instead.
        if self.settings.adapters.is_empty() && !plan.is_empty() {
            emitted.diagnostics.push(unconfigured(compilation, project));
            return emitted;
        }

        for adapter in &self.settings.adapters {
            emitted.files.extend(emit::emit_adapter(
                compilation,
                &self.settings,
                adapter,
                &plan,
                &referenced,
            ));
        }
        emitted
    }
}
