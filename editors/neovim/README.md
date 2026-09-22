# Piton for Neovim

Copy `piton.lua` into your config and call `require("piton").setup()`.

Enter after a line that opens a block lands one level in; enter on a blank
line inside a dictionary or anchor lands one level back. The file registers
no indent rules of its own, so the language server answers both through
on-type formatting. Indent is four spaces, not tabs.

Autoformat on save is an option, off unless asked for:

```lua
require("piton").setup({ format_on_save = true })
```

The buffer is then formatted through the language server before every save,
and the formatter leaves commented content alone.

The language server runs as `piton lsp` through lspconfig, using
`piton.config.pi` as the root marker. It must be on `PATH`; install it with
`cargo xtask install`.
