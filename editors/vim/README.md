# Piton for Vim

Copy `syntax/`, `ftdetect/`, `ftplugin/` and `indent/` into `~/.vim/`, or point
a plugin manager at this directory.

Enter on a colon line (a declaration, or a dictionary or anchor property with
nothing after its colon) lands one level in; Enter on a blank line inside a
dictionary or anchor lands one level back. `indent/piton.vim` writes both rules
out, mirroring `on_type_formatting` in the language server, and runs on Enter
and on `o`/`O` (`indentkeys=o,O`). A list item or a merge line never opens a
block, even when it ends in a colon (`- Settings:` is just a string), and
neither does a comment.

`formatprg` is `piton format -` (stdin to stdout), so `gq` formats through the
compiler.

Autoformat on save is an option, off until a config turns it on:

```vim
let g:piton_format_on_save = 1
```

The buffer then runs through `piton format -` before every save. The
formatter adds the space after `//` and touches nothing else that is
commented. A failed run leaves the buffer as it was.

Highlighting works on its own. For everything else, Vim needs an LSP client
(vim-lsp, coc.nvim, ALE); point it at `piton lsp`. With vim-lsp:

```vim
if executable('piton')
  au User lsp_setup call lsp#register_server({
    \ 'name': 'piton',
    \ 'cmd': {server_info->['piton', 'lsp']},
    \ 'allowlist': ['piton'],
    \ 'root_uri': {server_info->lsp#utils#path_to_uri(
    \     lsp#utils#find_nearest_parent_file_directory(
    \       lsp#utils#get_buffer_path(), 'piton.config.pi'))},
    \ })
endif
```
