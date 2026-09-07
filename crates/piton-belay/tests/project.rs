//! End-to-end checks of Belay's output, driven from a real project on disk.

use std::path::{Path, PathBuf};

use piton_belay::Belay;
use piton_core::builtin;
use piton_core::compile::compile;
use piton_core::db::Db;
use piton_core::framework::{Frameworks, OutputFile};
use piton_core::project::Project;

/// Write a throwaway project and build it.
fn build(files: &[(&str, &str)]) -> (PathBuf, Vec<OutputFile>, Vec<String>) {
    let root = std::env::temp_dir().join(format!(
        "piton-belay-test-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    for (path, contents) in files {
        let target = root.join(path);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, contents).unwrap();
    }

    let mut frameworks = Frameworks::new(vec![Box::new(Belay::new())]);
    let loaded = Project::load(&root, &frameworks);
    if let Some(configuration) = &loaded.compilation {
        frameworks.configure(&loaded.project, configuration, &loaded.project.framework_configs);
    }

    let mut db = Db::new();
    for module in builtin::modules().into_iter().chain(frameworks.modules()) {
        db.add_virtual_module(module.name, module.source);
    }
    db.set_root(&loaded.project.root);
    let entry = db.load(&loaded.project.entry).expect("entry loads");
    let compilation = compile(db, vec![entry], &frameworks);
    let messages: Vec<String> = compilation
        .diagnostics
        .iter()
        .filter(|it| it.is_error())
        .map(|it| it.message.clone())
        .collect();

    let mut outputs = Vec::new();
    for framework in &frameworks.active {
        outputs.extend(framework.emit(&compilation, &loaded.project).files);
    }
    (root, outputs, messages)
}

fn find<'a>(outputs: &'a [OutputFile], root: &Path, suffix: &str) -> &'a str {
    outputs
        .iter()
        .find(|file| file.path.strip_prefix(root).is_ok_and(|it| it.to_string_lossy() == suffix))
        .map(|file| file.contents.as_str())
        .unwrap_or_else(|| {
            let listing: Vec<String> = outputs
                .iter()
                .map(|file| file.path.strip_prefix(root).unwrap_or(&file.path).display().to_string())
                .collect();
            panic!("no output at {suffix}; produced: {listing:#?}")
        })
}

const CONFIG: &str = "\
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi

    frameworks:
        - {BelayConfiguration}

belay-config BelayConfiguration:
    codeRoot: ./src
    shapeRoot: ./spec/shape

    adapters:
        - {ClaudeAdapter}
";

#[test]
fn agents_skills_and_commands_land_in_the_agent_directory() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        ("spec/index.pi", "from ./Definitions export *\n"),
        (
            "spec/Definitions.pi",
            "\
use @piton/belay

export agent ReviewBot:
    description: Reviews a change
    role: careful reviewer
    prompt: Read the diff and comment
    model: opus
    tools: Read, Grep

    checklist:
        - Intent
        - States

export skill WriteSpec:
    description: Writes a specification
    useWhen: the user asks for a spec
    prompt: Write the specification

export command Ship:
    description: Ships the change
    prompt: Run the tests, then push
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");

    let agent = find(&outputs, &root, ".claude/agents/review-bot.md");
    assert!(agent.starts_with("---\nname: review-bot\n"), "{agent}");
    assert!(agent.contains("description: Reviews a change"), "{agent}");
    assert!(agent.contains("model: opus"), "{agent}");
    assert!(agent.contains("tools: Read, Grep"), "{agent}");
    assert!(agent.contains("You are a careful reviewer"), "{agent}");
    assert!(agent.contains("Read the diff and comment"), "{agent}");
    assert!(agent.contains("# Checklist"), "{agent}");
    assert!(agent.contains("- Intent"), "{agent}");

    let skill = find(&outputs, &root, ".claude/skills/write-spec/SKILL.md");
    assert!(
        skill.contains("description: Writes a specification Use when the user asks for a spec"),
        "{skill}"
    );

    // Commands are always prefixed so they cannot collide.
    let command = find(&outputs, &root, ".claude/commands/x-ship.md");
    assert!(command.contains("description: Ships the change"), "{command}");
    assert!(command.contains("Run the tests, then push"), "{command}");
}

