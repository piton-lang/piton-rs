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
//! The adapters are embedded the same way, from `spec/scope/belay/adapters`, so
//! the anchor a project lists in its `belay-config` is the adapter the
//! specification describes. What stays in this file is the part with no such
//! counterpart: the configuration anchor and the special imports.

/// Module path for the project configuration package.
pub const CONFIG_PACKAGE: &str = "@piton/config";
/// Module path for the Belay framework package.
pub const BELAY_PACKAGE: &str = "@piton/belay";
/// Module path for the package-management package.
pub const PACKAGING_PACKAGE: &str = "@piton/packaging";

/// The special export that resolves to the compiled shape root of the current
/// output target.
pub const BELAY_SHAPE: &str = "BELAY_COMPILED_SHAPE";

/// The marker `BELAY_COMPILED_SHAPE` evaluates to.
///
/// Its value cannot be decided during evaluation: the path depends on the
/// adapter *and* on which generated file contains the use. Evaluation therefore
/// produces this marker and each adapter replaces it with a path relative to
/// the file being written.
///
/// The private-use bracket characters are what keep it from colliding with
/// prose: a document is free to discuss `BELAY_COMPILED_SHAPE` by name without
/// being rewritten.
pub const BELAY_SHAPE_TOKEN: &str = "\u{e000}BELAY_COMPILED_SHAPE\u{e001}";

/// The four location exports Belay resolves while writing.
///
/// Each names a directory the generated artifact has to be able to point at,
/// and none of them can be known during evaluation: the agent root depends on
/// which adapter is being compiled, and all four are written relative to the
/// file that carries the use. Evaluation therefore leaves a marker, exactly as
/// it does for `BELAY_COMPILED_SHAPE`, and Belay substitutes a path per file.
pub const BELAY_AGENT_ROOT: &str = "BELAY_AGENT_ROOT";
pub const BELAY_PROJECT_ROOT: &str = "BELAY_PROJECT_ROOT";
pub const BELAY_SHAPE_ROOT: &str = "BELAY_SHAPE_ROOT";
pub const BELAY_CODE_ROOT: &str = "BELAY_CODE_ROOT";

/// The marker each of the four location exports evaluates to.
pub const BELAY_AGENT_ROOT_TOKEN: &str = "\u{e000}BELAY_AGENT_ROOT\u{e001}";
pub const BELAY_PROJECT_ROOT_TOKEN: &str = "\u{e000}BELAY_PROJECT_ROOT\u{e001}";
pub const BELAY_SHAPE_ROOT_TOKEN: &str = "\u{e000}BELAY_SHAPE_ROOT\u{e001}";
pub const BELAY_CODE_ROOT_TOKEN: &str = "\u{e000}BELAY_CODE_ROOT\u{e001}";

/// Each location export, paired with the marker it evaluates to, so a consumer
/// can iterate the set rather than repeating the five names.
pub const BELAY_LOCATION_MARKERS: [(&str, &str); 5] = [
    (BELAY_SHAPE, BELAY_SHAPE_TOKEN),
    (BELAY_AGENT_ROOT, BELAY_AGENT_ROOT_TOKEN),
    (BELAY_PROJECT_ROOT, BELAY_PROJECT_ROOT_TOKEN),
    (BELAY_SHAPE_ROOT, BELAY_SHAPE_ROOT_TOKEN),
    (BELAY_CODE_ROOT, BELAY_CODE_ROOT_TOKEN),
];

/// Placeholders in the package template, swapped for the real markers at load.
const MARKER_PLACEHOLDERS: [(&str, &str); 5] = [
    ("__BELAY_SHAPE_MARKER__", BELAY_SHAPE_TOKEN),
    ("__BELAY_AGENT_ROOT_MARKER__", BELAY_AGENT_ROOT_TOKEN),
    ("__BELAY_PROJECT_ROOT_MARKER__", BELAY_PROJECT_ROOT_TOKEN),
    ("__BELAY_SHAPE_ROOT_MARKER__", BELAY_SHAPE_ROOT_TOKEN),
    ("__BELAY_CODE_ROOT_MARKER__", BELAY_CODE_ROOT_TOKEN),
];

/// `@piton/config`: the anchors a `piton.config.pi` file is built from.
pub const CONFIG_SOURCE: &str = r#"// Bundled with the Piton compiler.

export abstract anchor PitonConfig as piton-config:
    root:: string
    entry:: string:: null: null
    output:: string:: null: null
    renderer:: string:: null: null
    frameworks:: list:: null: null
    packages:: list:: null: null
    dependencies:: list:: null: null

export abstract anchor FrameworkConfig as framework-config:
    pass
"#;

/// `@piton/packaging`: the anchor a project declares a publishable package with.
///
/// A package declaration says what part of this project is published and what
/// it needs. It is abstract for the same reason the configuration anchors are:
/// `piton-package MyThing:` is the form the specification writes, and a
/// keyword only reads that way when the anchor behind it is a shape rather
/// than a value.
///
/// `name` is how a package is called something an anchor cannot be, such as
/// `my-package`. Left out, the anchor's own name is the package name. Either
/// way it cannot contain `/` or start with `@`.
///
/// `dependencies` is a list rather than `string[]` because a URL may be
/// followed by a pin dictionary.
pub const PACKAGING_SOURCE: &str = r#"// Bundled with the Piton compiler.

