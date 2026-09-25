//! Adapter contracts.
//!
//! Each adapter maps the four Belay constructs onto one platform's native
//! artifacts. Where a platform has no native form for a construct, the adapter
//! says so explicitly: a translation is recorded as a translation, and anything
//! that cannot be represented becomes a diagnostic rather than a silent change
//! of activation behaviour.
//!
//! The adapters themselves are Piton anchors: `spec/scope/belay/adapters/*.pi`
//! is embedded into `@piton/belay`, and a project selects one by listing it in
//! its `belay-config`. Everything those anchors state -- the target id, the
//! instruction file, the reference root, each construct's output path, the
//! command representation, the default agent mode and the date the mapping was
//! checked -- is read from the anchor the project configured ([`Target`]). The
//! constants below are the same facts for callers that have no compilation at
//! hand, plus what the specification describes only in prose: which native
//! options each target accepts and which metadata it requires. A test holds
//! the two in agreement.

use piton_core::{Properties, Value};

use crate::construct::ConstructKind;

/// How a platform represents a command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSupport {
    /// Translated to a manually invoked skill with model invocation disabled.
    TranslatedSkill,
    /// Translated to a skill plus a policy file that disables implicit
    /// invocation.
    TranslatedSkillWithPolicy,
    /// The platform has a native command form.
    Native,
}

/// How a platform represents an agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentFormat {
    /// Markdown with YAML frontmatter carrying `name` and `description`.
    MarkdownNamed,
    /// Markdown with YAML frontmatter where identity comes from the filename.
    MarkdownFilenameIdentity,
    /// A standalone TOML document.
    Toml,
}

/// How a native option's value is checked before it is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionRule {
    /// Any string.
    Text,
    /// A string or a list of strings.
    TextOrList,
    /// One of a fixed set of strings.
    OneOf(&'static [&'static str]),
    /// A provider-qualified model, `provider/model`.
    ProviderModel,
    /// An OpenCode permission map: tool names to `allow`, `ask` or `deny`, or
    /// to a map of patterns that each carry one of those.
    PermissionMap,
}

/// One native option a target accepts for a construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeOption {
    pub name: &'static str,
    pub rule: OptionRule,
}

const fn option(name: &'static str, rule: OptionRule) -> NativeOption {
    NativeOption { name, rule }
}

/// One output target, as the compiler knows it without a compilation.
#[derive(Debug, Clone)]
pub struct Adapter {
    pub target_id: &'static str,
    /// The anchor `@piton/belay` exports for this target.
    pub export_name: &'static str,
    /// The platform's own name, for people choosing between targets.
    pub platform: &'static str,
    /// The platform's own directory in the project, which `BELAY_AGENT_ROOT`
    /// resolves to: the directory holding the reference root.
    pub root: &'static str,
    /// Scoped guidance filename, placed by shape mapping.
    pub instruction_file: &'static str,
    /// Belay-owned directory for compiled references. This is a convention, not
    /// a platform discovery directory: nothing here loads automatically.
    pub reference_root: &'static str,
    pub skill_root: &'static str,
    pub agent_root: &'static str,
    pub command_root: &'static str,
    pub command_support: CommandSupport,
    pub agent_format: AgentFormat,
    /// How a command is invoked once generated, used in diagnostics.
    pub invocation: &'static str,
    /// Version of the target these mappings were validated against.
    pub documentation_checked: &'static str,
    /// True when the platform discovers skill directories belonging to other
    /// adapters, which makes cross-target isolation a planning concern.
    pub discovers_foreign_skills: bool,
    /// Native agent options this target accepts.
    pub agent_options: &'static [NativeOption],
    /// Native skill options this target accepts.
    pub skill_options: &'static [NativeOption],
    /// Native command options this target accepts.
    pub command_options: &'static [NativeOption],
    /// Constructs whose native form needs a `description`.
    pub description_required: &'static [ConstructKind],
    /// The discovery description length the target accepts, in characters.
    pub description_range: (usize, usize),
    /// The default byte budget for the whole instruction chain, when the target
    /// truncates guidance past one.
    pub instruction_byte_limit: Option<u64>,
    /// A same-directory file that replaces the generated instruction file.
    pub instruction_override: Option<&'static str>,
    /// False when the platform does not load instruction files below the
    /// project root at startup unless configured to.
    pub loads_nested_instructions: bool,
}

