//! End-to-end checks of Belay's output, driven from a real project on disk.

mod common;

use common::{build, find, normalise, CONFIG};

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

    // And each instruction is also published under the agent directory.
    find(&outputs, &root, ".claude/reference/shape/components/button/ButtonComponent.md");
    find(&outputs, &root, ".claude/reference/shape/components/button/ButtonStates.md");
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
    // A Markdown link, not an `@` import: every agent can follow one, and it is
    // relative to the file it lands in rather than to the project root.
    let skill = find(&outputs, &root, ".claude/skills/build/SKILL.md");
    assert!(skill.contains("[Design](../../reference/Design.md)"), "{skill}");
    assert!(!skill.contains("(.claude/"), "a project-relative reference would not resolve");
    // And the document says what to do with the link, since nothing expands it.
    assert!(skill.contains("Read one when the work touches"), "{skill}");
    // `${}` on a complex value inserts the compiled name, not the structure.
    assert!(skill.contains("name it Design"), "{skill}");
    // The referenced anchor is published so the agent can actually read it.
    let reference = find(&outputs, &root, ".claude/reference/Design.md");
    assert!(reference.contains("Blue with contrasting text"), "{reference}");
}

#[test]
fn references_resolve_from_every_document_that_carries_them() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/components/button/.keep", ""),
        ("spec/index.pi", "from ./shape/components/button/Button export *\n"),
        (
            "spec/shape/components/button/Button.pi",
            "\
use @piton/belay

export anchor ButtonDesign:
    surface: Blue

export instruction ButtonComponent:
    description: The button
    prompt: For details, read @{ButtonDesign}.
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");

    // Same sentence, three documents, three different correct paths.
    let cases = [
        ("src/components/button/AGENTS.md", "../../../.claude/reference/shape/components/button/ButtonDesign.md"),
        (".claude/reference/shape/components/button/ButtonComponent.md", "ButtonDesign.md"),
    ];
    for (file, expected) in cases {
        let contents = find(&outputs, &root, file);
        assert!(contents.contains(&format!("[ButtonDesign]({expected})")), "{file}:\n{contents}");
        // And the path has to actually land on a file that gets written.
        let resolved = normalise(&root.join(file).parent().unwrap().join(expected));
        let published = outputs.iter().any(|output| normalise(&output.path) == resolved);
        assert!(published, "{file} points at {}, which is never written", resolved.display());
    }
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
    // The path is relative to the document that ended up carrying it, which
    // is what makes it usable from wherever the agent reads it.
    let skill = find(&outputs, &root, ".claude/skills/build/SKILL.md");
    assert!(skill.contains("../../reference/shape"), "{skill}");
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

#[test]
fn a_reference_stays_text_even_with_no_adapter_configured() {
    // No `belay-config`, so there is no compiled file to point at. The
    // reference still has to render as text: letting it fall through would
    // splice the anchor into the document and break the `:: string` around it.
    let (root, outputs, errors) = build(&[
        (
            "piton.config.pi",
            "use @piton/config\n\nexport piton-config Config:\n    root: ./spec\n    entry: ./spec/index.pi\n",
        ),
        ("spec/index.pi", "from ./Doc export *\n"),
        (
            "spec/Doc.pi",
            "\
use @piton/belay

export anchor Design:
    surface: Blue

export skill Build:
    description: Builds a component
    useWhen: asked to build
    prompt: Follow @{Design}.
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");
    // Nothing is emitted without an adapter, but the compile has to be clean.
    let _ = (root, outputs);
}

#[test]
fn every_adapter_points_at_its_own_copy_of_a_reference() {
    let (root, outputs, errors) = build(&[
        (
            "piton.config.pi",
            "\
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter, OpenCodeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi

    frameworks:
        - {BelayConfiguration}

belay-config BelayConfiguration:
    codeRoot: ./src
    shapeRoot: ./spec

    adapters:
        - {ClaudeAdapter}
        - {OpenCodeAdapter}
",
        ),
        ("src/.keep", ""),
        (
            "spec/index.pi",
            "\
use @piton/belay

export anchor Design:
    surface: Blue

export skill Build:
    description: Builds it
    useWhen: asked
    prompt: Follow @{Design}.
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");

    // Each adapter writes its own copy of everything, so each has to point at
    // the copy beside it rather than at whichever adapter compiled first.
    for directory in [".claude", ".opencode"] {
        let skill = find(&outputs, &root, &format!("{directory}/skills/build/SKILL.md"));
        let reference = skill
            .split_once("](")
            .and_then(|(_, tail)| tail.split_once(')'))
            .map(|(target, _)| target)
            .expect("the skill carries a reference");
        assert!(
            !reference.contains(".claude") && !reference.contains(".opencode"),
            "{directory} still names an agent directory: {reference}"
        );
        let resolved =
            normalise(&root.join(directory).join("skills/build").join(reference));
        assert!(
            outputs.iter().any(|output| normalise(&output.path) == resolved),
            "{directory} points at {}, which is never written",
            resolved.display()
        );
    }
}
