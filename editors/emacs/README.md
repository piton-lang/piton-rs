# Piton for Emacs

```elisp
(add-to-list 'load-path "/path/to/piton/editors/emacs")
(require 'piton-mode)
(add-hook 'piton-mode-hook #'eglot-ensure)
```

`piton` must be on `PATH`; install it with `cargo xtask install`.

For the tree-sitter variant, install the grammar from
`editors/tree-sitter-piton` and use `piton-ts-mode`.
