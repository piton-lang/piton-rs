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

- [ClaudeCodeAdapter](../../plugins/belay/pi/adapters/ClaudeCode.md#claude-code-adapter)
- [CodexAdapter](../../plugins/belay/pi/adapters/Codex.md#codex-adapter)
- [OpenCodeAdapter](../../plugins/belay/pi/adapters/OpenCode.md#open-code-adapter)
- [MuseAdapter](../../plugins/belay/pi/adapters/Muse.md#muse-adapter)

## Guarantees

- [BuildGuarantees](./Decisions.md#build-guarantees)
- [PermissionBoundary](./Decisions.md#permission-boundary)

## Unresolved

[OpenDecisions](./Decisions.md#open-decisions)
