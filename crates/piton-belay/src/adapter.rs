//! Adapter contracts.
//!
//! Each adapter maps the four Belay constructs onto one platform's native
//! artifacts. Where a platform has no native form for a construct, the adapter
//! says so explicitly: a translation is recorded as a translation, and anything
//! that cannot be represented becomes a diagnostic rather than a silent change
//! of activation behaviour.

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

/// One output target.
#[derive(Debug, Clone)]
pub struct Adapter {
    pub target_id: &'static str,
    /// The platform's own directory in the project, which `BELAY_AGENT_ROOT`
    /// resolves to. Stated rather than derived: Codex keeps its skills outside
    /// its own directory, so no common prefix of the roots below names it.
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
    /// Native agent option names this target accepts.
    pub agent_options: &'static [&'static str],
    /// Native skill option names this target accepts.
    pub skill_options: &'static [&'static str],
    /// Native command option names this target accepts.
    pub command_options: &'static [&'static str],
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

/// Claude Code. Claude unifies custom commands with skills, so a command
/// becomes an `x-` prefixed skill with automatic invocation disabled rather
/// than being emitted twice.
pub const CLAUDE_CODE: Adapter = Adapter {
    target_id: "claude-code",
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
    agent_options: &["tools", "model"],
    skill_options: &["allowed-tools", "model"],
    command_options: &["allowed-tools", "model"],
};

/// Codex. Commands become explicitly invoked skills, and the policy that
/// disables implicit invocation lives in a separate file.
pub const CODEX: Adapter = Adapter {
    target_id: "codex",
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
    agent_options: &["model", "model_reasoning_effort", "sandbox_mode"],
    skill_options: &[],
    command_options: &[],
};

/// OpenCode. Commands are native here, and the platform also discovers skill
/// directories belonging to the other two adapters.
pub const OPENCODE: Adapter = Adapter {
    target_id: "opencode",
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
    agent_options: &["mode", "model", "permission"],
    skill_options: &[],
    command_options: &["agent", "model"],
};

static ALL: &[Adapter] = &[CLAUDE_CODE, CODEX, OPENCODE];

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
}
