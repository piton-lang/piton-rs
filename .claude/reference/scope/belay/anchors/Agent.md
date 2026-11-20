# Agent Behavior

## Description

An agent describes a role and the instructions for performing it, expressed in the format understood by the target platform.

## Requirements

- role is a required property of the Agent anchor, so the compiler reports an agent without it as soon as the file changes, without a plugin build. description and prompt are optional. An agent that a native file supplies is a vendored-agent, which has no role.
- Report an error if the target needs a description and there isn't one.
- Emit the agent into the configured target's agent directory and format.
- Use the kebab-case anchor name as agent identity unless the target requires another form.
- Emit description as discovery metadata.
- Begin the body with You are a followed by the role.
- Emit prompt after the role introduction.
- Map explicit tools and model settings only through supported target configuration fields.
- Serialize additional properties after the primary prompt.
- Report an error for an agent that names a file, and tell the author to declare it as a vendored-agent, as described in [VendoredAgent](./Agent.md#vendored-agent), which piton migrate does.

# Vendored Agent

## Description

An agent can be supplied as the target's own definition file, used as it is, so a package keeps a tool's native agent settings, such as Claude Code's tools and model, that Belay cannot carry portably. Such an agent is declared with the vendored-agent keyword, from the VendoredAgent anchor of @piton/belay. Its file is required, and it has no role.

## Requirements

- Resolve file, and report problems with it, as [VendoredFiles](./Skill.md#vendored-files) does for a skill's files.
- Write the file's bytes to the agent's output path unchanged.
- Report an error if its extension is not the one the target's agent format uses.
- Report an error if it names the agent differently from the name Belay gives it, for a target whose agent format carries a name, and if it has no description where the target needs one.
- Report an error for any other property set on the agent, because it would be ignored.

## Value

file is a path, or a dictionary from target id to path, resolved like a skill's files.
```piton
export vendored-agent ImpeccableDocumenter:
    file:
        claude-code: ./vendor/claude/agents/impeccable-documenter.md
        codex: ./vendor/codex/agents/impeccable-documenter.toml
        opencode: ./vendor/opencode/agents/impeccable-documenter.md
```