#[test]
fn instructions_mirror_the_shape_tree_onto_the_code_tree() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/components/button/.keep", ""),
        ("spec/index.pi", "from ./shape export *\n"),
        ("spec/shape/index.pi", "from ./components/button export *\n"),
        (
            "spec/shape/components/button/index.pi",
            "from ./Button export *\nfrom ./Extra export *\n",
        ),
        (
            "spec/shape/components/button/Button.pi",
            "use @piton/belay\n\nexport instruction ButtonComponent:\n    description: The button\n    prompt: Make it clickable\n",
        ),
        (
            "spec/shape/components/button/Extra.pi",
            "use @piton/belay\n\nexport instruction ButtonStates:\n    description: The button states\n    prompt: Handle hover and disabled\n",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");

    // Files at the same shape scope concatenate into one AGENTS.md.
    let agents = find(&outputs, &root, "src/components/button/AGENTS.md");
    assert!(agents.contains("Make it clickable"), "{agents}");
    assert!(agents.contains("Handle hover and disabled"), "{agents}");
    assert_eq!(find(&outputs, &root, "src/components/button/CLAUDE.md"), "@AGENTS.md\n");

    // And each instruction is also published under the agent directory.
    find(&outputs, &root, ".claude/reference/shape/components/button/Button.md");
    find(&outputs, &root, ".claude/reference/shape/components/button/Extra.md");
}

#[test]
fn instructions_fall_back_to_the_nearest_real_directory() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        ("spec/index.pi", "from ./shape/nowhere/Deep export *\n"),
        (
            "spec/shape/nowhere/Deep.pi",
            "use @piton/belay\n\nexport instruction Deep:\n    description: No matching code directory\n    prompt: Applies to the whole project\n",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    let agents = find(&outputs, &root, "src/AGENTS.md");
    assert!(agents.contains("Applies to the whole project"), "{agents}");
    find(&outputs, &root, "src/CLAUDE.md");
}

#[test]
fn the_reference_sigil_links_to_the_compiled_file() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        ("spec/index.pi", "from ./Doc export *\n"),
        (
            "spec/Doc.pi",
            "\
use @piton/belay

export anchor Design:
    surface: Blue with contrasting text

export skill Build:
    description: Builds a component
    useWhen: asked to build
    prompt:
        Follow the design at @{Design} and name it ${Design}.
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    let skill = find(&outputs, &root, ".claude/skills/build/SKILL.md");
    assert!(skill.contains("@.claude/reference/Doc.md"), "{skill}");
    // `${}` on a complex value inserts the compiled name, not the structure.
    assert!(skill.contains("name it Design"), "{skill}");
    // The referenced anchor is published so the agent can actually read it.
    let reference = find(&outputs, &root, ".claude/reference/Doc.md");
    assert!(reference.contains("Blue with contrasting text"), "{reference}");
}

#[test]
fn the_shape_variable_points_at_the_compiled_shape_directory() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        ("spec/index.pi", "from ./Doc export *\n"),
        (
            "spec/Doc.pi",
            "\
use @piton/belay

from @piton/belay import __BELAY_SHAPE__

export skill Build:
    description: Builds a component
    useWhen: asked to build
    prompt: Build following the structure outlined in {__BELAY_SHAPE__}.
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    let skill = find(&outputs, &root, ".claude/skills/build/SKILL.md");
    assert!(skill.contains(".claude/reference/shape"), "{skill}");
}

#[test]
fn a_concrete_anchor_may_implement_only_one_belay_kind() {
    let (_root, _outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        ("spec/index.pi", "from ./Doc export *\n"),
        (
            "spec/Doc.pi",
            "\
use @piton/belay

from @piton/belay import Skill

export agent Confused extends Skill:
    description: Both at once
    role: confused
    prompt: Nothing
    useWhen: never
",
        ),
    ]);
    assert!(
        errors.iter().any(|message| message.contains("more than one abstract anchor")),
        "{errors:?}"
    );
}
