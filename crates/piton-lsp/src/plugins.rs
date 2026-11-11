//! Plugin builds, as the editor sees them.
//!
//! The editor plans the build of a plugin when the user saves a file. It never
//! plans a build while the user types. Thus a keystroke does not run plugin code.
//! The editor keeps the plans until the next save. Their diagnostics show
//! next to the compiler diagnostics. The editor preview and the source map
//! show the files that the plans will write.

use std::rc::Rc;

use piton_compile::{Compilation, Project};
use piton_core::{Diagnostic, Span};
use piton_plugin::plan::PluginPlan;
use piton_plugin::{Services, Stream};

/// The requests of the plugin host to an editor. The editor prints nothing.
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

/// Plans each plugin build that the project enabled. When a plugin fails, or
/// makes a plan that is not correct, the configuration shows an error. The
/// other plugins continue to plan.
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

/// The anchors that the plugins of the project use, by file and name. An
/// example is the tests that the test plugin runs. They are roots of the
/// unused check, together with the entry. The editor asks for them when the
/// user saves a file, as for the build plans. When a plugin cannot supply
/// them, the result does not include that plugin. The other plugins still
/// count.
pub fn reached(project: &Project) -> Vec<(std::path::PathBuf, String)> {
    if project.config_path.is_none() {
        return Vec::new();
    }
    let (manifests, _) = piton_plugin::discover(project);
    let (roots, _) = piton_plugin::reach::roots(project, &manifests, || Rc::new(Quiet { project: project.clone() }));
    roots.into_iter().map(|root| (root.file, root.name)).collect()
}
