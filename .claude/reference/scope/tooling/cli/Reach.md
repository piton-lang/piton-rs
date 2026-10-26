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

The report ends with two summary lines, and they count exactly what the report lists.
```
37 anchors reachable, 412 unreachable (380 in 50 never-imported files)
53 modules loaded, 2 carrying nothing reachable, 50 never imported
```
The first line counts anchors. Its unreachable count covers every unreachable anchor, including the ones declared in files nothing imports. Those files are never compiled, but reach parses them to count the anchors they declare. So the headline can't say 0 unreachable while files are listed as dead. Whenever there are never-imported files, the line ends with how many of the unreachable anchors are in them and how many files that is, even when the number of anchors is 0.
The second line counts modules. It gives how many were loaded, how many of those carry nothing reachable, and how many files were never imported.
In the unreachable list, each never-imported file is marked with how many anchors it declares, for example (never imported, 8 anchors). Every anchor is counted once. An anchor that lost a duplicate export is counted where it is declared, like any other anchor.

## Diagnostics

```
errors: false
warnings: false
```

## Exit Code

```
success: 0
```
