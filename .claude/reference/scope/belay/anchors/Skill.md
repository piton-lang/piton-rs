# Skill Behavior

## Description

A skill is a selectively loaded set of instructions for a particular kind of work. Its discovery metadata explains when it is useful.

## Requirements

- Require useWhen unless the skill is supplied as it is. description and prompt are optional.
- Report an error if the target needs a description and there isn't one.
- Emit the skill into the configured target's skill directory and format.
- Derive the skill name from the anchor using the adapter's naming rules.
- Form the discovery description from description followed by Use when and useWhen.
- Emit prompt as the primary body of the skill.
- Serialize additional properties after the primary prompt.
- Leave skill selection and loading to the consuming platform.
- Copy the folder files names into the skill's output directory, as described in [VendoredFiles](./Skill.md#vendored-files).

# Vendored Files

## Description

A skill can bring files from its own source tree: scripts, data and reference documents that its instructions read beside its SKILL.md. This is how a package carries a skill written for agentic tools directly, without converting it, and lets Piton manage only where it goes and which version is installed.

## Requirements

- Resolve the path against the directory of the file that declares files, or against the source root when it starts with a slash, as imports do.
- Report an error if the path is not a directory inside the project.
- With a dictionary, report an error for a key that is not a target id, and for a target the project builds that the dictionary leaves out.
- With a dictionary, emit nothing for a target whose entry is null, the way a tool's own build can leave a construct out of one tool.
- Copy every regular file under the folder into the skill's output directory, keeping its relative path, its bytes and its execute permission. Nothing in a copied file is rendered, linked or rewritten.
- Report an error for a symbolic link in the folder.
- When the folder holds the skill's SKILL.md, use it as the skill instead of rendering one. The skill then carries no description, useWhen, prompt, native options or other properties, and setting one is an error, because it would be ignored.
- Report an error if a SKILL.md supplied as it is names the skill differently from the name Belay gives it, and if it has no description where the target needs one.
- When the folder has no SKILL.md, render the skill as usual and copy the folder beside it.
- Record every copied file in the manifest, so a later build removes it when the skill no longer brings it.
- When a target reads another target's copy of the skill, write no copy for it, and warn if files names a different folder for it.

## Value

files is a path, or a dictionary from target id to path. A dictionary gives each target its own folder, for a skill that is built differently for each tool.
```piton
export skill Impeccable:
    files: ./vendor/impeccable

export skill Impeccable:
    files:
        claude-code: ./vendor/claude/impeccable
        codex: ./vendor/codex/impeccable
        opencode: ./vendor/opencode/impeccable
```
