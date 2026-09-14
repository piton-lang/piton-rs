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
fn an_instruction_or_self_instruction_needs_only_a_prompt() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/button/.keep", ""),
        ("spec/index.pi", "from ./shape/button/Button export *\n"),
        ("spec/shape/button/Button.pi", "use @piton/belay\n\nexport instruction Button:\n    prompt: Make it clickable\n"),
        ("spec/scope/Guide.pi", "use @piton/belay\n\nexport self-instruction Guide:\n    prompt: Keep it short\n"),
    ]);
    assert!(errors.is_empty(), "{errors:?}");

    // The empty default leaves no stray paragraph or section behind.
    for (file, prompt) in [("src/button/AGENTS.md", "Make it clickable"), ("spec/scope/AGENTS.md", "Keep it short")] {
        let heading = if prompt.starts_with("Make") { "# Button" } else { "# Guide" };
        assert_eq!(find(&outputs, &root, file), format!("{heading}\n\n{prompt}\n"), "{file}");
    }
}

#[test]
fn a_description_must_still_be_a_string() {
    let (_root, _outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        ("spec/index.pi", "use @piton/belay\n\nexport instruction Listy:\n    description:\n        - not\n        - a string\n    prompt: p\n"),
    ]);
    assert!(!errors.is_empty(), "a list description should be rejected");
}

#[test]
fn self_instructions_compile_beside_themselves_without_being_reached() {
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        // Nothing imports the self-instructions.
        ("spec/index.pi", "export anchor Nothing:\n    here: true\n"),
        (
            "spec/scope/lsp/Guide.pi",
            "use @piton/belay\n\nexport self-instruction LspGuide:\n    description: How the LSP scope is written\n    prompt: Keep one feature per file\n",
        ),
        (
            "spec/scope/lsp/Naming.pi",
            "use @piton/belay\n\nfrom @piton/belay import SelfInstruction\n\nanchor LspNaming extends SelfInstruction:\n    description: Names\n    prompt: Name files after the feature\n",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");

    // Everything in one directory concatenates, whichever spelling declared it.
    let agents = find(&outputs, &root, "spec/scope/lsp/AGENTS.md");
    assert!(agents.starts_with("# Lsp Guide\n"), "{agents}");
    assert!(agents.contains("Keep one feature per file"), "{agents}");
    assert!(agents.contains("Name files after the feature"), "{agents}");

    // It is about the specification, so nothing lands in the code tree, and
    // it is not shape, so nothing is published beside the shape references.
    let paths: Vec<String> = outputs
        .iter()
        .map(|it| it.path.strip_prefix(&root).unwrap().display().to_string())
        .collect();
    assert!(!paths.iter().any(|it| it.starts_with("src/")), "{paths:#?}");
    assert!(!paths.iter().any(|it| it.starts_with(".claude/")), "{paths:#?}");
}

#[test]
fn only_the_claude_adapter_writes_claude_files() {
    let config = |adapters: &str| {
        format!(
            "\
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter, OpenCodeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi

    frameworks:
        - {{BelayConfiguration}}

belay-config BelayConfiguration:
    codeRoot: ./src
    shapeRoot: ./spec/shape

    adapters:
{adapters}
"
        )
    };
    let files = |config: &str| {
        build(&[
            ("piton.config.pi", config),
            ("src/button/.keep", ""),
            ("spec/index.pi", "from ./shape/button/Button export *\n"),
            ("spec/shape/button/Button.pi", "use @piton/belay\n\nexport instruction Button:\n    prompt: Click\n"),
            ("spec/scope/Guide.pi", "use @piton/belay\n\nexport self-instruction Guide:\n    prompt: Short\n"),
        ])
    };

    // OpenCode reads AGENTS.md itself, so a project that only targets it gets
    // nothing Claude-specific in its code or spec tree.
    let (root, outputs, errors) = files(&config("        - {OpenCodeAdapter}"));
    assert!(errors.is_empty(), "{errors:?}");
    find(&outputs, &root, "src/button/AGENTS.md");
    find(&outputs, &root, "spec/scope/AGENTS.md");
    assert!(!outputs.iter().any(|it| it.path.ends_with("CLAUDE.md")), "OpenCode alone wrote a CLAUDE.md");

    // Spelled as a directory rather than the ready-made anchor, it is still Claude.
    let (root, outputs, errors) = files(&config(
        "        - {Claude}\n\nbelay-agent-adapter Claude:\n    description: d\n    directory: .claude",
    ));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(find(&outputs, &root, "src/button/CLAUDE.md"), "@AGENTS.md\n");
    assert_eq!(find(&outputs, &root, "spec/scope/CLAUDE.md"), "@AGENTS.md\n");
}

#[test]
fn a_self_instruction_outside_the_project_root_is_an_error() {
    let (root, outputs, errors) = build(&[
        (
            "piton.config.pi",
            "\
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi

    frameworks:
        - {BelayConfiguration}

    libraries:
        customLib: ./lib

belay-config BelayConfiguration:
    codeRoot: ./src

    adapters:
        - {ClaudeAdapter}
",
        ),
        ("src/.keep", ""),
        ("spec/index.pi", "from /customLib/Guide export *\n"),
        (
            "lib/Guide.pi",
            "use @piton/belay\n\nexport self-instruction Borrowed:\n    description: Lives in a library\n    prompt: Should never be written\n",
        ),
    ]);
    assert!(
        errors.iter().any(|it| it.contains("`Borrowed` is a self-instruction") && it.contains("project root")),
        "{errors:?}"
    );
    assert!(
        !outputs.iter().any(|it| it.contents.contains("Should never be written")),
        "{:#?}",
        outputs.iter().map(|it| it.path.strip_prefix(&root).unwrap().to_path_buf()).collect::<Vec<_>>()
    );
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
fn two_documents_may_reference_each_other() {
    // The pair that the specification calls out: each file imports the other
    // and each skill points at the other's compiled document. Nothing here is
    // a cycle, because a reference settles to a name and a link.
    let (root, outputs, errors) = build(&[
        ("piton.config.pi", CONFIG),
        ("src/.keep", ""),
        ("spec/index.pi", "from ./Build export *\n"),
        (
            "spec/Build.pi",
            "\
use @piton/belay

from ./Ship import Ship

export skill Build:
    description: Builds the change
    useWhen: asked to build
    prompt: Build it, then hand over to ${Ship}, which is at @{Ship}.
",
        ),
        (
            "spec/Ship.pi",
            "\
use @piton/belay

from ./Build import Build

export skill Ship:
    description: Ships the change
    useWhen: asked to ship
    prompt: Ship it, after ${Build}, which is at @{Build}.
",
        ),
    ]);
    assert!(errors.is_empty(), "{errors:?}");

    let build_skill = find(&outputs, &root, ".claude/skills/build/SKILL.md");
    assert!(build_skill.contains("hand over to Ship"), "{build_skill}");
    assert!(build_skill.contains("[Ship]("), "{build_skill}");

    let ship_skill = find(&outputs, &root, ".claude/skills/ship/SKILL.md");
    assert!(ship_skill.contains("after Build"), "{ship_skill}");
    assert!(ship_skill.contains("[Build]("), "{ship_skill}");
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
