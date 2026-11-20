# Skill Behavior

## Description

A skill is a selectively loaded set of instructions for a particular kind of work. Its discovery metadata explains when it is useful.

## Requirements

- useWhen is a required property of the Skill anchor, so the compiler reports a skill without it as soon as the file changes, without a plugin build. description and prompt are optional. A skill that its folder supplies whole is a vendored-skill, which has no useWhen, as described in [VendoredFiles](./Skill.md#vendored-files).
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
A skill that Belay renders names its folder with files, and keeps its useWhen. A skill whose folder holds its own SKILL.md is declared with the vendored-skill keyword instead, from the VendoredSkill anchor of @piton/belay. Its files is required, and it has no useWhen, prompt or other properties, because the SKILL.md is the whole skill. The keyword says which of the two a skill is, so the compiler can require useWhen of every skill that Belay renders.

## Requirements

- Resolve the path against the directory of the file that declares files, or against the source root when it starts with a slash, as imports do.
- Report an error if the path is not a directory inside the project.
- With a dictionary, report an error for a key that is not a target id, and for a target the project builds that the dictionary leaves out.
- With a dictionary, emit nothing for a target whose entry is null, the way a tool's own build can leave a construct out of one tool.
- Copy every regular file under the folder into the skill's output directory, keeping its relative path, its bytes and its execute permission. Nothing in a copied file is rendered, linked or rewritten.
- Report an error for a symbolic link in the folder.
- For a vendored-skill, use the SKILL.md in the folder as the skill instead of rendering one. The skill carries no description, prompt, native options or other properties, and setting one is an error, because it would be ignored. Report an error if the folder has no SKILL.md.
- For a skill, report an error if the folder holds a SKILL.md, and tell the author to declare it as a vendored-skill, which piton migrate does.
- Report an error if a SKILL.md supplied as it is names the skill differently from the name Belay gives it, and if it has no description where the target needs one.
- For a skill, render it as usual and copy the folder beside it.
- Record every copied file in the manifest, so a later build removes it when the skill no longer brings it.
- When a target reads another target's copy of the skill, write no copy for it, and warn if files names a different folder for it.

## Value

files is a path, or a dictionary from target id to path. A dictionary gives each target its own folder, for a skill that is built differently for each tool.
```piton
export vendored-skill Impeccable:
    files: ./vendor/impeccable

export vendored-skill Impeccable:
    files:
        claude-code: ./vendor/claude/impeccable
        codex: ./vendor/codex/impeccable
        opencode: ./vendor/opencode/impeccable
```
