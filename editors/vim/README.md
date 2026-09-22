# Piton for Vim

Copy `syntax/`, `ftdetect/`, `ftplugin/` and `indent/` into `~/.vim/`, or point
a plugin manager at this directory.

Enter after a line that opens a block lands one level in; enter on a blank
line inside a dictionary or anchor lands one level back. The indent file
writes both rules out, mirroring `on_type_formatting` in the language server.

Autoformat on save is an option, off until a config turns it on:

```vim
let g:piton_format_on_save = 1
```

The buffer then runs through `piton format` before every save, which leaves
commented content alone. A failed run is undone rather than saved.

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
