# Piton for Neovim

The specification lists Neovim's highlighter as `vim`: highlighting and the
Enter rules come from the Vim runtime files in [`../vim`](../vim), and
everything else from the language server.

## Install

Keep the repository checkout (or copy `editors/vim` and `editors/neovim`
together, side by side), put `piton.lua` where `require` can find it, and call
setup:

```lua
-- e.g. with the checkout at ~/src/piton
package.path = package.path .. ";" .. vim.fn.expand("~/src/piton/editors/neovim/?.lua")
require("piton").setup()
```

`setup()` prepends `editors/vim` to `'runtimepath'`, so `syntax/piton.vim`,
`indent/piton.vim` and `ftplugin/piton.vim` load for `.pi` files with nothing
else to configure. If `piton.lua` lives somewhere without a sibling `vim/`
directory, say where it is:

```lua
require("piton").setup({ vim_runtime = "/path/to/editors/vim" })
```

The `piton` binary must be on `PATH` (`cargo xtask install`), or pass
`command = "/path/to/piton"`.

## Editor behaviour

- **Enter on a colon line** (a new dictionary or anchor property, or a
  declaration) lands one level in. **Enter on a blank line** inside a
  dictionary or anchor lands one level back. Both rules are written out in
  `indent/piton.vim`. A list item (`- Settings:`) or a merge line (`+ ...`,
  `++ ...`) never opens a block, because a list item is never a key.
- On Neovim 0.12+, `setup()` also enables LSP on-type formatting
  (`vim.lsp.on_type_formatting.enable`) for the Piton server, which answers
  the same two rules. Turn it off with `on_type_formatting = false`.
- `formatprg` is `piton format -`, so `gq` formats through the compiler.
- Indent is four spaces, not tabs.

Autoformat on save is an option, off unless asked for:

```lua
require("piton").setup({ format_on_save = true })
```

The buffer is then formatted through the language server before every save.
The formatter adds the space after `//` and touches nothing else that is
commented.

## Language server

On Neovim 0.11+ the server is registered with the built-in `vim.lsp.config`
and `vim.lsp.enable`; older versions go through nvim-lspconfig. It runs as
`piton lsp`, using `piton.config.pi` (or `.git`) as the root marker. Pass
`server = { ... }` to extend the configuration.

## Tree-sitter (optional)

If nvim-treesitter is installed, `setup()` registers the published grammar
(`github.com/piton-lang/tree-sitter-piton`, at the revision the Zed extension
pins) for both its `master` and `main` branches. Run `:TSInstall piton` to
build it. Enabling tree-sitter *highlighting* for Piton is fine. Leave
tree-sitter *indentation* off for Piton: the grammar is line-oriented, so its
`indents.scm` can indent after a colon line but cannot express the blank-line
dedent. `indent/piton.vim` handles both rules.
