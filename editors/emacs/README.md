# Piton for Emacs

```elisp
(add-to-list 'load-path "/path/to/piton/editors/emacs")
(require 'piton-mode)
(add-hook 'piton-mode-hook #'eglot-ensure)
```

`piton` must be on `PATH`; install it with `cargo xtask install`, or set
`piton-executable`. eglot starts it as `piton lsp` for both modes.

## Highlighting

The specification lists Emacs as `["tree-sitter", "lsp"]`. `piton-ts-mode`
highlights with the tree-sitter grammar through `treesit-font-lock-rules` that
mirror `tree-sitter-piton/queries/highlights.scm`; the server adds semantic
tokens on top where eglot supports them. Install the grammar and use the mode:

```elisp
(add-to-list 'treesit-language-source-alist
             '(piton "https://github.com/piton-lang/tree-sitter-piton" "main"))
;; M-x treesit-install-language-grammar RET piton RET
(add-to-list 'auto-mode-alist '("\\.pi\\'" . piton-ts-mode))
```

That is the published grammar repository the Zed extension pins. Emacs clones
a branch rather than a commit (`git clone --branch`), so this names `main`, the
branch `cargo xtask publish-grammar` pushes. Without the grammar,
`piton-ts-mode` falls back to the regex highlighting of `piton-mode`.

## Editor behaviour

- **Enter on a colon line** — a declaration, or a dictionary or anchor
  property with nothing after its colon (`frameworks:`, `config:: dictionary:`)
  — indents the new line one level.
- **Enter on a blank line** inside a dictionary or anchor dedents the new line
  one level. `electric-indent-mode` strips the whitespace from the blank line
  as Enter leaves it, so a blank line with no indentation is measured from the
  line before the run of blanks, one level less for each blank in the run.
- A list item or a merge line never opens a block, even when it ends in a
  colon (`- Settings:` is just the string `Settings:`), and neither does a
  comment or a line of prose with spaces in it (`For example:`). A line of
  prose inside a string that is a single word ending in a colon looks exactly
  like a key without the parser, and is treated as one.
- Both rules are in `piton--indent-line`, for both modes; the grammar is
  line-oriented and cannot express them.

Autoformat on save is an option, off by default:

```elisp
(setq piton-format-on-save t)
```

Each Piton buffer carries a buffer-local `before-save-hook` that formats
through eglot when the option is on at save time. The formatter adds the space
after `//` and touches nothing else that is commented.
