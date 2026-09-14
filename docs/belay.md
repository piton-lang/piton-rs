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

## The five constructs

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

Only `prompt` is required. The anchor's name is already the heading, so a
`description`, when given, becomes the paragraph beneath it, and when left out
nothing is written in its place.

The `shapeRoot` tree mirrors `codeRoot`:

```
spec/shape/components/button/Button.pi   ->   src/components/button/AGENTS.md
                                              src/components/button/CLAUDE.md
```

Every instruction written at one scope is concatenated into the one `AGENTS.md`
for the matching code directory. If no code directory matches, the instructions
move up to the nearest one that does, ending at `codeRoot/AGENTS.md`.

Claude Code reads `CLAUDE.md`, not `AGENTS.md`, so with the Claude adapter each
`AGENTS.md` gets a `CLAUDE.md` beside it containing only `@AGENTS.md`. Claude
Code loads a subdirectory's `CLAUDE.md` when it reads files there. Other
adapters write no `CLAUDE.md`.

Instructions are also published under `<agent-dir>/reference/shape/`, keeping
their structure, so an agent can read the whole shape tree.

### `self-instruction`

A self-instruction is guidance about the specification itself: how the
documents in one part of the spec tree are written, and what an agent editing
them should know.

```piton
export self-instruction LspScopeGuide:
    description: How the language server scope is written
    prompt: One feature per file, named after the feature.
```

Like an instruction, only `prompt` is required. It differs from an
`instruction` in three ways:

- **It must live under the project `root`.** One that arrives through a library
  or the shared root is an error, because it would write into a tree this
  project does not own.
- **It does not need to be reached.** Belay finds every file under `root` that
  declares one and compiles it, whether or not anything imports it.
- **It compiles into its own directory.** There is no mirroring onto `codeRoot`:

```
spec/scope/lsp/Guide.pi   ->   spec/scope/lsp/AGENTS.md
                               spec/scope/lsp/CLAUDE.md
```

Every self-instruction in one directory is concatenated into that directory's
`AGENTS.md`, with the same companion `CLAUDE.md` an instruction gets. Self-instructions are not published under `reference/shape/`; one
that an `@{}` reference points at is published like any other anchor.

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
work in one adapter. And where it does work it inlines the whole reachable
reference tree into context at launch, up to a limit of four hops, past which
files are dropped without warning. Publishing references as separate files is
meant to let an agent read what it needs, which is the opposite of that.

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

The path is written relative to the document that ends up carrying it, so the
same variable reads correctly from a skill, an agent, or an `AGENTS.md` sitting
at a different depth.

## How values become Markdown

Everything Belay writes is prose, so every value is serialised:

| Value | Becomes |
| --- | --- |
| A simple value | Its literal text |
| A list | `- item` lines |
| A pure dictionary of key/value pairs | Indented `key: value` lines, in a fenced block |
| An anchor, or anything else with structure | Headers, one level per depth |
| Depth past six | **Bold**, because Markdown has six heading levels |

The dictionary and anchor rules are easy to confuse. A *dictionary* that holds
nothing but scalars indents, because it is data — and the indentation is fenced,
because Markdown throws leading whitespace away and the shape is the whole
point. An *anchor* never indents, however flat it looks: its properties are the
sections of a document, so they are always headers. So an anchor with one prose
property becomes a heading and a paragraph, while a dictionary of design tokens
nested under one of those properties still indents beneath its heading.

A dictionary written *among prose* — inside an implicit list, beside the text it
belongs to — is the exception. The author put it there as content, so its keys
become headings like an anchor's, and it goes back to being fenced data only
once it nests another dictionary and the nesting is itself the information.

Property names are split into words and title-cased, so `useWhen` becomes
`Use When`.
