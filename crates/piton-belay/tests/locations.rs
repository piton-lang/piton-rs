//! Checks the location exports Belay resolves while writing.
//!
//! `BELAY_COMPILED_SHAPE`, `BELAY_AGENT_ROOT`, `BELAY_PROJECT_ROOT`,
//! `BELAY_SHAPE_ROOT` and `BELAY_CODE_ROOT` name directories a generated
//! artifact has to point at. None can be decided while evaluating: the agent
//! root depends on which adapter is being compiled, and every one of them is
//! written relative to the file that carries it. So evaluation leaves a marker
//! and Belay substitutes a path per file, which is what these tests hold.

use std::path::PathBuf;

use piton_compile::{config, BelayConfig, Compilation, Framework};

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

/// The skill every test compiles: one line per location export, so a failure
/// names which root moved.
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

    frameworks:
        - {{SandboxBelay}}

belay-config SandboxBelay:
    codeRoot: ./src
{shape}
    adapters:
        - {{{adapter}}}
"
    )
}

/// Builds the sandbox project and returns the generated skill, keyed by the
/// location name each line reports.
fn locations(sandbox: &Sandbox) -> Vec<(String, String)> {
    let (project, diagnostics) = config::load(&sandbox.dir, None);
    assert!(
        !diagnostics.has_errors(),
        "configuration did not load: {:?}",
        diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    let belay: BelayConfig = project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        })
        .expect("belay configured");
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

    let plan = piton_belay::plan(&compilation, &belay);
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

/// No marker survives into a generated file, whatever it resolved to.
fn no_markers_remain(sandbox: &Sandbox) {
    let (project, _) = config::load(&sandbox.dir, None);
    let belay = project
        .frameworks
        .iter()
        .find_map(|framework| match framework {
            Framework::Belay(config) => Some(config.clone()),
        })
        .expect("belay configured");
    let compilation = Compilation::build(project);
    let plan = piton_belay::plan(&compilation, &belay);
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

    // The skill lands at .claude/skills/locations/SKILL.md, so every path is
    // written from two directories below the project root.
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

    // .opencode/skills/locations/SKILL.md sits at the same depth, so only the
    // directory the agent root names changes -- and it is that directory, not
    // a path that still says `.claude`.
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
