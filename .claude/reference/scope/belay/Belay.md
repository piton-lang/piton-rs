# Belay

## Description

A Piton framework for structured agent instructions, skills, commands, and agents, compiled into the formats used by agentic coding tools.

## Special Imports

```
description: The following special imports can be brought in from `@piton/belay` and used. They're filled in at compile time, for each adapter, as a path relative to the file they end up in.
BELAY_AGENT_ROOT: The agent's directory, so that might be .claude or .opencode, etc.
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

- [ClaudeCodeAdapter](./adapters/ClaudeCode.md#claude-code-adapter)
- [CodexAdapter](./adapters/Codex.md#codex-adapter)
- [OpenCodeAdapter](./adapters/OpenCode.md#open-code-adapter)

## Guarantees

- [BuildGuarantees](./Decisions.md#build-guarantees)
- [PermissionBoundary](./Decisions.md#permission-boundary)

## Unresolved

[OpenDecisions](./Decisions.md#open-decisions)
