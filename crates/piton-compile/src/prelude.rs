//! Built-in packages.
//!
//! `@piton/config` and `@piton/belay` are bundled with the compiler and resolve
//! from a virtual filesystem rather than from disk, so a project gets the same
//! vocabulary wherever it is checked out.
//!
//! The four agentic constructs are not written here. They are the `.pi` files in
//! `spec/scope/belay/anchors`, embedded at build time, so the specification's
//! description of a skill *is* the skill the compiler ships. Changing
//! `Skill.pi` changes the framework; there is no second copy to keep in step.
//!
//! What stays in this file is the part with no such counterpart. The adapter
//! contract the specification describes is prose about how each target behaves;
//! the compiler needs machine-readable target configuration, carrying fields the
//! prose contract does not. Those are different artifacts that describe the same
//! thing, so they are written separately.

/// Module path for the project configuration package.
pub const CONFIG_PACKAGE: &str = "@piton/config";
/// Module path for the Belay framework package.
pub const BELAY_PACKAGE: &str = "@piton/belay";

/// The special export that resolves to the compiled shape root of the current
/// output target.
pub const BELAY_SHAPE: &str = "__BELAY_SHAPE__";

/// The marker `__BELAY_SHAPE__` evaluates to.
///
/// Its value cannot be decided during evaluation: the path depends on the
/// adapter *and* on which generated file contains the use. Evaluation therefore
/// produces this marker and each adapter replaces it with a path relative to
/// the file being written.
///
/// The private-use bracket characters are what keep it from colliding with
/// prose: a document is free to discuss `__BELAY_SHAPE__` by name without being
/// rewritten.
pub const BELAY_SHAPE_TOKEN: &str = "\u{e000}__BELAY_SHAPE__\u{e001}";

/// Placeholder in the package template, swapped for the real marker at load.
const BELAY_SHAPE_PLACEHOLDER: &str = "__BELAY_SHAPE_MARKER__";

/// `@piton/config`: the anchors a `piton.config.pi` file is built from.
pub const CONFIG_SOURCE: &str = r#"// Bundled with the Piton compiler.

export abstract anchor PitonConfig as piton-config:
    root:: string
    entry:: string:: null: null
    frameworks:: list:: null: null

export abstract anchor FrameworkConfig as framework-config:
    pass
"#;

// The framework's own definitions, taken from the specification that describes
// them. A build fails loudly if one of these files moves, which is the point:
// the compiler and the specification cannot disagree about what a skill is.
const BELAY_CONSTRUCT: &str = include_str!("../../../spec/scope/belay/anchors/Construct.pi");
const BELAY_INSTRUCTION: &str = include_str!("../../../spec/scope/belay/anchors/Instruction.pi");
const BELAY_SKILL: &str = include_str!("../../../spec/scope/belay/anchors/Skill.pi");
const BELAY_COMMAND: &str = include_str!("../../../spec/scope/belay/anchors/Command.pi");
const BELAY_AGENT: &str = include_str!("../../../spec/scope/belay/anchors/Agent.pi");
/// The constructs extend `FrameworkSpecification`, which lives here.
const BELAY_FRAMEWORK: &str = include_str!("../../../spec/scope/belay/Framework.pi");

/// `@piton/belay`: re-exports the constructs, and adds the target configuration
/// the specification's prose contract does not carry.
const BELAY_INDEX: &str = r#"// Bundled with the Piton compiler.
//
// The four constructs come from the specification that describes them. Only
// their names are listed here, so the package exports the framework without
// also exporting the documentation that surrounds it.

use @piton/config

from @piton/config import FrameworkConfig

from ./anchors/Construct export Construct
from ./anchors/Instruction export Instruction
from ./anchors/Skill export Skill
from ./anchors/Command export Command
from ./anchors/Agent export Agent

export abstract anchor Adapter as belay-agent-adapter:
    targetId:: string
    instructionFile:: string
    referenceRoot:: string
    skillRoot:: string
    agentRoot:: string
    commandSupport:: string

