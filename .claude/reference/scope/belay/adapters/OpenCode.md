# Open Code Adapter

## Description

Compile Belay constructs into project-local OpenCode artifacts.

## Target Id

opencode

## Instruction File

AGENTS.md

## Reference Root

.opencode/reference

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
- Verify discovery metadata and body content are emitted exactly once.
- Verify a command retains the x- prefix after target-name normalization.
- Verify generated filenames and serialized native metadata against the target schema.
- Verify unsupported requested capabilities produce diagnostics before output is committed.

## Instruction

### Output

AGENTS.md at the scope selected by ShapeMapping

### Format

Markdown

### Loading

OpenCode searches local rule files upward from the working directory and prefers AGENTS.md over its CLAUDE.md fallback. Do not assume Claude's nested-loading behavior or Codex's chain.

### Rules

- Preserve scoped output placement without claiming identical activation across tools.
- Use explicit instructions configuration when the selected deployment needs additional files.
- Do not load all nested instructions globally merely to make them discoverable.

### Nested Activation

Only count on what OpenCode documents for startup. Don't assume nested files get picked up, and report an error if shape scoping is needed but can't be guaranteed.

## Skill

### Output

.opencode/skills/<name>/SKILL.md

### Format

Markdown with YAML frontmatter

### Metadata

```
name: Normalized construct name
description: Description followed by Use when and useWhen
```

### Body

Prompt followed by serialized additional content

### Rules

- Require a name of 1 to 64 lowercase alphanumeric characters with single hyphen separators.
- Match name metadata to the containing directory.
- Reject descriptions outside the supported 1 to 1024 character range.
- Do not rely on unrecognized frontmatter to enforce policy.

### Discovery

OpenCode also discovers compatible Claude and .agents skill directories. Check cross-adapter identities before installing several generated skill trees into one project.

## Command

### Support

native

### Output

.opencode/commands/x-<name>.md

### Format

Markdown with YAML frontmatter

### Metadata

```
description: Command description
agent: Explicit target agent when configured
model: Explicit provider-qualified model when configured
```

### Body

Command prompt followed by serialized additional content

### Invocation

/x-<name>

### Rules

- Derive command identity from the filename.
- Preserve explicitly authored runtime argument placeholders.
- Do not emit Claude-specific allowed-tools as an enforced command setting.

## Agent

### Output

.opencode/agents/<name>.md

### Format

Markdown with YAML frontmatter

### Metadata

```
description: Agent description
mode: Explicit primary, subagent, or all selection
model: Explicit provider-qualified model when configured
permission: Explicit native permission map when configured
```

### Body

Role introduction followed by prompt and serialized additional content

### Default Mode

subagent

### Rules

- Derive agent identity from the filename.
- Prefer native permission entries over the deprecated tools option.
- Validate native options instead of copying metadata from another adapter.

## Permissions

```
description: Map permissions only when their meanings are equivalent. Native allow, ask, and deny entries are not interchangeable with a prose tool list. Reject ambiguous conversions.
```

## Checks

- A command keeps its x-prefixed filename and native frontmatter.
- Agent mode is explicit in generated output.
- Duplicate skills discovered through other adapters are reported.
- Required scope behavior that cannot be established for the target version is diagnosed.
