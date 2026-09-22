# Piton for Zed

Install `piton` first, so the language server is on `PATH`:

```
cargo xtask install
```

Then add this directory as a dev extension: **zed: install dev extension**, and
choose `editors/zed`.

Zed runs `piton lsp` for diagnostics, completion, hover, navigation, rename,
symbols, inlay hints, code actions, formatting, folding and selection ranges.
The tree-sitter grammar handles highlighting.
