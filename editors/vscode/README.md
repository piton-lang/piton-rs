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
