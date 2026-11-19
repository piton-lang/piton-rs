# Belay

## Description

A Piton framework for structured agent instructions, skills, commands, and agents, compiled into the formats used by agentic coding tools.

## Plugin

[BelayPlugin](./Plugin.md#belay-plugin)

## Special Imports

```
description: The following special imports can be brought in from `@piton/belay` and used. They're the Belay plugin's placeholders: filled in by Belay's build, for each adapter, as a path relative to the file they end up in. Anywhere else, like the output of `piton compile` or the renderer's output of `piton build`, the project root resolves the same way, the code and shape roots are written as the project root, and the agent root and compiled shape, which only exist for an adapter, are written as their names.
BELAY_AGENT_ROOT: The agent's directory, so that might be .claude, .codex, .opencode or .muse.
BELAY_PROJECT_ROOT: The project, where piton.config.pi lives.
BELAY_SHAPE_ROOT: The shape as configured in the project config. If not specified, it's the project root.
BELAY_CODE_ROOT: The code as configured in the project config.
BELAY_COMPILED_SHAPE: The compiled copy of the shape for the agent, under its reference directory.
```

## Framework

- [FrameworkPurpose](./Framework.md#framework-purpose)
- [SourceAuthority](./Framework.md#source-authority)
- [FrameworkBoundaries](./Framework.md#framework-boundaries)
- [ProjectConfiguration](./Framework.md#project-configuration)
- [ConstructComposition](./Framework.md#construct-composition)

## Anchors

- [InstructionBehavior](./anchors/Instruction.md#instruction-behavior)
- [SkillBehavior](./anchors/Skill.md#skill-behavior)
- [CommandBehavior](./anchors/Command.md#command-behavior)
- [AgentBehavior](./anchors/Agent.md#agent-behavior)

## Compilation

[Compilation](./Compilation.md#compilation)

## Scope

- [ShapeMapping](./Scope.md#shape-mapping)
- [ShapeRootReference](./Scope.md#shape-root-reference)
- [AnchorReferences](./Scope.md#anchor-references)

## Adapters

- description: Compile Belay constructs into project-local Claude Code artifacts.
  targetId: claude-code
  instructionFile: CLAUDE.md
  referenceRoot: .claude/reference
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve BELAY_COMPILED_SHAPE to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
    convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter. How you write them is still an open decision.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: The tools read each other's files. Codex, OpenCode and Muse all read AGENTS.md; OpenCode also discovers the skills in .claude/skills and .agents/skills, and Muse those in .agents/skills, .claude/skills and .codex/skills. So with several adapters enabled, Belay plans one project that every enabled tool reads, and writes each artifact once, where the tools that need it will find it. With crossDiscovery set to separate, the trees are deployed apart instead, and each adapter writes its own complete tree.
    sharedReferences: The first adapter listed owns the reference tree, compiled shape included. Every adapter's links point into it, and the others write no reference tree of their own. Where the tree links to a skill, command, or agent, it links to the owner's. With one adapter, or with crossDiscovery set to separate, each adapter owns its own.
    unsupportedConstructs: When a target has no native form for a construct, like an agent for Muse, it writes nothing for it and the build warns. Links to that construct go to another enabled adapter's artifact for it when the project is shared, or else to its reference document.
    sharedInstructions: An instruction file that several adapters place at one path is written once, by the first of them listed, and the other tools read that file.
    discoveredSkills: An adapter whose tool also discovers another enabled adapter's skill directory writes no copy of a skill that adapter writes there, and links to that one instead. A tool that also honours the other adapters' command translations does the same for commands. A skill the tool discovers in more than one directory is reported, since the tool offers each copy and nothing in the project can stop it, unless the tool itself keeps only one copy.
    discoveredCommands: A command translated into a skill gets its activation from metadata or a policy file. When another enabled tool also discovers that skill and does not honour that metadata or policy, the project has to hide it from that tool with a control the tool enforces, or accept the change by setting crossDiscovery to allow. Otherwise it is an error.
    requirements:
      - Check the complete output plan before writing.
      - Reject incompatible writes to a shared path, other than an instruction file placed by several adapters.
      - Write identical shared output once.
      - Report a skill a tool discovers more than once.
      - Report a command that another tool would offer as an ordinary skill, unless the project hides it or accepts the change.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Pass Markdown links an author wrote in prose through as written, without verifying them; Piton has no link syntax, so they are text.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: CLAUDE.md at the scope selected by ShapeMapping
    format: Markdown
    rules:
      - Combine guidance for the same scope in one file.
      - Preserve a separate compiled shape reference beneath .claude/reference/shape.
    loading: Ancestor guidance loads at startup; nested guidance loads when Claude reads within that subtree. Placement does not make every nested instruction part of the initial context.
  skill:
    output: .claude/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
    rules:
      - Use the same normalized name for the directory and metadata.
      - Preserve native allowed-tools and model options only when explicitly configured.
  command:
    support: translated to a manually invoked Claude skill
    output: .claude/skills/x-<name>/SKILL.md
    metadata:
      name: x- followed by the normalized construct name
      description: Command description
      disable-model-invocation: true
    body: Command prompt followed by serialized additional content
    invocation: /x-<name>
    rationale: Claude unifies custom commands with skills. The older commands directory remains supported, but this adapter chooses skills for new output. Do not emit both representations of one command.
  agent:
    output: .claude/agents/<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Agent description
      tools: Explicit native tool list when configured
      model: Explicit native model selector when configured
    body: Role introduction followed by prompt and serialized additional content
    rules:
      - Preserve agent identity independently of display headings.
      - Omit optional settings when absent so native defaults remain effective.
  permissions:
    description: Skill allowed-tools grants and subagent tool selection serve different purposes. Do not interpret either as a portable sandbox definition or copy it into another platform without a mapping.
  checks:
    - A command produces one x-prefixed skill with automatic invocation disabled.
    - Two constructs normalizing to the same skill identity cause a collision diagnostic.
    - Nested instructions retain their mapped scope and target-relative links.
- description: Compile Belay constructs into project-local Codex artifacts.
  targetId: codex
  instructionFile: AGENTS.md
  referenceRoot: .codex/reference
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve BELAY_COMPILED_SHAPE to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
    convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter. How you write them is still an open decision.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: The tools read each other's files. Codex, OpenCode and Muse all read AGENTS.md; OpenCode also discovers the skills in .claude/skills and .agents/skills, and Muse those in .agents/skills, .claude/skills and .codex/skills. So with several adapters enabled, Belay plans one project that every enabled tool reads, and writes each artifact once, where the tools that need it will find it. With crossDiscovery set to separate, the trees are deployed apart instead, and each adapter writes its own complete tree.
    sharedReferences: The first adapter listed owns the reference tree, compiled shape included. Every adapter's links point into it, and the others write no reference tree of their own. Where the tree links to a skill, command, or agent, it links to the owner's. With one adapter, or with crossDiscovery set to separate, each adapter owns its own.
    unsupportedConstructs: When a target has no native form for a construct, like an agent for Muse, it writes nothing for it and the build warns. Links to that construct go to another enabled adapter's artifact for it when the project is shared, or else to its reference document.
    sharedInstructions: An instruction file that several adapters place at one path is written once, by the first of them listed, and the other tools read that file.
    discoveredSkills: An adapter whose tool also discovers another enabled adapter's skill directory writes no copy of a skill that adapter writes there, and links to that one instead. A tool that also honours the other adapters' command translations does the same for commands. A skill the tool discovers in more than one directory is reported, since the tool offers each copy and nothing in the project can stop it, unless the tool itself keeps only one copy.
    discoveredCommands: A command translated into a skill gets its activation from metadata or a policy file. When another enabled tool also discovers that skill and does not honour that metadata or policy, the project has to hide it from that tool with a control the tool enforces, or accept the change by setting crossDiscovery to allow. Otherwise it is an error.
    requirements:
      - Check the complete output plan before writing.
      - Reject incompatible writes to a shared path, other than an instruction file placed by several adapters.
      - Write identical shared output once.
      - Report a skill a tool discovers more than once.
      - Report a command that another tool would offer as an ordinary skill, unless the project hides it or accepts the change.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Pass Markdown links an author wrote in prose through as written, without verifying them; Piton has no link syntax, so they are text.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: AGENTS.md at the scope selected by ShapeMapping
    format: Markdown
    rules:
      - Diagnose a same-directory AGENTS.override.md that shadows generated guidance.
      - Account for the configured instruction byte limit.
      - Do not generate override files merely to win precedence.
    loading: Codex builds a root-to-working-directory instruction chain at run start. Descendant placement does not guarantee initial loading from a session started above it.
  skill:
    output: .agents/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
  command:
    support: translated to an explicitly invoked Codex skill
    output: .agents/skills/x-<name>/SKILL.md
    policyOutput: .agents/skills/x-<name>/agents/openai.yaml
    metadata:
      name: x- followed by the normalized construct name
      description: Command description
    policy:
      allow_implicit_invocation: false
    body: Command prompt followed by serialized additional content
    invocation: $x-<name>
    rules:
      - Nest allow_implicit_invocation under policy in the generated YAML.
      - Do not emit deprecated personal custom prompts as repository commands.
      - Do not invent a project-local .codex/commands discovery directory.
  agent:
    output: .codex/agents/<name>.toml
    format: TOML
    fields:
      name: Normalized construct name
      description: Agent description
      developer_instructions: Role introduction, prompt, and serialized additional content
    optionalFields:
      - model
      - model_reasoning_effort
      - sandbox_mode
    rules:
      - Serialize the complete instruction body as a valid TOML string.
      - Omit absent options rather than imposing model or sandbox defaults.
      - Do not translate a generic tool list into an invented tools field.
    qualification: This targets the documented standalone custom-agent format, not older role-registration layouts or every hosted Codex surface.
  permissions:
    description: Native sandbox and approval configuration govern execution. Generated guidance cannot override the active runtime's controls. Unsupported per-command tool or model settings must be diagnosed.
  checks:
    - Commands produce both the skill and its explicit-invocation policy file.
    - Agent output parses as TOML and contains the three required identity and instruction fields.
    - Shadowed or over-budget instruction output is reported before being called usable.
- description: Compile Belay constructs into project-local OpenCode artifacts.
  targetId: opencode
  instructionFile: AGENTS.md
  referenceRoot: .opencode/reference
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve BELAY_COMPILED_SHAPE to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
    convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter. How you write them is still an open decision.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: The tools read each other's files. Codex, OpenCode and Muse all read AGENTS.md; OpenCode also discovers the skills in .claude/skills and .agents/skills, and Muse those in .agents/skills, .claude/skills and .codex/skills. So with several adapters enabled, Belay plans one project that every enabled tool reads, and writes each artifact once, where the tools that need it will find it. With crossDiscovery set to separate, the trees are deployed apart instead, and each adapter writes its own complete tree.
    sharedReferences: The first adapter listed owns the reference tree, compiled shape included. Every adapter's links point into it, and the others write no reference tree of their own. Where the tree links to a skill, command, or agent, it links to the owner's. With one adapter, or with crossDiscovery set to separate, each adapter owns its own.
    unsupportedConstructs: When a target has no native form for a construct, like an agent for Muse, it writes nothing for it and the build warns. Links to that construct go to another enabled adapter's artifact for it when the project is shared, or else to its reference document.
    sharedInstructions: An instruction file that several adapters place at one path is written once, by the first of them listed, and the other tools read that file.
    discoveredSkills: An adapter whose tool also discovers another enabled adapter's skill directory writes no copy of a skill that adapter writes there, and links to that one instead. A tool that also honours the other adapters' command translations does the same for commands. A skill the tool discovers in more than one directory is reported, since the tool offers each copy and nothing in the project can stop it, unless the tool itself keeps only one copy.
    discoveredCommands: A command translated into a skill gets its activation from metadata or a policy file. When another enabled tool also discovers that skill and does not honour that metadata or policy, the project has to hide it from that tool with a control the tool enforces, or accept the change by setting crossDiscovery to allow. Otherwise it is an error.
    requirements:
      - Check the complete output plan before writing.
      - Reject incompatible writes to a shared path, other than an instruction file placed by several adapters.
      - Write identical shared output once.
      - Report a skill a tool discovers more than once.
      - Report a command that another tool would offer as an ordinary skill, unless the project hides it or accepts the change.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Pass Markdown links an author wrote in prose through as written, without verifying them; Piton has no link syntax, so they are text.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: AGENTS.md at the scope selected by ShapeMapping
    format: Markdown
    loading: OpenCode searches local rule files upward from the working directory and prefers AGENTS.md over its CLAUDE.md fallback. Do not assume Claude's nested-loading behavior or Codex's chain.
    rules:
      - Preserve scoped output placement without claiming identical activation across tools.
      - Use explicit instructions configuration when the selected deployment needs additional files.
      - Do not load all nested instructions globally merely to make them discoverable.
    nestedActivation: Only count on what OpenCode documents for startup. Don't assume nested files get picked up, and report an error if shape scoping is needed but can't be guaranteed.
  skill:
    output: .opencode/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
    rules:
      - Require a name of 1 to 64 lowercase alphanumeric characters with single hyphen separators.
      - Match name metadata to the containing directory.
      - Reject descriptions outside the supported 1 to 1024 character range.
      - Do not rely on unrecognized frontmatter to enforce policy.
    discovery: OpenCode also discovers the skills in .claude/skills and .agents/skills, and ignores frontmatter it doesn't recognise, like Claude's disable-model-invocation. With Claude Code or Codex enabled beside it, OpenCode writes no copy of a skill they already write there.
    discoveredCommands: OpenCode enforces skill permissions from opencode.json or opencode.jsonc, where deny hides a skill. A permission that denies the x- skills Claude Code and Codex write for commands, like "permission": {"skill": {"x-*": "deny"}}, keeps OpenCode from offering them as ordinary skills; its own x- commands are unaffected. Of several patterns that match a name, the last one listed decides.
  command:
    support: native
    output: .opencode/commands/x-<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      description: Command description
      agent: Explicit target agent when configured
      model: Explicit provider-qualified model when configured
    body: Command prompt followed by serialized additional content
    invocation: /x-<name>
    rules:
      - Derive command identity from the filename.
      - Preserve explicitly authored runtime argument placeholders.
      - Do not emit Claude-specific allowed-tools as an enforced command setting.
  agent:
    output: .opencode/agents/<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      description: Agent description
      mode: Explicit primary, subagent, or all selection
      model: Explicit provider-qualified model when configured
      permission: Explicit native permission map when configured
    body: Role introduction followed by prompt and serialized additional content
    defaultMode: subagent
    rules:
      - Derive agent identity from the filename.
      - Prefer native permission entries over the deprecated tools option.
      - Validate native options instead of copying metadata from another adapter.
  permissions:
    description: Map permissions only when their meanings are equivalent. Native allow, ask, and deny entries are not interchangeable with a prose tool list. Reject ambiguous conversions.
  checks:
    - A command keeps its x-prefixed filename and native frontmatter.
    - Agent mode is explicit in generated output.
    - A skill another enabled adapter writes where OpenCode discovers it is not written again.
    - Duplicate skills discovered through other adapters are reported.
    - A command skill OpenCode discovers is an error unless a skill permission denies it or crossDiscovery is set.
    - Required scope behavior that cannot be established for the target version is diagnosed.
- description: Compile Belay constructs into project-local Muse Code artifacts.
  targetId: muse
  instructionFile: AGENTS.md
  referenceRoot: .muse/reference
  documentationChecked: 2026-10-13
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve BELAY_COMPILED_SHAPE to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
    convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter. How you write them is still an open decision.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: The tools read each other's files. Codex, OpenCode and Muse all read AGENTS.md; OpenCode also discovers the skills in .claude/skills and .agents/skills, and Muse those in .agents/skills, .claude/skills and .codex/skills. So with several adapters enabled, Belay plans one project that every enabled tool reads, and writes each artifact once, where the tools that need it will find it. With crossDiscovery set to separate, the trees are deployed apart instead, and each adapter writes its own complete tree.
    sharedReferences: The first adapter listed owns the reference tree, compiled shape included. Every adapter's links point into it, and the others write no reference tree of their own. Where the tree links to a skill, command, or agent, it links to the owner's. With one adapter, or with crossDiscovery set to separate, each adapter owns its own.
    unsupportedConstructs: When a target has no native form for a construct, like an agent for Muse, it writes nothing for it and the build warns. Links to that construct go to another enabled adapter's artifact for it when the project is shared, or else to its reference document.
    sharedInstructions: An instruction file that several adapters place at one path is written once, by the first of them listed, and the other tools read that file.
    discoveredSkills: An adapter whose tool also discovers another enabled adapter's skill directory writes no copy of a skill that adapter writes there, and links to that one instead. A tool that also honours the other adapters' command translations does the same for commands. A skill the tool discovers in more than one directory is reported, since the tool offers each copy and nothing in the project can stop it, unless the tool itself keeps only one copy.
    discoveredCommands: A command translated into a skill gets its activation from metadata or a policy file. When another enabled tool also discovers that skill and does not honour that metadata or policy, the project has to hide it from that tool with a control the tool enforces, or accept the change by setting crossDiscovery to allow. Otherwise it is an error.
    requirements:
      - Check the complete output plan before writing.
      - Reject incompatible writes to a shared path, other than an instruction file placed by several adapters.
      - Write identical shared output once.
      - Report a skill a tool discovers more than once.
      - Report a command that another tool would offer as an ordinary skill, unless the project hides it or accepts the change.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Pass Markdown links an author wrote in prose through as written, without verifying them; Piton has no link syntax, so they are text.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: AGENTS.md at the scope selected by ShapeMapping
    format: Markdown
    loading: Muse reads rules files from the working directory up to the workspace root, and only in a trusted workspace. It reads AGENTS.md and CLAUDE.md; when both sit in one directory, AGENTS.md takes precedence and CLAUDE.md is ignored with a warning. Descendant placement does not guarantee initial loading from a session started above it.
    rules:
      - Combine guidance for the same scope in one file.
      - Share the AGENTS.md Codex and OpenCode read rather than writing a second rules file.
  skill:
    output: .agents/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
    rules:
      - Always write a description; Muse rejects a SKILL.md without one.
      - Do not emit allowed-tools; Muse records it as advisory metadata and enforces nothing.
    discovery: Muse discovers project skills in .agents/skills, .claude/skills and .codex/skills, in that order of priority. A skill found again in a lower-priority directory is skipped, so Muse offers one copy. With Claude Code or Codex enabled beside it, Muse writes no copy of a skill or command they already write there, and links to theirs.
  command:
    support: translated to a manually invoked skill
    output: .agents/skills/x-<name>/SKILL.md
    metadata:
      name: x- followed by the normalized construct name
      description: Command description
      disable-model-invocation: true
    body: Command prompt followed by serialized additional content
    invocation: /x-<name>
    discoveredCommands: Muse honours both translations the other adapters write: Claude's disable-model-invocation and Codex's agents/openai.yaml policy with allow_implicit_invocation false. Either makes the skill user-invocable only, so a command another adapter writes keeps its activation in Muse.
  agent:
    support: unsupported
    description: Muse has no project-local agent definition file. Agent definitions come from plugins or from a per-run overlay, neither of which a build can write. An agent is not translated into a skill: Muse gets no file for it, and the build warns with unsupported-construct. Everything else, the reference tree included, is still written.
    references: A reference to an agent links to the agent another enabled adapter writes in the same project. With none, the agent is documented in the reference tree like any other referenced anchor.
  permissions:
    description: Muse's approval mode, sandbox and permission profiles govern execution. Generated guidance cannot change them, and Muse accepts no native options from Belay.
  checks:
    - A command keeps its x-prefixed name and disable-model-invocation.
    - A skill or command another enabled adapter writes where Muse discovers it is not written again.
    - Every agent is reported as unsupported, and no agent file is written.

## Guarantees

- [BuildGuarantees](./Decisions.md#build-guarantees)
- [PermissionBoundary](./Decisions.md#permission-boundary)

## Unresolved

[OpenDecisions](./Decisions.md#open-decisions)
