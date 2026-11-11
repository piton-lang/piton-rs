# Belay Plugin

## Description

Belay is a plugin, described in [Plugins](../plugins/index.md#plugins), bundled with the compiler. Nothing about Belay is built into the compiler: its constructs and adapters are its Piton files, its build is its TypeScript, and the compiler only provides what any plugin gets.

## Files

Its source is in the compiler's repository at plugins/belay. Its Piton files, in pi, are @piton/belay: the four constructs, the adapters, the belay-config settings anchor, and the special imports. Its declaration is plugin.pi, and its code is in src. The specification here describes what they do and shows their shapes; the anchors themselves are the ones in plugins/belay.

## Settings

A project turns Belay on by listing a belay-config anchor under plugins in piton.config.pi. A project that doesn't gets no Belay output, though it can still use Belay's keywords and imports.
```piton
use @piton/belay
use @piton/config

from @piton/belay import ClaudeCodeAdapter

export piton-config App:
    root: ./spec
    plugins:
        - {BelayConfiguration}

belay-config BelayConfiguration:
    codeRoot: ./src
    adapters:
        - {ClaudeCodeAdapter}
```

## Build

Belay's build is its plan, described in the compilation

### Specification

the instruction files, skills, commands, agents and

reference documents for every configured adapter, with the diagnostics planning finds. The host writes it, owns the manifest, and removes what an earlier build wrote and this one doesn't. Its output is byte for byte what the compilation specification describes.
The build also writes the fluency reference, FLUENCY_PROMPT.md of the project or the copy Belay ships with, as a skill named piton-fluency for each adapter, at that adapter's skill path, like .claude/skills/piton-fluency/SKILL.md. Its description says it is the Piton fluency reference, for reading or changing .pi files and running piton commands, and its body is the reference. It follows the rules of every other skill: an adapter whose tool already discovers another adapter's copy writes none, and it counts for name collisions and cross-target discovery. A tool finds it with its own skill discovery, in a session `piton agent` starts or any other.
Reference documents and the Markdown in every construct are rendered by the host's Markdown renderer, in a link space Belay describes, and frontmatter and agent definitions are encoded by the host's YAML and TOML encoders. The special imports are Belay's placeholders, resolved in each file Belay writes.

## Capabilities

Belay may read files in the project, which vendored skills and agents need, and write its outputs: the agent directories .claude, .codex, .agents, .opencode and .muse, and the instruction files CLAUDE.md and AGENTS.md wherever its shape puts them. It runs the agentic tools for `piton agent`, and writes temporary files that hold the prompt for the tools that read it from a file.

## Commands

- [AgentCli](./AgentCommand.md#agent-cli)
- [BelaySlice](./SliceCommand.md#belay-slice)

## Exports

Belay exports agents, for plugins that launch agentic tools: list, the tools it knows, each with its name, the program it runs and its target; fluency, the full fluency prompt of a tool; prompt, the prompt a tool is given, as text and, for a tool that reads it from a file, the temporary file that holds it; and launch, how to start a tool interactively with that prompt and, optionally, a first message, as a program, arguments and environment. The plugin that uses them runs the tool itself, with its own capabilities. The test plugin requires Belay and uses them.

## Editor

Belay's diagnostics and its per-adapter output preview show in the editor when a file is saved, from the same plan a build writes.
