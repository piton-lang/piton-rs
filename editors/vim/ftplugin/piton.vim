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

" `piton format` is the canonical formatter. `-` reads the text from stdin and
" writes the formatted text to stdout, which is what `gq` needs.
setlocal formatprg=piton\ format\ -

" Autoformat on save is an option (the spec's autoFormatOnSave): nothing
" happens until a config sets `g:piton_format_on_save`. When it is on, the
" buffer runs through the same command `formatprg` names, and the formatter
" only adds the space after `//` in a comment and touches nothing else that is
" commented. A failed run (non-zero exit, empty output) leaves the buffer as it
" was, and an unchanged result leaves no undo step behind.
function! s:FormatOnSave() abort
  if !get(g:, 'piton_format_on_save', 0) || !executable('piton')
    return
  endif
  let l:lines = getline(1, '$')
  let l:formatted = systemlist('piton format -', l:lines)
  if v:shell_error || empty(l:formatted) || l:formatted ==# l:lines
    return
  endif
  let l:view = winsaveview()
  silent keepjumps call setline(1, l:formatted)
  if line('$') > len(l:formatted)
    silent keepjumps execute (len(l:formatted) + 1) . ',$delete _'
  endif
  call winrestview(l:view)
endfunction

augroup piton_format_on_save
  autocmd! * <buffer>
  autocmd BufWritePre <buffer> call <SID>FormatOnSave()
augroup END
