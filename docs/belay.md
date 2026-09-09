# The Belay framework

Belay is the framework bundled with Piton. It turns Piton declarations into the
Markdown that agentic coding tools load: sub-agents, skills, slash commands, and
per-directory instructions.

Belay is a plugin, not part of the compiler. `piton-core` defines a `Framework`
trait and never names Belay; the `piton` binary registers it in one function.
Another framework could target something else entirely.

## Configuration

```piton
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi    // optional; defaults to `root`

    frameworks:
        - {Belay}

belay-config Belay:
    // Where your application's source code lives.
    codeRoot: ./src

    // A tree that mirrors codeRoot, holding the descriptions of each part.
    shapeRoot: ./spec/shape

    adapters:
        - {ClaudeAdapter}
```

A `belay-config` anchor that is not listed under `frameworks` leaves Belay
unconfigured and nothing is written. The build warns when that happens.

### Adapters

An adapter decides where output goes. `@piton/belay` exports `ClaudeAdapter`
(`.claude/`) and `OpenCodeAdapter` (`.opencode/`); you can also write your own:

```piton
belay-agent-adapter MyAdapter:
    description: Writes into .agents
    agents: true          // or: claude, opencode, codex, cursor
```

or name a directory outright with `directory: .somewhere`. Configure several
adapters and each gets a complete, self-consistent copy of the output.

## The four constructs

Each is an abstract anchor exported as a keyword, so `skill Name:` is shorthand
for `anchor Name extends Skill:`. A concrete anchor may implement only one of
them.

### `agent`

```piton
export agent CodeReviewer:
    description: Reviews a change against the recorded design intent
    role: careful reviewer who values intent over taste
    prompt: Read the diff, then read the shape document for anything you touch.

    // Optional, and passed through to the front matter.
    tools: Read, Grep
    model: opus
```

Compiles to `<agent-dir>/agents/code-reviewer.md`, with `name` (kebab-cased),
`description`, and any `tools` and `model` in the front matter. The body opens
`You are a {role}`, then the prompt, then everything else the anchor declares.

### `skill`

```piton
export skill BuildComponent:
    description: Implements a component from its shape document
    useWhen: the user asks to build or change a component
    prompt: Read the shape document first, then write code to match it.
```

Compiles to `<agent-dir>/skills/build-component/SKILL.md`. The front-matter
description becomes `{description} Use when {useWhen}`, which is the shape
Claude Code matches against.

### `command`

```piton
export command Ship:
    description: Runs the checks before anything leaves the branch
    prompt: Run the tests and `piton build check`, then report.
    allowedTools: Bash, Read
    model: sonnet
```

Compiles to `<agent-dir>/commands/x-ship.md`. Every Belay command is prefixed
`x-` so it cannot collide with a command from somewhere else.

### `instruction`

Instructions are the interesting one: they compile to guidance that sits *beside
the code it applies to*.

```piton
export instruction ButtonComponent:
    description: The clickable button component
    prompt: It should be clickable, and it should have a hover state.
```

The `shapeRoot` tree mirrors `codeRoot`:

```
spec/shape/components/button/Button.pi   ->   src/components/button/AGENTS.md
                                              src/components/button/CLAUDE.md
```

Every instruction written at one scope is concatenated into the one `AGENTS.md`
for the matching code directory, and a `CLAUDE.md` beside it imports that file.
If no code directory matches, the instructions move up to the nearest one that
does, ending at `codeRoot/AGENTS.md`.

Instructions are also published under `<agent-dir>/reference/shape/`, keeping
their structure, so an agent can read the whole shape tree.

## References

`@{Anchor}` points the agent at the compiled file for an anchor:

```piton
export instruction ButtonComponent:
    description: The clickable button component
    prompt:
        It should be clickable.

        For details about the design, read @{ButtonDesign}.
```

becomes, in `src/components/button/AGENTS.md`:

```markdown
For details about the design, read [ButtonDesign](../../../.claude/reference/shape/components/button/ButtonDesign.md).
```

The path is relative to the document that contains it, so the same reference
written into a different document gets a different path. Every document holding
at least one reference ends with a line telling the agent to follow its links.

A reference is a Markdown link rather than Claude Code's `@` import for two
reasons. The import is a memory file feature that no other agent implements,
since opencode and Codex both treat `@path` as plain text, so it would only ever
work in one adapter, and only in the `AGENTS.md` reached through a `CLAUDE.md`
at that. And where it does work it inlines the whole reachable reference tree
into context at launch, up to a limit of four hops, past which files are dropped
without warning. Publishing references as separate files is meant to let an
agent read what it needs, which is the opposite of that.

The one import Belay still writes is the `@AGENTS.md` in a generated
`CLAUDE.md`: that payload you do want loaded eagerly, and it is one hop deep.

`${Anchor}` inserts the anchor's *name* rather than a path, for when you want to
mention something without sending the agent to read it.

Any anchor a reference reaches is published under `<agent-dir>/reference/`,
named after the anchor, so two anchors in one file cannot collide.

## `__BELAY_SHAPE__`

Belay exports the compiled shape directory as a variable:

```piton
from @piton/belay import __BELAY_SHAPE__

export skill BuildFromShape:
    description: Builds a component from its shape document
    useWhen: asked to build a component
    prompt: Build following the structure outlined in {__BELAY_SHAPE__}.
```

## How values become Markdown

Everything Belay writes is prose, so every value is serialised:

| Value | Becomes |
| --- | --- |
| A simple value | Its literal text |
| A list | `- item` lines |
| A dictionary of scalars | Indented `key: value` lines |
| Anything with structure | Headers, one level per depth |
| Depth past six | **Bold**, because Markdown has six heading levels |

Property names are split into words and title-cased, so `useWhen` becomes
`Use When`.
