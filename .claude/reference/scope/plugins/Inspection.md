# Plugin Inspection

## Description

A plugin can look at a project before any command runs, and offer to change it: to bring a project written for an older Piton up to date, say. It doesn't change anything itself. Its inspect hook returns what it found, each with the edits that would fix it, and the host shows them and asks.
```typescript
export default definePlugin({
  inspect(ctx) {
    return {
      findings: [{
        message: "piton.config.pi lists frameworks, which is now plugins",
        edits: [{ path: "piton.config.pi", contents: "..." }],
      }],
    };
  },
});
```
A plugin with an inspect hook says so in its declaration with `inspect: true`, so the host knows without running it which plugins to ask.

## When

Before every command, in a project with a piton.config.pi, the host runs each inspect hook, so any command that would fail on an older form offers the migration first. Not before `piton --help` or `--version`, nor before the command of the plugin that inspects, which deals with its findings itself.
Not before `piton lsp` either, whose output is the editor's protocol. The language server asks the hooks itself when a file is saved, and offers each finding as a quick fix on the configuration diagnostic it resolves, `frameworks-removed` say. Accepting the quick fix applies the finding's edits.
`piton check` never writes a file of its own. Before it, the host offers the findings as before every command, but the check runs on the project as it is: if the user says yes, the edits are applied only after the check ends, and the check is not run again.
A hook runs before the project has to compile, so it can help a project that no longer does. It reads what it needs as files, and has to be quick: it runs every time.

## Findings

Each finding is a message saying what is out of date, and the edits that fix it: each a file in the project and its whole new contents. A finding with no edits is only reported. Every edited path has to be inside the project and among the edits the plugin's capabilities list; a finding that isn't is reported as the plugin's error, and nothing of it is offered.

## Asking

On a terminal, when stdin and stderr are both terminals, the host shows each finding with a diff of what would change, on stderr, and asks once, for all of them: Switch it over? Yes is the default.
Yes applies the edits, each file written whole through a staging file, reports each path changed, and runs the command that was asked for again, against the updated project, so it behaves as if the project had always been up to date. Before `piton check`, yes applies the edits after the check instead, as `when` says. No leaves the project as it is, and the command runs as asked.
Anywhere else, nobody is there to answer, so nothing is asked and nothing changes: each finding is printed as a warning offering the command that applies the migration automatically, `piton migrate` say, and the command runs as asked.

## Failures

A hook that fails is reported as a warning, and the command runs as asked. An inspection never stops a command on its own account.
