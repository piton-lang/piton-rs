# Piton for Helix

Highlighting from the tree-sitter grammar, everything structural from
`piton lsp`.

## Install

Copy `languages.toml` into `~/.config/helix/languages.toml` (or merge it with
what is there). Then install the query files. The shared ones come from the
grammar; `indents.scm` comes from this directory, because the grammar's copy is
written in nvim-treesitter's capture vocabulary (`@indent.begin`) and Helix
reads its own (`@indent`, `@extend`):

```
mkdir -p ~/.config/helix/runtime/queries/piton
cp ../tree-sitter-piton/queries/*.scm ~/.config/helix/runtime/queries/piton/
cp queries/indents.scm ~/.config/helix/runtime/queries/piton/indents.scm
hx --grammar fetch
hx --grammar build
```

The grammar is fetched from `github.com/piton-lang/tree-sitter-piton` at the
same commit the Zed extension pins.

## Editor behaviour

- **Enter on a colon line** (a declaration, or a dictionary or anchor property
  with nothing after its colon) lands one level in, from `queries/indents.scm`.
  A property with a value on its line opens nothing, and neither does a list
  item or a merge line (`- Settings:` is just a string).
- **Enter on a blank line** inside a dictionary or anchor should come back one
  level. Helix cannot do this. Its indentation comes only from tree-sitter
  queries, the grammar is line-oriented, so no node spans a block for a query
  to measure, and Helix does not ask the language server for on-type
  formatting (the server's answer to this rule). A new line after a blank line
  keeps the indentation of the block you are in. Unindent it with `<` in
  normal mode to leave the block.
- `auto-format` in `languages.toml` is the `autoFormatOnSave` option. It is
  `false`. Set it to `true` to format through the language server on save. The
  formatter adds the space after `//` and touches nothing else that is
  commented.

`:lsp-workspace-command` and the usual bindings then work against `piton lsp`.
