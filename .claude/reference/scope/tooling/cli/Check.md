# Check

## Description

Checks specific files or the project and reports errors.
The check itself never writes a file. Before it, the host offers the findings of inspect hooks as before every command, but the check runs on the project as it is. A migration the user accepts is applied by the host after the check ends, not by the check.

## Command Name

check

## Positional Arguments

```
path: Optional file or glob. Defaults to the project.
```

## Named Arguments

null

## Emit

false

## Validate

- syntax
- imports
- references
- types
- inheritance
- composition
- exports
- circular-dependencies

## Diagnostics

```
errors: true
warnings: true
```

## Exit Code

```
success: 0
errors: 1
```
