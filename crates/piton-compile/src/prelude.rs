//! Built-in packages.
//!
//! `@piton/config` and `@piton/belay` are bundled with the compiler. They are
//! written in Piton rather than hand-built in Rust so that the vocabulary a
//! project sees is exactly the vocabulary the language can express.

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

/// `@piton/belay`: the four agentic constructs, the adapter contract, and the
/// three bundled adapters.
pub const BELAY_SOURCE: &str = r#"// Bundled with the Piton compiler.

use @piton/config

from @piton/config import FrameworkConfig

// Shared required fields for Belay's four constructs.
export abstract anchor Construct:
    description:: string
    prompt:: string

export abstract anchor Instruction extends Construct as instruction:
    pass

export abstract anchor Skill extends Construct as skill:
    useWhen:: string

export abstract anchor Command extends Construct as command:
    pass

export abstract anchor Agent extends Construct as agent:
    role:: string

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

/// The Belay package source, with the shape marker substituted in.
pub fn belay_source() -> &'static str {
    static SOURCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SOURCE.get_or_init(|| BELAY_SOURCE.replace(BELAY_SHAPE_PLACEHOLDER, BELAY_SHAPE_TOKEN))
}

/// Returns the source of a bundled package, if `path` names one.
pub fn package_source(path: &str) -> Option<&'static str> {
    match path {
        CONFIG_PACKAGE => Some(CONFIG_SOURCE),
        BELAY_PACKAGE => Some(belay_source()),
        _ => None,
    }
}

/// True when `path` refers to a bundled package.
pub fn is_package(path: &str) -> bool {
    package_source(path).is_some()
}
