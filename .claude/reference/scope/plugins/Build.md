# Plugin Build

## Description

A plugin can produce what `piton build` writes. It doesn't write

### Files

its build hook returns a plan, and the host checks the plan

and writes it. So every plugin gets the same guarantees about what ends up on disk, and the same plan serves a build, a dry run, `piton check`, and the editor, which only reads it.
```typescript
export default definePlugin({
  build(ctx) {
    const compiled = ctx.compile();
    return {
      files: [{ path: "docs/index.md", contents: "# Index\n", origin: "Index" }],
      copies: [],
      diagnostics: [],
    };
  },
});
```
A plugin with a build hook says so in its declaration with `build: true`, so the host knows without running it which plugins take part in a build.

## Plan

```
files: Each file to write: its path relative to the project root, its full contents, the anchors it came from, and the source files it was compiled from.
copies: Each file to copy as it is, from a file the plugin may read to a path in the project, keeping its bytes and its execute permission.
diagnostics: Problems found while planning, each with its severity, code, message, and the location in the source it points at.
locations: Optionally, where each anchor was placed: the file and heading a reference to it lands on, for commands that cite the output instead of the source.
targets: Optionally, labels recorded with the plan in the manifest, like the date each target's documentation was last checked.
```

## Writing

The host writes a plan only when it has no errors, its own or the plugin's. Before writing anything it checks the whole plan.
Every path has to be inside the project, and inside the outputs the plugin declares among its capabilities. Two files, or a file and a copy, can't have the same path unless their contents are the same. Two plugins can't write the same path. A copy's source has to be a file the plugin may read. Each problem is an error naming the path and the plugin, and nothing is written.
Files are written whole, through a staging file, so a build that stops partway never leaves a half-written file. A copy keeps its execute permission.

## Manifest

The host records every path a build wrote in .piton/manifest.json, in path order, with the plans' targets. The next build removes what the last one wrote and this one doesn't, and nothing else, so a build never deletes a file it didn't write. A file listed in the manifest that someone has since edited is still the build's: the manifest says it owns that path.

## Determinism

Identical sources, configuration and toolchain give identical bytes. The host sorts nothing a plugin ordered, so a plugin keeps its output stable by planning in a stable order, and the manifest is written in path order.

## Renderer

A project whose plugins build something gets their output instead of the renderer's, unless the configuration sets output or renderer, in which case it gets both. Without a building plugin, `piton build` writes the renderer's output as before.

## Dry Run

`piton build --dry-run` runs every build hook and checks the plans, reports what would be written and removed, and writes nothing.

## Plan Only

`piton check` runs every build hook too, to report the plans' diagnostics, and writes nothing. The language server runs them when a file is saved, never as you type, so typing stays free of plugin code. It shows their diagnostics, and its compiled-output preview shows the planned files.
