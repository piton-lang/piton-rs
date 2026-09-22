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
