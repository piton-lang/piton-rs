# Belay Slice

## Description

Command invokable by the cli by running piton <command>

## Command Name

belay slice

## Positional Arguments

```
target: What to start from: `file.pi#Anchor`, `file.pi#Anchor.property`, `file.pi#variable`, or a bare name to look up in what the build compiles.
```

## Named Arguments

```
adapter: A Belay target the project builds, like `claude-code`. The slice cites what that target compiles.
```

## Plugin

Added by the Belay plugin. It slices like [Slice](../tooling/cli/Slice.md#slice), and cites what one adapter's build compiles instead of the source.

## Citing

The slice is for an agent that reads the compiled reference documents rather than the `.pi` source. Each declaration is placed in the file, and under the heading, that target's build writes it to, and every reference or name the text cites is a link to that place, like `.claude/reference/ui.md#color`. An anchor embedded into another's document is placed in the section that embeds it.
The places are the ones `piton build` writes, so the project is compiled from its own entry, as a build compiles it, and the target has to be something that build writes out.
The source is never cited. Everything the target depends on was compiled somewhere, even when it has no document of its own: an abstract anchor is compiled into the anchors that extend it, and an anchor or variable that is only read is compiled into whatever reads it. Those are described without a place, and names that cite them are not links.

## Paths

Every place is written relative to the directory the command runs in, like `../../.claude/reference/components/Button.md#button` from spec/components. The project is the one the command runs inside.

## How It Works

Belay lays out the adapter's documents the way a build does, and gives the layout to the host; the host finds the slice and prints it, as `piton slice` does, citing those places.

## Diagnostics

A project without a Belay configuration, a target the project doesn't build, a project with errors, a file the build doesn't reach, a name the build doesn't compile, and a target anchor the build never writes out are errors, as are the errors `piton slice` reports.

## Exit Code

```
success: 0
errors: 1
```