impl Adapter {
    /// The compiled shape root for this target.
    pub fn shape_root(&self) -> String {
        format!("{}/shape", self.reference_root)
    }

    pub fn all() -> &'static [Adapter] {
        ALL
    }

    pub fn by_target(target: &str) -> Option<&'static Adapter> {
        ALL.iter().find(|adapter| adapter.target_id == target)
    }
}

const CLAUDE_TEXT_OPTIONS: &[NativeOption] = &[
    option("allowed-tools", OptionRule::TextOrList),
    option("model", OptionRule::Text),
];

/// Claude Code. Claude unifies custom commands with skills, so a command
/// becomes an `x-` prefixed skill with automatic invocation disabled rather
/// than being emitted twice.
pub const CLAUDE_CODE: Adapter = Adapter {
    target_id: "claude-code",
    export_name: "ClaudeCodeAdapter",
    platform: "Claude Code",
    root: ".claude",
    instruction_file: "CLAUDE.md",
    reference_root: ".claude/reference",
    skill_root: ".claude/skills",
    agent_root: ".claude/agents",
    command_root: ".claude/skills",
    command_support: CommandSupport::TranslatedSkill,
    agent_format: AgentFormat::MarkdownNamed,
    invocation: "/x-<name>",
    documentation_checked: "2026-09-21",
    discovers_foreign_skills: false,
    agent_options: &[
        option("tools", OptionRule::TextOrList),
        option("model", OptionRule::Text),
    ],
    skill_options: CLAUDE_TEXT_OPTIONS,
    command_options: CLAUDE_TEXT_OPTIONS,
    // Subagents are selected by their description; skills and command skills
    // fall back to their content when it is missing.
    description_required: &[ConstructKind::Agent],
    description_range: (1, 1024),
    instruction_byte_limit: None,
    instruction_override: None,
    loads_nested_instructions: true,
};

/// Codex. Commands become explicitly invoked skills, and the policy that
/// disables implicit invocation lives in a separate file.
pub const CODEX: Adapter = Adapter {
    target_id: "codex",
    export_name: "CodexAdapter",
    platform: "Codex",
    root: ".codex",
    instruction_file: "AGENTS.md",
    reference_root: ".codex/reference",
    skill_root: ".agents/skills",
    agent_root: ".codex/agents",
    command_root: ".agents/skills",
    command_support: CommandSupport::TranslatedSkillWithPolicy,
    agent_format: AgentFormat::Toml,
    invocation: "$x-<name>",
    documentation_checked: "2026-09-21",
    discovers_foreign_skills: false,
    agent_options: &[
        option("model", OptionRule::Text),
        option(
            "model_reasoning_effort",
            OptionRule::OneOf(&["minimal", "low", "medium", "high", "xhigh"]),
        ),
        option(
            "sandbox_mode",
            OptionRule::OneOf(&["read-only", "workspace-write", "danger-full-access"]),
        ),
    ],
    skill_options: &[],
    command_options: &[],
    // A Codex skill needs a name and a description, and so does the skill a
    // command becomes; a custom agent needs name, description and
    // developer_instructions.
    description_required: &[
        ConstructKind::Skill,
        ConstructKind::Command,
        ConstructKind::Agent,
    ],
    description_range: (1, 1024),
    // Codex's `project_doc_max_bytes` defaults to 32 KiB across the whole
    // root-to-working-directory chain.
    instruction_byte_limit: Some(32 * 1024),
    instruction_override: Some("AGENTS.override.md"),
    loads_nested_instructions: true,
};

/// OpenCode. Commands are native here, and the platform also discovers skill
/// directories belonging to the other two adapters.
pub const OPENCODE: Adapter = Adapter {
    target_id: "opencode",
    export_name: "OpenCodeAdapter",
    platform: "OpenCode",
    root: ".opencode",
    instruction_file: "AGENTS.md",
    reference_root: ".opencode/reference",
    skill_root: ".opencode/skills",
    agent_root: ".opencode/agents",
    command_root: ".opencode/commands",
    command_support: CommandSupport::Native,
    agent_format: AgentFormat::MarkdownFilenameIdentity,
    invocation: "/x-<name>",
    documentation_checked: "2026-09-21",
    discovers_foreign_skills: true,
    agent_options: &[
        option("mode", OptionRule::OneOf(&["primary", "subagent", "all"])),
        option("model", OptionRule::ProviderModel),
        option("permission", OptionRule::PermissionMap),
    ],
    skill_options: &[],
    command_options: &[
        option("agent", OptionRule::Text),
        option("model", OptionRule::ProviderModel),
    ],
    description_required: &[ConstructKind::Skill, ConstructKind::Agent],
    description_range: (1, 1024),
    instruction_byte_limit: None,
    instruction_override: None,
    loads_nested_instructions: false,
};

