# Piton for Helix

Copy `languages.toml` into `~/.config/helix/languages.toml` (or merge it with
what is there), then copy the query files so Helix can find them:

```
mkdir -p ~/.config/helix/runtime/queries/piton
cp ../tree-sitter-piton/queries/*.scm ~/.config/helix/runtime/queries/piton/
hx --grammar fetch
hx --grammar build
```

Enter after a line that opens a block lands one level in. Helix has no
rule of its own for the blank-line dedent — enter on a blank line inside a
dictionary or anchor comes back one level through the language server's
on-type formatting.

`auto-format` in `languages.toml` is the `autoFormatOnSave` option: it is
`false`, and setting it to `true` formats on save, with commented content
left untouched.

`:lsp-workspace-command` and the usual bindings then work against `piton lsp`.
