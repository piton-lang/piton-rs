# Plugin Reach

## Description

Some anchors are used though nothing in the specbase leads to them. A test is run by the test plugin because it is exported, not because the entry imports or references it. Reach and used or unused anchors are what the plugins say as well as what the entry reaches: a plugin names the anchors it reaches itself, and Piton counts them as roots beside the entry's exports.
```typescript
export default definePlugin({
  reach(ctx) {
    const compiled = ctx.compile({ workspace: true });
    const tests = compiled.anchors({
      extends: { module: "@piton/test", name: "Test" },
      exported: true,
      concrete: true,
      withinSourceRoot: true,
    });
    return { anchors: tests.map((test) => ({ file: test.file, name: test.name })) };
  },
});
```
A plugin with a reach hook says so in its declaration with `reach: true`, so the host knows without running it which plugins to ask.

## Anchors

The hook returns the anchors it reaches, each the file that declares it, as an anchor's file gives it or relative to the project root, and its name. An anchor that names nothing declared is ignored. A file that nothing imports is compiled with the project, so its anchors can be roots.

## Roots

Each anchor a plugin reaches is a root beside the entry's exports, with depth 0, and whatever it imports, references, inherits or embeds is reached through it. Such an anchor is not unreachable and not unused.

## Where

- `piton reach` without targets, described in [Reach](../tooling/cli/Reach.md#reach), adds them to the roots of the project. Named targets ask what those targets see, so plugins add nothing to them.
- The language server, described in [Lsp](../tooling/cli/Lsp.md#lsp), adds them to the roots of its unused check. It asks the plugins when a file is saved, never as you type, and keeps the anchors until the next save.

## When

Only in a project with a piton.config.pi. A reach hook runs when `piton reach` runs and when the editor saves a file. It reads the project and returns anchors: it has to be quick and change nothing.

## Failures

A hook that fails is reported as a warning, and the plugins that didn't fail still count. Reach never fails on a plugin's account.