export anchor ClaudeAdapter extends Adapter:
    description: Compile Belay constructs into project-local Claude Code artifacts.
    targetId: claude-code
    instructionFile: CLAUDE.md
    referenceRoot: .claude/reference
    skillRoot: .claude/skills
    agentRoot: .claude/agents
    commandSupport: skill

export anchor CodexAdapter extends Adapter:
    description: Compile Belay constructs into project-local Codex artifacts.
    targetId: codex
    instructionFile: AGENTS.md
    referenceRoot: .codex/reference
    skillRoot: .agents/skills
    agentRoot: .codex/agents
    commandSupport: skill

export anchor OpenCodeAdapter extends Adapter:
    description: Compile Belay constructs into project-local OpenCode artifacts.
    targetId: opencode
    instructionFile: AGENTS.md
    referenceRoot: .opencode/reference
    skillRoot: .opencode/skills
    agentRoot: .opencode/agents
    commandSupport: native

// Resolved during compilation, separately for each adapter and each output
// location. Never resolved at agent runtime.
export __BELAY_SHAPE__: __BELAY_SHAPE_MARKER__

export abstract anchor BelayConfig extends FrameworkConfig as belay-config:
    codeRoot:: string
    shapeRoot:: string:: null: null
    adapters:: list
"#;

/// Every module a bundled package provides, keyed by its virtual path.
///
/// The paths carry no extension, matching how a resolved module path is
/// spelled, so `from ./anchors/Skill export Skill` inside the package resolves
/// exactly as it would on disk.
fn modules() -> &'static [(&'static str, &'static str)] {
    static MODULES: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
        std::sync::OnceLock::new();
    MODULES
        .get_or_init(|| {
            let index: &'static str = Box::leak(
                BELAY_INDEX
                    .replace(BELAY_SHAPE_PLACEHOLDER, BELAY_SHAPE_TOKEN)
                    .into_boxed_str(),
            );
            vec![
                (CONFIG_PACKAGE, CONFIG_SOURCE),
                (BELAY_PACKAGE, index),
                ("@piton/belay/Framework", BELAY_FRAMEWORK),
                ("@piton/belay/anchors/Construct", BELAY_CONSTRUCT),
                ("@piton/belay/anchors/Instruction", BELAY_INSTRUCTION),
                ("@piton/belay/anchors/Skill", BELAY_SKILL),
                ("@piton/belay/anchors/Command", BELAY_COMMAND),
                ("@piton/belay/anchors/Agent", BELAY_AGENT),
            ]
        })
        .as_slice()
}

/// Returns the source of a bundled module, if `path` names one.
pub fn package_source(path: &str) -> Option<&'static str> {
    modules()
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, source)| *source)
}

/// True when `path` names a module inside a bundled package.
pub fn is_package(path: &str) -> bool {
    package_source(path).is_some()
}

/// True when `path` lies inside a bundled package, whether or not it names a
/// module. Used to keep package internals out of user-facing reports.
pub fn is_package_path(path: &str) -> bool {
    path.starts_with('@')
}

/// The packages a project may import by name.
pub const PACKAGE_ROOTS: [&str; 2] = [CONFIG_PACKAGE, BELAY_PACKAGE];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constructs_come_from_the_specification() {
        // Not a copy of them: the same bytes the specification is written in.
        let skill = package_source("@piton/belay/anchors/Skill").expect("module");
        assert!(
            skill.contains("abstract anchor Skill extends Construct as skill"),
            "{skill}"
        );
        assert_eq!(skill, BELAY_SKILL);
    }

    #[test]
    fn every_package_module_is_addressable() {
        for (path, source) in modules() {
            assert!(is_package(path), "{path}");
            assert!(is_package_path(path), "{path}");
            assert!(!source.is_empty(), "{path}");
        }
    }

    #[test]
    fn the_shape_marker_is_substituted_once() {
        let index = package_source(BELAY_PACKAGE).expect("index");
        assert!(index.contains(BELAY_SHAPE_TOKEN));
        assert!(!index.contains(BELAY_SHAPE_PLACEHOLDER));
    }
}
