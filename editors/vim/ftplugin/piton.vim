" Four spaces per indent level, not tabs. The language prefers it, `piton
" format` applies it, and it is not configurable.
setlocal expandtab
setlocal shiftwidth=4
setlocal softtabstop=4
setlocal tabstop=4
setlocal commentstring=//\ %s
setlocal comments=:// 

" A blank line is an explicit line break inside a string, so joining lines
" changes the value. Leave formatoptions alone rather than reflowing prose.
setlocal formatoptions-=t
setlocal formatoptions-=c

" `piton format` is the canonical formatter.
setlocal formatprg=piton\ format\ /dev/stdin

" Autoformat on save is an option (the spec's autoFormatOnSave): nothing
" happens until a config sets `g:piton_format_on_save`. When it is on, the
" buffer runs through the same command `formatprg` names, and the formatter
" leaves commented content alone. A failed run is undone rather than left in
" the buffer for the save to pick up.
function! s:FormatOnSave() abort
  if !get(g:, 'piton_format_on_save', 0)
    return
  endif
  let l:view = winsaveview()
  silent keepjumps %!piton format /dev/stdin
  if v:shell_error
    silent undo
  endif
  call winrestview(l:view)
endfunction

augroup piton_format_on_save
  autocmd! * <buffer>
  autocmd BufWritePre <buffer> call <SID>FormatOnSave()
augroup END
