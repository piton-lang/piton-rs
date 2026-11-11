//! Plugin builds, as the editor sees them.
//!
//! A plugin's build is planned when a file is saved, never as you type, so
//! typing stays free of plugin code. The plans are kept until the next save:
//! their diagnostics show beside the compiler's, and the editor's preview and
//! source map show what they would write.

use std::rc::Rc;

use piton_compile::{Compilation, Project};
use piton_core::{Diagnostic, Span};
use piton_plugin::plan::PluginPlan;
use piton_plugin::{Services, Stream};

/// What the plugin host asks of an editor: nothing is printed.
struct Quiet {
    project: Project,
}

impl Services for Quiet {
    fn project(&self) -> &Project {
        &self.project
    }
    fn write(&self, _: Stream, _: &str) {}
    fn paint(&self, _: &str, text: &str) -> String {
        text.to_string()
    }
    fn report(&self, _: &str, _: &str) {}
    fn diagnostic(&self, _: Diagnostic) {}
    fn compilation_diagnostics(&self, _: &Compilation) -> bool {
        false
    }
    fn slice(&self, _: &Compilation, _: &str, _: &dyn piton_emit::markdown::LinkResolver) -> u8 {
        1
    }
}

/// Plans every plugin build the project turned on. A plugin that fails, or
/// plans something malformed, is reported on the configuration, and the
/// others still plan.
pub fn plan(project: &Project) -> Vec<PluginPlan> {
    if project.config_path.is_none() {
        return Vec::new();
    }
    let (manifests, _) = piton_plugin::discover(project);
    let mut plans = Vec::new();
    for manifest in piton_plugin::builders(project, &manifests) {
        let services = Rc::new(Quiet { project: project.clone() });
        let failure = match piton_plugin::run_build(&manifests, &manifest.name, services) {
            Ok(planned) => match piton_plugin::plan::read(manifest, &planned, &project.root) {
                Ok(plan) => {
                    plans.push(plan);
                    continue;
                }
                Err(message) => message,
            },
            Err(failure) => failure.to_string(),
        };
        plans.push(PluginPlan {
            diagnostics: vec![Diagnostic::error(
                "plugin-failed",
                format!("plugin `{}` failed to plan its build: {failure}", manifest.name),
                project.config_path.clone().unwrap_or_default(),
                Span::default(),
            )],
            ..PluginPlan::default()
        });
    }
    plans
}

/// The anchors the project's plugins reach themselves, by file and name, like
/// the tests the test plugin runs. They are roots of the unused check beside
/// the entry. Asked when a file is saved, like the build plans; a plugin that
/// can't say is left out, and the others still count.
pub fn reached(project: &Project) -> Vec<(std::path::PathBuf, String)> {
    if project.config_path.is_none() {
        return Vec::new();
    }
    let (manifests, _) = piton_plugin::discover(project);
    let (roots, _) = piton_plugin::reach::roots(project, &manifests, || Rc::new(Quiet { project: project.clone() }));
    roots.into_iter().map(|root| (root.file, root.name)).collect()
}
