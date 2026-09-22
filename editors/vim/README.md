# Piton for Vim

Copy `syntax/`, `ftdetect/` and `ftplugin/` into `~/.vim/`, or point a plugin
manager at this directory.

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