export abstract anchor PitonPackage as piton-package:
    name:: string:: null: null
    root:: string
    dependencies:: list:: null: null
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
// The adapters, likewise taken from the specification: a project selects one
// by listing the anchor, and Belay reads the target's paths and settings from
// it.
const BELAY_ADAPTER: &str = include_str!("../../../spec/scope/belay/adapters/Adapter.pi");
const BELAY_CLAUDE_CODE: &str = include_str!("../../../spec/scope/belay/adapters/ClaudeCode.pi");
const BELAY_CODEX: &str = include_str!("../../../spec/scope/belay/adapters/Codex.pi");
const BELAY_OPENCODE: &str = include_str!("../../../spec/scope/belay/adapters/OpenCode.pi");

/// `@piton/belay`: re-exports the constructs and the adapters, and adds the
/// configuration anchor and the special imports.
const BELAY_INDEX: &str = r#"// Bundled with the Piton compiler.
//
// The four constructs and the adapters come from the specification that
// describes them. Only their names are listed here, so the package exports the
// framework without also exporting the documentation that surrounds it.

use @piton/config

from @piton/config import FrameworkConfig

from ./anchors/Construct export Construct
from ./anchors/Instruction export Instruction
from ./anchors/Skill export Skill
from ./anchors/Command export Command
from ./anchors/Agent export Agent

from ./adapters/Adapter export Adapter
from ./adapters/ClaudeCode export ClaudeCodeAdapter
from ./adapters/Codex export CodexAdapter
from ./adapters/OpenCode export OpenCodeAdapter

from ./adapters/ClaudeCode import ClaudeCodeAdapter

// Resolved during compilation, separately for each adapter and each output
// location. Never resolved at agent runtime.
export BELAY_COMPILED_SHAPE: __BELAY_SHAPE_MARKER__

// The directories a generated artifact may need to point at. Each is written
// relative to the file that carries it, for the adapter being compiled.
export BELAY_AGENT_ROOT: __BELAY_AGENT_ROOT_MARKER__
export BELAY_PROJECT_ROOT: __BELAY_PROJECT_ROOT_MARKER__
export BELAY_SHAPE_ROOT: __BELAY_SHAPE_ROOT_MARKER__
export BELAY_CODE_ROOT: __BELAY_CODE_ROOT_MARKER__

// crossDiscovery is the deployment choice for skills one adapter writes and
// another discovers (OpenCode reads .claude/skills and .agents/skills): allow
// accepts that, separate says the trees are deployed apart. It is required
// only when discovery would change how something activates.
// instructionByteLimit is the byte budget of an instruction chain, for
// targets that truncate past one (Codex's project_doc_max_bytes).
export abstract anchor BelayConfig extends FrameworkConfig as belay-config:
    codeRoot:: string
    shapeRoot:: string:: null: null
    adapters:: list
    crossDiscovery:: string:: null: null
    instructionByteLimit:: number:: null: null
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
            let mut text = BELAY_INDEX.to_string();
            for (placeholder, marker) in MARKER_PLACEHOLDERS {
                text = text.replace(placeholder, marker);
            }
            let index: &'static str = Box::leak(text.into_boxed_str());
            vec![
                (CONFIG_PACKAGE, CONFIG_SOURCE),
                (PACKAGING_PACKAGE, PACKAGING_SOURCE),
                (BELAY_PACKAGE, index),
                ("@piton/belay/Framework", BELAY_FRAMEWORK),
                ("@piton/belay/anchors/Construct", BELAY_CONSTRUCT),
                ("@piton/belay/anchors/Instruction", BELAY_INSTRUCTION),
                ("@piton/belay/anchors/Skill", BELAY_SKILL),
                ("@piton/belay/anchors/Command", BELAY_COMMAND),
                ("@piton/belay/anchors/Agent", BELAY_AGENT),
                ("@piton/belay/adapters/Adapter", BELAY_ADAPTER),
                ("@piton/belay/adapters/ClaudeCode", BELAY_CLAUDE_CODE),
                ("@piton/belay/adapters/Codex", BELAY_CODEX),
                ("@piton/belay/adapters/OpenCode", BELAY_OPENCODE),
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
pub const PACKAGE_ROOTS: [&str; 3] = [CONFIG_PACKAGE, BELAY_PACKAGE, PACKAGING_PACKAGE];

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
    fn every_location_marker_is_substituted_once() {
        let index = package_source(BELAY_PACKAGE).expect("index");
        for (name, marker) in BELAY_LOCATION_MARKERS {
            assert!(index.contains(marker), "{name} has no marker in the package");
        }
        for (placeholder, _) in MARKER_PLACEHOLDERS {
            assert!(
                !index.contains(placeholder),
                "{placeholder} survived substitution"
            );
        }
    }

    #[test]
    fn location_markers_are_distinct() {
        for (index, (name, marker)) in BELAY_LOCATION_MARKERS.iter().enumerate() {
            for (other_name, other) in &BELAY_LOCATION_MARKERS[index + 1..] {
                assert_ne!(marker, other, "{name} and {other_name} share a marker");
            }
        }
    }
}
