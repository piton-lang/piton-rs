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
