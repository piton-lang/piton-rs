# Piton for JetBrains IDEs

JetBrains IDEs read TextMate bundles and speak LSP, so Piton needs no compiled
plugin. Both halves are set up from the IDE.

## Highlighting

The TextMate bundle is `editors/vscode` — an extension directory with a
`package.json` and a `syntaxes/` folder is a valid bundle.

**Settings → Editor → TextMate Bundles → +** and select `editors/vscode`.

## Language server

Requires a JetBrains IDE with LSP support (2023.2 or newer, Ultimate-tier) and
the **LSP4IJ** plugin, which works everywhere else.

With LSP4IJ: **Settings → Languages & Frameworks → Language Servers → +**

| Field | Value |
| --- | --- |
| Name | Piton |
| Command | `piton lsp` |
| File name patterns | `*.pi` |

`piton` must be on `PATH`; install it with `cargo xtask install`.

## Why there is no plugin here

A JetBrains plugin is a Gradle project with its own lexer, parser and PSI
implementation — a second parser for a language that already has one. The
TextMate bundle and the language server give the same result from the
definitions in this repository, and cannot drift from them.
