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

## Editor behavior

The IDE has no Piton indent rules of its own; the language server answers
both through LSP4IJ's on-type formatting: enter after a line that opens a
block lands one level in, and enter on a blank line inside a dictionary or
anchor lands one level back.

## Formatting on save

There is no save hook to attach `autoFormatOnSave` to: LSP4IJ runs
`textDocument/formatting` on request (the IDE's format action) but does not
format on save. Use that action when formatting is wanted; the formatter
leaves commented content alone.

## Why there is no plugin here

A JetBrains plugin is a Gradle project with its own lexer, parser and PSI
implementation — a second parser for a language that already has one. The
TextMate bundle and the language server give the same result from the
definitions in this repository, and cannot drift from them.
