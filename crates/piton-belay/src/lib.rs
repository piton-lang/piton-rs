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

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use piton_core::compile::Compilation;
use piton_core::diag::Diagnostic;
use piton_core::framework::{Emitted, Framework, Interpolation, VirtualModule};
use piton_core::hir::lower_hir;
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

/// Every `.pi` file under `root` that declares a self-instruction.
///
/// Only the declaration is looked at: a file is parsed when its text mentions a
/// self-instruction at all, and chosen when one of its anchors is declared with
/// the keyword or names the anchor as a base.
fn self_instruction_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .map(|entry| entry.into_path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pi"))
        .filter(|path| {
            let Ok(text) = std::fs::read_to_string(path) else { return false };
            if !text.contains(module::SELF_INSTRUCTION_KEYWORD)
                && !text.contains(module::SELF_INSTRUCTION_ANCHOR)
            {
                return false;
            }
            let hir = lower_hir(&piton_syntax::parse(&text).root());
            hir.anchors.iter().any(|anchor| {
                anchor
                    .via_keyword
                    .as_ref()
                    .is_some_and(|keyword| keyword.value == module::SELF_INSTRUCTION_KEYWORD)
                    || anchor.bases.iter().any(|base| base.value == module::SELF_INSTRUCTION_ANCHOR)
            })
        })
        .collect();
    files.sort();
    files
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

    /// `@{X}` links the agent to the compiled file; `${X}` names it.
    ///
    /// The link is project-relative here, because evaluation runs before anyone
    /// knows which document the text lands in. The emitter rewrites it relative
    /// to that document, and anything else that renders a value — `piton
    /// compile`, an editor hover — still shows a path that means something.
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
                    Some(path) => format!("[{}]({})", anchor.name, path.display()),
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

    /// Every file under the project root that declares a self-instruction.
    ///
    /// A self-instruction is addressed by the directory it sits in, so nothing
    /// has to import it.
    fn roots(&self, project: &Project) -> Vec<PathBuf> {
        if !project.has_root() {
            return Vec::new();
        }
        self_instruction_files(&project.root)
    }

    fn emit(&self, compilation: &Compilation, project: &Project) -> Emitted {
        let mut plan = emit::Plan::discover(compilation);
        let referenced = self.referenced.lock().unwrap().clone();
        let mut emitted = Emitted::default();

        // Declaring a `belay-config` without listing it under `frameworks`
        // leaves Belay unconfigured, which used to mean a silent `wrote 0
        // files`. Say so instead.
        if self.settings.adapters.is_empty() && !plan.is_empty() {
            emitted.diagnostics.push(unconfigured(compilation, project));
            return emitted;
        }
        emitted
            .diagnostics
            .extend(emit::misplaced_self_instructions(compilation, &self.settings, &mut plan));

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
