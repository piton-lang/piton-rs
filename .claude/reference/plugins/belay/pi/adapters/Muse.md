# Muse Adapter

## Description

Compile Belay constructs into project-local Muse Code artifacts.

## Target Id

muse

## Instruction File

AGENTS.md

## Reference Root

.muse/reference

## Documentation Checked

2026-10-13

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

The tools read each other's files. Codex, OpenCode and Muse all read AGENTS.md; OpenCode also discovers the skills in .claude/skills and .agents/skills, and Muse those in .agents/skills, .claude/skills and .codex/skills. So with several adapters enabled, Belay plans one project that every enabled tool reads, and writes each artifact once, where the tools that need it will find it. With crossDiscovery set to separate, the trees are deployed apart instead, and each adapter writes its own complete tree.

### Shared References

The first adapter listed owns the reference tree, compiled shape included. Every adapter's links point into it, and the others write no reference tree of their own. Where the tree links to a skill, command, or agent, it links to the owner's. With one adapter, or with crossDiscovery set to separate, each adapter owns its own.

### Unsupported Constructs

When a target has no native form for a construct, like an agent for Muse, it writes nothing for it and the build warns. Links to that construct go to another enabled adapter's artifact for it when the project is shared, or else to its reference document.

### Shared Instructions

An instruction file that several adapters place at one path is written once, by the first of them listed, and the other tools read that file.

### Discovered Skills

An adapter whose tool also discovers another enabled adapter's skill directory writes no copy of a skill that adapter writes there, and links to that one instead. A tool that also honours the other adapters' command translations does the same for commands. A skill the tool discovers in more than one directory is reported, since the tool offers each copy and nothing in the project can stop it, unless the tool itself keeps only one copy.

### Discovered Commands

A command translated into a skill gets its activation from metadata or a policy file. When another enabled tool also discovers that skill and does not honour that metadata or policy, the project has to hide it from that tool with a control the tool enforces, or accept the change by setting crossDiscovery to allow. Otherwise it is an error.

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

AGENTS.md at the scope selected by ShapeMapping

### Format

Markdown

### Loading

Muse reads rules files from the working directory up to the workspace root, and only in a trusted workspace. It reads AGENTS.md and CLAUDE.md; when both sit in one directory, AGENTS.md takes precedence and CLAUDE.md is ignored with a warning. Descendant placement does not guarantee initial loading from a session started above it.

### Rules

- Combine guidance for the same scope in one file.
- Share the AGENTS.md Codex and OpenCode read rather than writing a second rules file.

## Skill

### Output

.agents/skills/<name>/SKILL.md

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

- Always write a description; Muse rejects a SKILL.md without one.
- Do not emit allowed-tools; Muse records it as advisory metadata and enforces nothing.

### Discovery

Muse discovers project skills in .agents/skills, .claude/skills and .codex/skills, in that order of priority. A skill found again in a lower-priority directory is skipped, so Muse offers one copy. With Claude Code or Codex enabled beside it, Muse writes no copy of a skill or command they already write there, and links to theirs.

## Command

```
support: translated to a manually invoked skill
output: .agents/skills/x-<name>/SKILL.md
metadata:
  name: x- followed by the normalized construct name
  description: Command description
  disable-model-invocation: true
body: Command prompt followed by serialized additional content
invocation: /x-<name>
discoveredCommands: Muse honours both translations the other adapters write: Claude's disable-model-invocation and Codex's agents/openai.yaml policy with allow_implicit_invocation false. Either makes the skill user-invocable only, so a command another adapter writes keeps its activation in Muse.
```

## Agent

```
support: unsupported
description: Muse has no project-local agent definition file. Agent definitions come from plugins or from a per-run overlay, neither of which a build can write. An agent is not translated into a skill: Muse gets no file for it, and the build warns with unsupported-construct. Everything else, the reference tree included, is still written.
references: A reference to an agent links to the agent another enabled adapter writes in the same project. With none, the agent is documented in the reference tree like any other referenced anchor.
```

## Permissions

```
description: Muse's approval mode, sandbox and permission profiles govern execution. Generated guidance cannot change them, and Muse accepts no native options from Belay.
```

## Checks

- A command keeps its x-prefixed name and disable-model-invocation.
- A skill or command another enabled adapter writes where Muse discovers it is not written again.
- Every agent is reported as unsupported, and no agent file is written.
