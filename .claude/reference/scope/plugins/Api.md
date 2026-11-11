# Plugin Api

## Description

A plugin's code imports @piton/plugin, which is bundled with the compiler, and exports what definePlugin returns: a handler for each command its declaration lists. A handler gets a context and the command's arguments, and returns the exit code; returning nothing is 0.
```typescript
import { definePlugin, type Context, type Json } from "@piton/plugin";

async function hello(ctx: Context, args: Record<string, Json>) {
  const compiled = ctx.compile({ workspace: true });
  for (const anchor of compiled.anchors({ exported: true, withinSourceRoot: true })) {
    ctx.stdout(`${anchor.name}\n`);
  }
  ctx.report.done("listed the anchors");
  return 0;
}

export default definePlugin({ commands: { hello } });
```
The module is written in TypeScript with its types, so it is also the reference an author reads. It is in the compiler's source at crates/piton-plugin/api/index.ts.

## Editors

@piton/plugin exists only inside Piton's loader, so an editor's TypeScript can't find it on its own and reports the import as a missing module. A plugin gives it a tsconfig.json whose paths map @piton/plugin to the API's file; then the editor and tsc check the plugin against the API. The test plugin's tsconfig.json, in plugins/test, does this with strict checking. The configuration is for editors only: Piton never reads it, and nothing is emitted.

## Context

What a handler can do. Each method that reaches the machine is checked against the plugin's capabilities, and throws when the plugin didn't declare what it needs.

### Plugin

The plugin's name.

### Terminal

Whether stdout and stderr are terminals, the terminal's width in columns, and whether colour is shown, which NO_COLOR turns off.

### Project

The project the command runs in, from the piton.config.pi in the working directory: its root, source root, entry, output directory and configuration path, and Belay's code root and adapters when it configures Belay. Paths are absolute.

### Settings

The project's settings for the plugin, or null when it lists none.

### Compile

Compiles the project. With workspace, every source under the root, not only what the entry reaches, as the language server does.

### Spawn

Starts a program the plugin may run, with arguments, and optionally a working directory inside the project and environment variables. The program runs in the project root with nothing on its stdin.

### On Path

Whether a program the plugin may run is on PATH.

### Read File

The text of a file inside the project.

### Temporary File

Writes text to a new temporary file and returns its path. It is removed when the command ends.

### Sleep

A promise that resolves after a number of milliseconds.

### Paint

Text in a style, error, warning, success, help, note, name, heading or dim, which is colour on a terminal and plain text otherwise.

stdout and stderr: Write text, removing colour wherever the compiler's own output would.

### Stderr Raw

Writes to stderr exactly as given, terminal control codes included, for a status block a command redraws.

### Report

Problems, held back and printed after everything else with the count last, as every command does: error, warning, help and note; and done and stopped, the outcome line.

### Agents

The agentic tools Piton knows, each with its name, the program it runs and its Belay target, and the fluency prompt `piton agent` gives each.

## Anchors

A compilation's anchors are read with a query. Every part of it is optional, and the anchors that match come ordered by the file they are declared in, then by name.

### Extends

Only anchors with this one in their inheritance chain, named by its module and its name, like the module @piton/test and the name Test. The anchor itself isn't included. A module nothing loaded matches nothing.

### Exported

Only anchors declared with export.

### Concrete

Only anchors that aren't abstract.

### Within Source Root

Only anchors declared in the project's own source, not in tethers/ or a bundled package.

This is how the test plugin finds every test: every exported, concrete anchor in the source root that extends Test, whatever keyword or chain it got there by.
Each anchor comes with its name, the file it is declared in, whether it is exported and abstract, the keyword it was declared with, its resolved properties, and its location: where it names a property itself, or else where its name is, to point a diagnostic at.

## Values

Properties arrive as JSON. A string is its plain text, with references written as the names they point at. A number, boolean, null, list and dictionary are themselves, and an implicit list is a list. An anchor used as a value is not inlined, which could repeat a large part of the project, but named, as an object with $anchor and its name, and a reference is an object with $reference and the name it points at.

## Diagnostics

A problem reported with a location is shown with the source it points at, like the compiler's own diagnostics, under a code the plugin names, or the plugin's name. A compilation's own diagnostics are printed with reportDiagnostics, as `piton check` prints them, and it says whether any were errors.

## Processes

A process's output is read a line at a time as it is written, from stdout and stderr both, each line saying which it came from. Reading ends when the process has exited and every line has been read, and its exit code is then known, or null when a signal stopped it. Several processes run at once by awaiting them together.
