# Piton for VS Code

Syntax highlighting from a TextMate grammar, and everything else from the
language server.

## Installing

The extension needs the `piton` binary on `PATH`:

```
cargo xtask install
```

Then, from this directory:

```
npm install
code --install-extension .
```

Or symlink it into `~/.vscode/extensions/piton` for development.

## What comes from where

The TextMate grammar colours tokens. Diagnostics, completion, hover,
go-to-definition, find-references, rename, symbols, semantic highlighting,
inlay hints, code actions, formatting, folding, and selection ranges all come
from `piton lsp`, because they need the resolved program rather than the text.

Set `piton.server.path` if the binary is not on `PATH`, or
`piton.server.enabled` to `false` to use highlighting alone.

`piton.formatOnSave` formats the document before it is saved — the
`autoFormatOnSave` option. It is `false` unless you turn it on. The formatter
adds the space after `//` and touches nothing else that is commented.

Enter on a colon line (a declaration, or a dictionary or anchor property with
nothing after its colon) lands one level in; Enter on a blank line inside a
dictionary or anchor lands one level back. Both are `onEnterRules` in
`language-configuration.json`. A list item or a merge line never opens a block,
even when it ends in a colon (`- Settings:` is just a string), and Enter does
not continue a list by itself. The language server answers the same two moves
through on-type formatting if you turn on `editor.formatOnType`.

The blank-line rule reads the whitespace VS Code leaves on the line it just
indented, so it applies while you are typing. A blank line with no whitespace
at all (one already in the file, or after `editor.trimAutoWhitespace` removed
it) gives the rule nothing to measure; `editor.formatOnType` covers that case
through the server.

## Commands

| Command | |
| --- | --- |
| **Piton: Show Compiled Output for This Construct** (`piton.sourceToOutput`) | Opens the compiled file the construct under the cursor produced, at the place it produced. The server's code lenses run the same command. |
| **Piton: Go to Piton Source** (`piton.outputToSource`) | From a compiled file, jumps to the Piton construct that produced the text under the cursor. |
| **Piton: Preview Compiled Output** (`piton.previewOutput`) | Runs `piton compile --renderer <json\|yaml\|markdown>` on the current file and shows the result beside it. |

A compiled file has to exist on disk for the first two; build the project
first.
