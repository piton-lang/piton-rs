# The agentic text editor

A complete Piton project. It describes a plain text editor — Notepad's feature
set, written in Rust with egui — and compiles to the agents, skills, commands
and per-directory `CLAUDE.md` files that a coding tool loads before it writes
any of it.

The `src/` tree is empty on purpose. It is where the editor gets built, and the
guidance for building it is already sitting in each of its directories.

```sh
piton build
```

## Three trees, three questions

```
spec/
  agent/      Who reviews, what runs, which skills exist
    agents/       IntentReviewer, InteractionAuditor
    commands/     Check, Run
    skills/       BuildFromShape, AddEditorAction, ApplyStyle
  concept/    How the editor fits together, and why
  shape/      What each part is — mirrors src/, one document per directory
```

**`concept/`** is the wiring. Each concept is a small page that says what an
idea is and points at the parts that carry it — `Application` points at
`Layout`, which points at `MenuBar`, `Toolbar`, `EditorSurface` and
`StatusBar`. Concepts declare a keyword of their own, in `concept/Concept.pi`:

```piton
export abstract anchor Concept as concept:
    summary:: string
    parts:: list
```

so every concept has to say what it is and what realises it, and a reader can
walk the whole design from `Application` without being told where to look next.

**`shape/`** is the detail, without the context. `shape/ui/header/MenuBar.pi`
says how the menu bar is built and nothing about why the header exists. Because
the tree mirrors `src/`, each of those documents lands in the directory it
describes:

```
spec/shape/ui/header/MenuBar.pi  ->  src/ui/header/AGENTS.md
spec/shape/ui/header/Toolbar.pi  ->  src/ui/header/AGENTS.md
                                     src/ui/header/CLAUDE.md
```

Both documents written at that scope concatenate into the one `AGENTS.md`, and
the `CLAUDE.md` beside it imports that file, so an agent working in
`src/ui/header` reads both without being asked to.

## What is worth copying

- **One table, many renderings.** `shape/app/Actions.pi` is an `anchor`, not an
  `instruction`: every menu item, toolbar button and keyboard shortcut is one
  row in it, and `MenuBar`, `Toolbar` and the `AddEditorAction` skill all point
  at the same table rather than restating it.
- **`@{Name}` instead of repetition.** A reference compiles to the path of the
  other document, worked out per file, so the same sentence in a skill and in an
  `AGENTS.md` gets two different relative paths and both of them resolve.
- **Inheritance for things that come in pairs.** `LightTokens` and `DarkTokens`
  both extend `Tokens`, so the two palettes cannot drift apart in shape, only in
  value.
