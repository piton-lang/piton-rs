//! Examines the location exports that Belay resolves when it writes files.
//!
//! `BELAY_COMPILED_SHAPE`, `BELAY_AGENT_ROOT`, `BELAY_PROJECT_ROOT`,
//! `BELAY_SHAPE_ROOT` and `BELAY_CODE_ROOT` name directories. A generated
//! artifact points to these directories. Evaluation cannot find their values.
//! The agent root changes with the adapter that Belay compiles. Belay writes
//! each path relative to the file that holds it. Thus evaluation leaves a
//! marker. Belay replaces the marker with a path for each file. These tests
//! examine that replacement.

mod belay;

use std::path::PathBuf;

use piton_compile::{config, Compilation};

struct Sandbox {
    dir: PathBuf,
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!(
            "piton-locations-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        Sandbox { dir }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.dir.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent");
        }
        std::fs::write(path, contents).expect("write");
    }
}

/// The skill that each test compiles. It has one line for each location
/// export. Thus a failure names the root that changed.
const SKILL: &str = "\
use @piton/belay

from @piton/belay import
    BELAY_AGENT_ROOT,
    BELAY_CODE_ROOT,
    BELAY_PROJECT_ROOT,
    BELAY_SHAPE_ROOT,
    BELAY_COMPILED_SHAPE

export skill Locations:
    description: Reports where each Belay root resolved.
    useWhen: Explicitly invoked
    prompt:
        agent ${BELAY_AGENT_ROOT}

        project ${BELAY_PROJECT_ROOT}

        shape ${BELAY_SHAPE_ROOT}

        code ${BELAY_CODE_ROOT}

        compiled ${BELAY_COMPILED_SHAPE}
";

fn configuration(adapter: &str, shape_root: Option<&str>) -> String {
    let shape = shape_root
        .map(|root| format!("    shapeRoot: {root}\n"))
        .unwrap_or_default();
    format!(
        "use @piton/config
use @piton/belay

from @piton/belay import {adapter}

export piton-config Sandbox:
    root: ./spec
    entry: ./spec/index.pi

    plugins:
        - {{SandboxBelay}}

belay-config SandboxBelay:
    codeRoot: ./src
{shape}
    adapters:
        - {{{adapter}}}
"
    )
}

/// Builds the sandbox project and returns the generated skill. The key of each
/// line is the location name that the line shows.
fn locations(sandbox: &Sandbox) -> Vec<(String, String)> {
    let (project, diagnostics) = config::load(&sandbox.dir, None);
    assert!(
        !diagnostics.has_errors(),
        "configuration did not load: {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(!project.plugins.is_empty(), "belay configured");
    let compilation = Compilation::build(project);
    assert!(
        !compilation.has_errors(),
        "sandbox did not compile: {:?}",
        compilation
            .diagnostics
            .iter()
            .map(|d| &d.message)
            .collect::<Vec<_>>()
    );

    let plan = belay::plan(&compilation);
    let skill = plan
        .files
        .iter()
        .find(|file| file.path.ends_with("SKILL.md"))
        .expect("a skill was planned");

    skill
        .contents
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(name, _)| {
            ["agent", "project", "shape", "code", "compiled"].contains(name)
        })
        .map(|(name, path)| (name.to_string(), path.to_string()))
        .collect()
}

/// No marker stays in a generated file. This is true for each value.
fn no_markers_remain(sandbox: &Sandbox) {
    let (project, _) = config::load(&sandbox.dir, None);
    assert!(!project.plugins.is_empty(), "belay configured");
    let compilation = Compilation::build(project);
    let plan = belay::plan(&compilation);
    for file in &plan.files {
        assert!(
            !file.contents.contains('\u{e000}'),
            "{} still carries a location marker",
            file.path.display()
        );
    }
}

#[test]
fn every_root_resolves_relative_to_the_file_that_names_it() {
    let sandbox = Sandbox::new("claude");
    sandbox.write("piton.config.pi", &configuration("ClaudeCodeAdapter", Some("./spec/shape")));
    sandbox.write("spec/index.pi", SKILL);
    std::fs::create_dir_all(sandbox.dir.join("src")).expect("code root");

    // Belay writes the skill to .claude/skills/locations/SKILL.md. Thus each
    // path starts two directories below the project root.
    let found = locations(&sandbox);
    assert_eq!(
        found,
        vec![
            ("agent".to_string(), "../..".to_string()),
            ("project".to_string(), "../../..".to_string()),
            ("shape".to_string(), "../../../spec/shape".to_string()),
            ("code".to_string(), "../../../src".to_string()),
            ("compiled".to_string(), "../../reference/shape".to_string()),
        ]
    );
    no_markers_remain(&sandbox);
}

#[test]
fn the_agent_root_follows_the_adapter() {
    let sandbox = Sandbox::new("opencode");
    sandbox.write("piton.config.pi", &configuration("OpenCodeAdapter", Some("./spec/shape")));
    sandbox.write("spec/index.pi", SKILL);
    std::fs::create_dir_all(sandbox.dir.join("src")).expect("code root");

    // .opencode/skills/locations/SKILL.md has the same depth. Thus only the
    // directory of the agent root changes. That directory is `.opencode`, not
    // `.claude`.
    let found = locations(&sandbox);
    let agent = found
        .iter()
        .find(|(name, _)| name == "agent")
        .expect("agent root");
    assert_eq!(agent.1, "../..");

    let compiled = found
        .iter()
        .find(|(name, _)| name == "compiled")
        .expect("compiled shape");
    assert_eq!(compiled.1, "../../reference/shape");

    no_markers_remain(&sandbox);
}

#[test]
fn an_unconfigured_shape_root_falls_back_to_the_project_root() {
    let sandbox = Sandbox::new("noshape");
    sandbox.write("piton.config.pi", &configuration("ClaudeCodeAdapter", None));
    sandbox.write("spec/index.pi", SKILL);
    std::fs::create_dir_all(sandbox.dir.join("src")).expect("code root");

    let found = locations(&sandbox);
    let shape = found
        .iter()
        .find(|(name, _)| name == "shape")
        .expect("shape root");
    let project = found
        .iter()
        .find(|(name, _)| name == "project")
        .expect("project root");
    assert_eq!(shape.1, project.1);
    no_markers_remain(&sandbox);
}
