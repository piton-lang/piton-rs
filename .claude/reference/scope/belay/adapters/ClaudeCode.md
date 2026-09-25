# Claude Code Adapter

## Description

Compile Belay constructs into project-local Claude Code artifacts.

## Target Id

claude-code

## Instruction File

CLAUDE.md

## Reference Root

.claude/reference

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

The tools read each other's files. Codex and OpenCode both read AGENTS.md, and OpenCode also discovers the skills in .claude/skills and .agents/skills. So with several adapters enabled, Belay plans one project that every enabled tool reads, and writes each artifact once, where the tools that need it will find it. With crossDiscovery set to separate, the trees are deployed apart instead, and each adapter writes its own complete tree.

### Shared References

The first adapter listed owns the reference tree, compiled shape included. Every adapter's links point into it, and the others write no reference tree of their own. Where the tree links to a skill, command, or agent, it links to the owner's. With one adapter, or with crossDiscovery set to separate, each adapter owns its own.

### Shared Instructions

An instruction file that several adapters place at one path is written once, by the first of them listed, and the other tools read that file.

### Discovered Skills

An adapter whose tool also discovers another enabled adapter's skill directory writes no copy of a skill that adapter writes there, and links to that one instead. A skill the tool discovers in more than one directory is reported, since the tool offers each copy and nothing in the project can stop it.

### Discovered Commands

A command translated into a skill gets its activation from metadata or a policy file only its own tool reads. When another enabled tool also discovers that skill, the project has to hide it from that tool with a control the tool enforces, or accept the change by setting crossDiscovery to allow. Otherwise it is an error.

### Requirements

- Check the complete output plan before writing.
- Reject incompatible writes to a shared path, other than an instruction file placed by several adapters.
- Write identical shared output once.
- Report a skill a tool discovers more than once.
- Report a command that another tool would offer as an ordinary skill, unless the project hides it or accepts the change.

## Validation

- Verify all generated relative links point to planned outputs.
- Pass Markdown links an author wrote in prose through as written, without verifying them; Piton has no link syntax, so they are text.
- Verify discovery metadata and body content are emitted exactly once.
- Verify a command retains the x- prefix after target-name normalization.
- Verify generated filenames and serialized native metadata against the target schema.
- Verify unsupported requested capabilities produce diagnostics before output is committed.

## Instruction

### Output

CLAUDE.md at the scope selected by ShapeMapping

### Format

Markdown

### Rules

- Combine guidance for the same scope in one file.
- Preserve a separate compiled shape reference beneath .claude/reference/shape.

### Loading

Ancestor guidance loads at startup; nested guidance loads when Claude reads within that subtree. Placement does not make every nested instruction part of the initial context.

## Skill

### Output

.claude/skills/<name>/SKILL.md

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

- Use the same normalized name for the directory and metadata.
- Preserve native allowed-tools and model options only when explicitly configured.

## Command

```
support: translated to a manually invoked Claude skill
output: .claude/skills/x-<name>/SKILL.md
metadata:
  name: x- followed by the normalized construct name
  description: Command description
  disable-model-invocation: true
body: Command prompt followed by serialized additional content
invocation: /x-<name>
rationale: Claude unifies custom commands with skills. The older commands directory remains supported, but this adapter chooses skills for new output. Do not emit both representations of one command.
```

## Agent

### Output

.claude/agents/<name>.md

### Format

Markdown with YAML frontmatter

### Metadata

```
name: Normalized construct name
description: Agent description
tools: Explicit native tool list when configured
model: Explicit native model selector when configured
```

### Body

Role introduction followed by prompt and serialized additional content

### Rules

- Preserve agent identity independently of display headings.
- Omit optional settings when absent so native defaults remain effective.

## Permissions

```
description: Skill allowed-tools grants and subagent tool selection serve different purposes. Do not interpret either as a portable sandbox definition or copy it into another platform without a mapping.
```

## Checks

- A command produces one x-prefixed skill with automatic invocation disabled.
- Two constructs normalizing to the same skill identity cause a collision diagnostic.
- Nested instructions retain their mapped scope and target-relative links.
