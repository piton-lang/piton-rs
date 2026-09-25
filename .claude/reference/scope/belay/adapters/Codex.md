# Codex Adapter

## Description

Compile Belay constructs into project-local Codex artifacts.

## Target Id

codex

## Instruction File

AGENTS.md

## Reference Root

.codex/reference

## Documentation Checked

2026-09-21

## Requirements

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

## Paths

```
description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
```

## Metadata

### Description

Platform-specific options belong to the selected adapter. How you write them is still an open decision.

### Requirements

- Validate native options against the selected platform version.
- Serialize YAML or TOML with a format-aware encoder.
- Keep role, prompt, and other prose out of duplicated metadata fields.
- Reject permission translations that would silently widen access.

## Unsupported Behavior

```
description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
```

## Multiple Adapters

### Description

Check the complete output plan before writing. Codex and OpenCode can share AGENTS.md paths, and OpenCode can discover other adapters' skills. File separation alone does not guarantee target isolation.

### Requirements

- Reject incompatible writes to a shared instruction file.
- Coalesce identical shared guidance only once.
- Diagnose duplicate discoverable skills across enabled adapters.
- Require an explicit deployment choice when cross-discovery changes behavior.

## Validation

- Verify all generated relative links point to planned outputs.
- Pass Markdown links an author wrote in prose through as written, without verifying them; Piton has no link syntax, so they are text.
- Verify discovery metadata and body content are emitted exactly once.
- Verify a command retains the x- prefix after target-name normalization.
- Verify generated filenames and serialized native metadata against the target schema.
- Verify unsupported requested capabilities produce diagnostics before output is committed.

## Instruction

### Output

AGENTS.md at the scope selected by ShapeMapping

### Format

Markdown

### Rules

- Diagnose a same-directory AGENTS.override.md that shadows generated guidance.
- Account for the configured instruction byte limit.
- Do not generate override files merely to win precedence.

### Loading

Codex builds a root-to-working-directory instruction chain at run start. Descendant placement does not guarantee initial loading from a session started above it.

## Skill

```
output: .agents/skills/<name>/SKILL.md
format: Markdown with YAML frontmatter
metadata:
  name: Normalized construct name
  description: Description followed by Use when and useWhen
body: Prompt followed by serialized additional content
```

## Command

### Support

translated to an explicitly invoked Codex skill

### Output

.agents/skills/x-<name>/SKILL.md

### Policy Output

.agents/skills/x-<name>/agents/openai.yaml

### Metadata

```
name: x- followed by the normalized construct name
description: Command description
```

### Policy

```
allow_implicit_invocation: false
```

### Body

Command prompt followed by serialized additional content

### Invocation

$x-<name>

### Rules

- Nest allow_implicit_invocation under policy in the generated YAML.
- Do not emit deprecated personal custom prompts as repository commands.
- Do not invent a project-local .codex/commands discovery directory.

## Agent

### Output

.codex/agents/<name>.toml

### Format

TOML

### Fields

```
name: Normalized construct name
description: Agent description
developer_instructions: Role introduction, prompt, and serialized additional content
```

### Optional Fields

- model
- model_reasoning_effort
- sandbox_mode

### Rules

- Serialize the complete instruction body as a valid TOML string.
- Omit absent options rather than imposing model or sandbox defaults.
- Do not translate a generic tool list into an invented tools field.

### Qualification

This targets the documented standalone custom-agent format, not older role-registration layouts or every hosted Codex surface.

## Permissions

```
description: Native sandbox and approval configuration govern execution. Generated guidance cannot override the active runtime's controls. Unsupported per-command tool or model settings must be diagnosed.
```

## Checks

- Commands produce both the skill and its explicit-invocation policy file.
- Agent output parses as TOML and contains the three required identity and instruction fields.
- Shadowed or over-budget instruction output is reported before being called usable.
