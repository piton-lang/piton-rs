//! The framework plugin interface.
//!
//! Frameworks are how Piton grows without the core compiler learning about any
//! particular output target. A framework contributes builtin modules, custom
//! interpolation sigils, late-bound builtin values, and output adapters. The
//! core never names a concrete framework; the binary registers them.

use std::path::PathBuf;

use crate::compile::Compilation;
use crate::diag::Diagnostic;
use crate::project::Project;
use crate::value::{AnchorId, Value};

/// A module the compiler can resolve as `@name`.
#[derive(Clone, Debug)]
pub struct VirtualModule {
    pub name: String,
    pub source: String,
}

/// One interpolation the evaluator wants rendered.
pub struct Interpolation<'a> {
    /// `$`, `@`, or a framework-defined word.
    pub sigil: &'a str,
    /// The already-evaluated value.
    pub value: &'a Value,
    /// The anchor the interpolation was written in, if any.
    pub owner: Option<AnchorId>,
    /// Where the referenced anchor was declared, when the value is an anchor.
    pub anchor_source: Option<PathBuf>,
}

/// A file a framework wants written.
#[derive(Clone, Debug)]
pub struct OutputFile {
    pub path: PathBuf,
    pub contents: String,
}

/// What a framework produced.
#[derive(Clone, Debug, Default)]
pub struct Emitted {
    pub files: Vec<OutputFile>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A compiler plugin.
pub trait Framework: Send + Sync {
    /// The name used in diagnostics and in `piton build` output.
    fn name(&self) -> &str;

    /// Modules this framework makes resolvable, such as `@piton/belay`.
    fn modules(&self) -> Vec<VirtualModule> {
        Vec::new()
    }

    /// Interpolation sigils this framework claims, without the `{`.
    fn sigils(&self) -> Vec<String> {
        Vec::new()
    }

    /// Render an interpolation. Returning `None` falls back to the default.
    fn interpolate(&self, _interpolation: &Interpolation<'_>) -> Option<Value> {
        None
    }

    /// Supply a late-bound builtin variable such as `__BELAY_SHAPE__`.
    fn builtin_value(&self, _name: &str) -> Option<Value> {
        None
    }

    /// True when `config` is this framework's configuration anchor.
    fn claims_config(&self, _compilation: &Compilation, _config: &Value) -> bool {
        false
    }

    /// Read this framework's configuration anchor out of the project config.
    fn configure(
        &mut self,
        _project: &Project,
        _compilation: &Compilation,
        _config: &Value,
    ) -> Vec<String> {
        Vec::new()
    }

    /// Files this framework needs compiled whether or not an entry point
    /// reaches them.
    ///
    /// Some documents are addressed by where they sit rather than by who
    /// imports them. Requiring an import for those would only make it easy to
    /// write one that silently never builds.
    fn roots(&self, _project: &Project) -> Vec<PathBuf> {
        Vec::new()
    }

    /// Produce the framework's output files.
    fn emit(&self, compilation: &Compilation, project: &Project) -> Emitted;
}

/// The frameworks active for one build.
#[derive(Default)]
pub struct Frameworks {
    pub active: Vec<Box<dyn Framework>>,
}

impl Frameworks {
    pub fn new(active: Vec<Box<dyn Framework>>) -> Frameworks {
        Frameworks { active }
    }

    pub fn modules(&self) -> Vec<VirtualModule> {
        self.active.iter().flat_map(|framework| framework.modules()).collect()
    }

    /// Ask each framework in turn to render an interpolation.
    pub fn interpolate(&self, interpolation: &Interpolation<'_>) -> Option<Value> {
        self.active.iter().find_map(|framework| framework.interpolate(interpolation))
    }

    /// Every file some framework needs compiled regardless of reach, in a
    /// stable order.
    pub fn roots(&self, project: &Project) -> Vec<PathBuf> {
        let mut roots: Vec<PathBuf> =
            self.active.iter().flat_map(|framework| framework.roots(project)).collect();
        roots.sort();
        roots.dedup();
        roots
    }

    pub fn builtin_value(&self, name: &str) -> Option<Value> {
        self.active.iter().find_map(|framework| framework.builtin_value(name))
    }

    /// Hand each configuration value to the framework that claims it.
    ///
    /// Returns the messages frameworks reported about their own settings.
    pub fn configure(
        &mut self,
        project: &Project,
        compilation: &Compilation,
        configs: &[Value],
    ) -> Vec<String> {
        let mut messages = Vec::new();
        for config in configs {
            let claimed = self
                .active
                .iter()
                .position(|framework| framework.claims_config(compilation, config));
            match claimed {
                Some(index) => {
                    messages.extend(self.active[index].configure(project, compilation, config))
                }
                None => messages.push(format!(
                    "no registered framework recognises the configuration anchor `{}`",
                    match config {
                        Value::Anchor(anchor) => anchor.name.clone(),
                        other => other.type_name().to_string(),
                    }
                )),
            }
        }
        messages
    }

    /// Every sigil any active framework understands, plus the default `$`.
    pub fn knows_sigil(&self, sigil: &str) -> bool {
        sigil == "$"
            || self.active.iter().any(|framework| framework.sigils().iter().any(|it| it == sigil))
    }
}
