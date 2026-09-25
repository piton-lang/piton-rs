# Slice

## Description

Prints the part of the specification one anchor, property, or variable depends on, as a single document to hand to an agent working on that one thing.
It answers what someone needs in order to understand and implement the target, not which files happen to be near it. Imports alone bring nothing in: a declaration is included because the target, or something the target needs, actually depends on it.

## Command Name

slice

## Positional Arguments

```
target: What to start from: `file.pi#Anchor`, `file.pi#Anchor.property`, `file.pi#variable`, or a bare name to look up across the project.
```

## Named Arguments

```
adapter: A Belay target the project builds, like `claude-code`. The slice cites what that target compiles instead of the source.
```

## Emit

false

## Traverse

- inheritance
- type constraints
- reads
- references
- composition

## Direction

outgoing

## Include

- direct
- transitive

## Semantics

A whole anchor needs every property it has and every anchor it extends.
A property needs every base that declares it too, the anchors its type constraint names, whatever its value read to be computed, like `${Other.name}` or a variable, and whatever its value refers to or embeds. It does not need the anchor's other properties, so slicing a property is smaller than slicing its anchor.
A reference to an anchor needs the whole anchor, and a reference to a property needs only that property.
A variable needs what its value read, refers to, or embeds, and the anchors its type constraint names.

## Chain

The target is also placed where it sits in the project. The chain is the shortest way the build gets from the project's entry to the target's anchor: starting at the entry's exports, following what each property's value refers to or embeds, like `Specification.Tooling`, then `Tooling.cli`, then `Cli.commands` for a command the CLI lists.
Each link is only that one property. What else it refers to or embeds is not followed, since the target doesn't need it, and following it from the entry would bring in the whole specification.
The chain is traced from the project's entry even when the target is given as a file, so the project is compiled as a build compiles it. A file the build doesn't reach, an export of the entry, and a variable have no chain.

## Output

One Markdown document on stdout. The target comes first, then what it depends on, nearest first, with source order breaking ties, then the chain from the entry, outermost first, so the same source always prints the same bytes. The introduction names the chain's links in order.
Each declaration appears once, headed by its name as written in the source, like `SaveButton` or `SaveButton.color`, with the file it is declared in, what it extends, its type, and what it reads. Values are written the way the Markdown renderer writes them, except that a reference or an embedded anchor is written as its name, since the declaration it names is in the same document.
A cycle is followed until it comes back around, and every dependency in it is still listed, so it stays visible.

## Adapter

With `--adapter`, the slice is for an agent that reads the compiled reference documents rather than the `.pi` source. Each declaration is placed in the file, and under the heading, that target's build writes it to, and every reference or name the text cites is a link to that place, like `.claude/reference/ui.md#color`. An anchor embedded into another's document is placed in the section that embeds it.
The places are the ones `piton build` writes, so the project is compiled from its own entry, as a build compiles it, and the target has to be something that build writes out.
The source is never cited. Everything the target depends on was compiled somewhere, even when it has no document of its own: an abstract anchor is compiled into the anchors that extend it, and an anchor or variable that is only read is compiled into whatever reads it. Those are described without a place, and names that cite them are not links.

## Diagnostics

### Errors

true

### Warnings

false

A file that doesn't exist, a name that isn't declared or imported where it's looked up, a property the anchor doesn't have, a bare name declared in more than one file, and a path that reaches inside a property are all errors.
With `--adapter`, so are a project without a Belay configuration, a target the project doesn't build, a file the build doesn't reach, and a target anchor the build never writes out.

## Exit Code

```
success: 0
errors: 1
```