static ALL: &[Adapter] = &[CLAUDE_CODE, CODEX, OPENCODE];

/// Skill directories OpenCode discovers besides its own.
pub const FOREIGN_SKILL_ROOTS: &[&str] = &[".claude/skills", ".agents/skills"];

/// An adapter as the project configured it: the facts its anchor states, over
/// the defaults of the built-in adapter with the same target id.
#[derive(Debug, Clone)]
pub struct Target {
    /// The built-in adapter this target extends, for the facts its anchor
    /// does not state.
    pub base: &'static Adapter,
    pub id: &'static str,
    /// The adapter anchor's name, for diagnostics.
    pub anchor_name: String,
    pub root: String,
    pub instruction_file: String,
    pub reference_root: String,
    /// Output path templates with a `<name>` placeholder.
    pub skill_output: String,
    pub command_output: String,
    pub policy_output: Option<String>,
    pub agent_output: String,
    pub command_support: CommandSupport,
    pub agent_format: AgentFormat,
    pub default_mode: Option<String>,
    pub documentation_checked: String,
    pub instruction_byte_limit: Option<u64>,
}

impl Target {
    /// A target taken entirely from a built-in adapter.
    pub fn builtin(base: &'static Adapter) -> Target {
        let (skill_output, command_output, policy_output, agent_output) = default_outputs(base);
        Target {
            base,
            id: base.target_id,
            anchor_name: base.export_name.to_string(),
            root: base.root.to_string(),
            instruction_file: base.instruction_file.to_string(),
            reference_root: base.reference_root.to_string(),
            skill_output,
            command_output,
            policy_output,
            agent_output,
            command_support: base.command_support,
            agent_format: base.agent_format,
            default_mode: (base.agent_format == AgentFormat::MarkdownFilenameIdentity)
                .then(|| "subagent".to_string()),
            documentation_checked: base.documentation_checked.to_string(),
            instruction_byte_limit: base.instruction_byte_limit,
        }
    }

    /// A target read from an adapter anchor's resolved properties.
    ///
    /// Each fact the anchor states replaces the built-in default; what it
    /// leaves out keeps it.
    pub fn from_anchor(base: &'static Adapter, anchor_name: &str, properties: &Properties) -> Target {
        let mut target = Target::builtin(base);
        target.anchor_name = anchor_name.to_string();
        let text = |path: &[&str]| lookup(properties, path).and_then(plain);

        if let Some(file) = text(&["instructionFile"]) {
            target.instruction_file = file;
        }
        if let Some(root) = text(&["referenceRoot"]) {
            let root = root.trim_end_matches('/').to_string();
            target.root = match root.rsplit_once('/') {
                Some((parent, _)) => parent.to_string(),
                None => root.clone(),
            };
            target.reference_root = root;
        }
        if let Some(checked) = text(&["documentationChecked"]) {
            target.documentation_checked = checked;
        }
        if let Some(output) = text(&["skill", "output"]).filter(|o| o.contains("<name>")) {
            target.skill_output = output;
        }
        if let Some(output) = text(&["command", "output"]).filter(|o| o.contains("<name>")) {
            target.command_output = output;
        }
        if let Some(output) = text(&["command", "policyOutput"]).filter(|o| o.contains("<name>")) {
            target.policy_output = Some(output);
        }
        if let Some(output) = text(&["agent", "output"]).filter(|o| o.contains("<name>")) {
            target.agent_output = output;
        }
        if let Some(support) = text(&["command", "support"]) {
            target.command_support = if support.trim() == "native" {
                CommandSupport::Native
            } else if target.policy_output.is_some() {
                CommandSupport::TranslatedSkillWithPolicy
            } else {
                CommandSupport::TranslatedSkill
            };
        }
        if target.agent_output.ends_with(".toml") {
            target.agent_format = AgentFormat::Toml;
        } else if let Some(metadata) = lookup(properties, &["agent", "metadata"]) {
            target.agent_format = if value_has_key(metadata, "name") {
                AgentFormat::MarkdownNamed
            } else {
                AgentFormat::MarkdownFilenameIdentity
            };
        }
        if let Some(mode) = text(&["agent", "defaultMode"]) {
            target.default_mode = Some(mode);
        }
        target
    }

