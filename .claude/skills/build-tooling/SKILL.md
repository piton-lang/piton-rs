---
name: build-tooling
description: Read the spec for the Piton language, Belay framework and all associated concepts required to build the tooling and build the tooling. Use when Explicitly Invoked
---

Read the complete spec for the Piton, Belay framework, and all associated features, tooling, etc under .claude/reference and understand the everything.  Then build the tooling as described completely and thoroughly.

# Stack

- Rust
- chumsky
- rowan
- tower-lsp

# Xtask

```
install: Should build the release and install it locally.
sync-fluency: Regenerates FLUENCY_PROMPT.md at the repository root and copies it over crates/piton-cli/src/fluency.md, the copy `piton agent` falls back to and that is compiled into the binary, then says to rebuild.
  The prompt is written by an agent, so generating runs Claude Code headless, with `claude -p`, on the GenerateFluencyPrompt skill, in the repository root. Claude may read and search the repository and write files, with edits accepted since nobody is there to approve them, and may not run commands. `--model` chooses its model. Claude's output is shown as it finishes, and whether the prompt is right is for whoever runs the task to read in the diff.
  Claude failing, not being on PATH, or finishing without writing a non-empty FLUENCY_PROMPT.md is an error, and nothing is copied, so the built-in copy is never overwritten with nothing. A prompt that came out unchanged is said to be unchanged, and is still copied.
  `--copy-only` skips generating and copies the prompt as it is, for one written by hand or by running the skill interactively. `--check` generates and copies nothing, and fails when the two copies differ, naming the command that fixes it, for CI.
```
