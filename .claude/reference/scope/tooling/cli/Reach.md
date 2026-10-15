# Reach

## Description

Analyzes which parts of the specbase are reachable from specific files, anchors, or the project

## Command Name

reach

## Positional Arguments

```
target: Optional file, glob, or anchor. Defaults to the project.
```

## Named Arguments

null

## Emit

false

## Traverse

- imports
- references
- inheritance
- composition

## Composition

Composition covers everything a value is built from, including what evaluation folded into it. Embedding a top-level variable (`{PURPOSE}`) reaches the module that declares it, and whatever that variable embeds or reads in turn. Reading another anchor's property (`${Other.name}`) reaches that anchor through composition.

## Configuration

`piton.config.pi` is read rather than imported. It is never reported as unreachable.

## Direction

outgoing

## Include

- direct
- transitive

## Report

- reachable
- unreachable
- depth
- paths

## Group By

source

## Summary

true

## Diagnostics

```
errors: false
warnings: false
```

## Exit Code

```
success: 0
```