    /// The compiled shape root for this target.
    pub fn shape_root(&self) -> String {
        format!("{}/shape", self.reference_root)
    }

    pub fn skill_path(&self, name: &str) -> String {
        self.skill_output.replace("<name>", name)
    }

    /// The command's output path, for a name without its `x-` prefix.
    pub fn command_path(&self, name: &str) -> String {
        self.command_output.replace("<name>", name)
    }

    pub fn policy_path(&self, name: &str) -> Option<String> {
        self.policy_output
            .as_ref()
            .map(|template| template.replace("<name>", name))
    }

    pub fn agent_path(&self, name: &str) -> String {
        self.agent_output.replace("<name>", name)
    }

    /// Native options this target accepts for one construct.
    pub fn options(&self, kind: ConstructKind) -> &'static [NativeOption] {
        match kind {
            ConstructKind::Skill => self.base.skill_options,
            ConstructKind::Command => self.base.command_options,
            ConstructKind::Agent => self.base.agent_options,
            ConstructKind::Instruction => &[],
        }
    }

    pub fn requires_description(&self, kind: ConstructKind) -> bool {
        self.base.description_required.contains(&kind)
    }
}

fn default_outputs(base: &Adapter) -> (String, String, Option<String>, String) {
    let skill = format!("{}/<name>/SKILL.md", base.skill_root);
    let command = match base.command_support {
        CommandSupport::Native => format!("{}/x-<name>.md", base.command_root),
        _ => format!("{}/x-<name>/SKILL.md", base.command_root),
    };
    let policy = (base.command_support == CommandSupport::TranslatedSkillWithPolicy)
        .then(|| format!("{}/x-<name>/agents/openai.yaml", base.command_root));
    let extension = if base.agent_format == AgentFormat::Toml {
        "toml"
    } else {
        "md"
    };
    let agent = format!("{}/<name>.{extension}", base.agent_root);
    (skill, command, policy, agent)
}

/// Follows a dot path through dictionaries and mixed blocks.
fn lookup<'a>(properties: &'a Properties, path: &[&str]) -> Option<&'a Value> {
    let (first, rest) = path.split_first()?;
    let mut value = properties.get(*first)?;
    for key in rest {
        value = match value {
            Value::Dict(map) => map.get(*key)?,
            Value::Mixed(mixed) => mixed.get(key)?,
            _ => return None,
        };
    }
    Some(value)
}

fn value_has_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Dict(map) => map.contains_key(key),
        Value::Mixed(mixed) => mixed.get(key).is_some(),
        _ => false,
    }
}

fn plain(value: &Value) -> Option<String> {
    match value {
        Value::Str(text) => text.as_plain().map(|t| t.trim().to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapters_are_addressable_by_target_id() {
        assert_eq!(
            Adapter::by_target("claude-code").map(|a| a.instruction_file),
            Some("CLAUDE.md")
        );
        assert_eq!(
            Adapter::by_target("codex").map(|a| a.agent_format),
            Some(AgentFormat::Toml)
        );
        assert!(Adapter::by_target("nope").is_none());
    }

    #[test]
    fn shape_roots_sit_under_the_reference_root() {
        assert_eq!(CLAUDE_CODE.shape_root(), ".claude/reference/shape");
    }

    #[test]
    fn every_adapter_names_its_own_root() {
        for adapter in Adapter::all() {
            assert!(!adapter.root.is_empty(), "{}", adapter.target_id);
            assert!(
                adapter.reference_root.starts_with(adapter.root),
                "{} keeps its references outside its root",
                adapter.target_id
            );
        }
    }

    #[test]
    fn builtin_targets_fill_their_output_templates() {
        let codex = Target::builtin(&CODEX);
        assert_eq!(codex.skill_path("review"), ".agents/skills/review/SKILL.md");
        assert_eq!(codex.command_path("release"), ".agents/skills/x-release/SKILL.md");
        assert_eq!(
            codex.policy_path("release").as_deref(),
            Some(".agents/skills/x-release/agents/openai.yaml")
        );
        assert_eq!(codex.agent_path("reviewer"), ".codex/agents/reviewer.toml");
        let opencode = Target::builtin(&OPENCODE);
        assert_eq!(opencode.command_path("release"), ".opencode/commands/x-release.md");
        assert_eq!(opencode.default_mode.as_deref(), Some("subagent"));
    }
}
