//! The `@piton/belay` module: the anchors and keywords Belay contributes.

/// The module name Piton files import and `use`.
pub const MODULE: &str = "@piton/belay";

/// The abstract anchors Belay exports, paired with their keywords.
pub const KINDS: &[(&str, &str)] = &[
    ("Agent", "agent"),
    ("Skill", "skill"),
    ("Command", "command"),
    ("Instruction", "instruction"),
];

/// The anchor a project implements to configure Belay.
pub const CONFIG_ANCHOR: &str = "BelayConfig";

/// Build the module source, binding `__BELAY_SHAPE__` to the configured path.
pub fn source(shape_path: &str) -> String {
    format!(
        r#"// The Belay framework, bundled with the Piton compiler.
//
// Belay turns Piton declarations into the Markdown that agentic coding tools
// read: agents, skills, commands, and per-directory instructions.

// An agent: a role plus a prompt, compiled into the agent directory.
export abstract anchor Agent as agent:
    description:: string
    role:: string
    prompt:: string

// A skill: a prompt plus the situation it applies to.
export abstract anchor Skill as skill:
    description:: string
    prompt:: string
    useWhen:: string

// A command: a prompt invoked explicitly. Compiled files are prefixed `x-`.
export abstract anchor Command as command:
    description:: string
    prompt:: string

// An instruction: guidance compiled next to the code it applies to.
export abstract anchor Instruction as instruction:
    description:: string
    prompt:: string

// Belay's project configuration.
export abstract anchor BelayConfig as belay-config:
    codeRoot:: string

// The base of every output adapter.
export abstract anchor Adapter as belay-adapter:
    description:: string

// Writes agentic Markdown for tools such as Claude Code.
export abstract anchor AgentAdapter extends Adapter as belay-agent-adapter:
    description:: string

// A ready-made adapter that writes into `.claude`.
export anchor ClaudeAdapter extends AgentAdapter:
    description: Writes agentic Markdown into the .claude directory
    claude: true

// A ready-made adapter that writes into `.opencode`.
export anchor OpenCodeAdapter extends AgentAdapter:
    description: Writes agentic Markdown into the .opencode directory
    opencode: true

// The compiled shape directory, so skills can point agents at it.
export __BELAY_SHAPE__: {shape_path}
"#
    )
}
