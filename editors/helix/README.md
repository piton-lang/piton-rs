# Piton for Helix

Copy `languages.toml` into `~/.config/helix/languages.toml` (or merge it with
what is there), then copy the query files so Helix can find them:

```
mkdir -p ~/.config/helix/runtime/queries/piton
cp ../tree-sitter-piton/queries/*.scm ~/.config/helix/runtime/queries/piton/
hx --grammar fetch
hx --grammar build
```

`:lsp-workspace-command` and the usual bindings then work against `piton lsp`.
