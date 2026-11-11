# Plugin Capabilities

## Description

A plugin is code from somewhere else running on the user's machine, so it may do only what its declaration lists. Reading the compiled project is always allowed, since it changes nothing. Everything else is a capability, and anything not listed is refused.
```piton
capabilities:
    process: [claude, codex]
    files: read
    temporaryFiles: true
```

## Process

The programs the plugin may run, by name, looked up on PATH. Only these can be started, and only these can be looked for on PATH. A program is named as it is run, so `claude` allows `claude` and nothing else.

## Files

read lets the plugin read files inside the project root; none, the default, doesn't. A path that leads outside the project is refused either way. A plugin never writes files in the project: what it produces goes to stdout and stderr.

## Temporary Files

true lets the plugin write temporary files, outside the project, each removed when its command ends, like the file OpenCode reads its instructions from.

## Refusal

A refused operation throws in the plugin, saying what it tried and which capability would allow it, like "plugin `hello` may not run `ls`: its declaration does not list it under capabilities.process". The plugin can catch it; uncaught, the command fails with it.

## Unknown

A capability that isn't one of these makes the declaration invalid, so a plugin never runs believing it has something it doesn't.

## Visibility

`piton plugins` shows what each plugin may do, so what a project's plugins can reach is one command away.

## What It Is Not

Capabilities limit what the plugin's own code does. A program it is allowed to run is not sandboxed by them: an agentic tool started by the test plugin can do what that tool allows, which is why the test plugin starts each tool in its own read-only mode where it has one.
