# Piton for Emacs

```elisp
(add-to-list 'load-path "/path/to/piton/editors/emacs")
(require 'piton-mode)
(add-hook 'piton-mode-hook #'eglot-ensure)
```

Enter after a line that opens a block lands one level in; enter on a blank
line inside a dictionary or anchor lands one level back. `piton--indent-line`
writes both rules out, mirroring `on_type_formatting` in the language server.

Autoformat on save is an option, off by default:

```elisp
(setq piton-format-on-save t)
```

The buffer is then formatted through eglot before every save, and the
formatter leaves commented content alone.

`piton` must be on `PATH`; install it with `cargo xtask install`.

For the tree-sitter variant, install the grammar from
`editors/tree-sitter-piton` and use `piton-ts-mode`.
