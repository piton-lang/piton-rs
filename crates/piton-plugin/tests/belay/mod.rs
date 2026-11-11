//! Plans a compilation with the bundled Belay plugin, as `piton build` does.
//! It gives the tests a small view of the plan.

#![allow(dead_code)]

use std::path::PathBuf;
use std::rc::Rc;

use piton_compile::{Compilation, Project};
use piton_core::Diagnostic;
use piton_plugin::output::BuildPlan;
use piton_plugin::{Services, Stream};

/// The parts of a CLI that the plugin host needs. It prints nothing.
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
        0
    }
}

/// One file that the plugin plans.
#[derive(Debug, Clone)]
pub struct OutputFile {
    pub path: PathBuf,
    /// The contents as text. Binary copies lose some data.
    pub contents: String,
    pub data: Vec<u8>,
    /// The Belay target id. It is the `group` of the plan.
    pub target: String,
    /// The output kind from the label, for example `skill` or `skill file`.
    pub kind: String,
    pub origin: String,
    pub executable: bool,
}

impl OutputFile {
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// The plan of the Belay plugin for a compilation.
pub struct Plan {
    pub files: Vec<OutputFile>,
    pub diagnostics: Vec<Diagnostic>,
    pub targets: Vec<(String, String)>,
    /// The plan, as the host writes it.
    pub build: BuildPlan,
}

/// Tells the bundled Belay plugin to plan `compilation`.
pub fn plan(compilation: &Compilation) -> Plan {
    let project = compilation.project.clone();
    let (manifests, problems) = piton_plugin::discover(&project);
    assert!(problems.is_empty(), "plugins did not load: {problems:?}");
    let manifest = manifests.iter().find(|m| m.name == "belay").expect("the Belay plugin is bundled");
    let services = Rc::new(Quiet { project: project.clone() });
    let planned = match piton_plugin::run_build(&manifests, "belay", services) {
        Ok(planned) => planned,
        Err(failure) => panic!("the Belay plugin failed: {failure}"),
    };
    let read = piton_plugin::plan::read(manifest, &planned, &project.root)
        .unwrap_or_else(|message| panic!("the Belay plugin's plan is malformed: {message}"));
    let files = read
        .files
        .iter()
        .map(|file| {
            let kind = file.label.rsplit_once(", ").map_or(file.label.as_str(), |(kind, _)| kind);
            OutputFile {
                path: PathBuf::from(&file.path),
                contents: String::from_utf8_lossy(&file.data).into_owned(),
                data: file.data.clone(),
                target: file.group.clone().unwrap_or_default(),
                kind: kind.to_string(),
                origin: file.origin.clone(),
                executable: file.executable,
            }
        })
        .collect();
    Plan {
        files,
        diagnostics: read.diagnostics,
        targets: read.targets.clone(),
        build: BuildPlan { files: read.files, targets: read.targets },
    }
}
