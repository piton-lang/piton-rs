" Indentation for Piton.
"
" A declaration or a key that ends in a colon opens a block, and the block is
" the indented lines beneath it, so pressing enter there lands one level in.
" Every other line continues where the one above it was.
"
" Vim is the one editor here with neither a tree-sitter grammar nor a client
" that asks the server to format as you type, so the rule is written out.

if exists("b:did_indent")
  finish
endif
let b:did_indent = 1

setlocal autoindent
setlocal indentexpr=PitonIndent()
" Nothing a person types closes a block, so nothing should re-indent the line
" they are on.
setlocal indentkeys=

if exists("*PitonIndent")
  finish
endif

function! PitonIndent() abort
  let l:previous = prevnonblank(v:lnum - 1)
  if l:previous == 0
    return 0
  endif

  let l:text = getline(l:previous)
  " A comment is not structure, whatever it happens to end with.
  if l:text =~ '^\s*//'
    return indent(l:previous)
  endif

  " Four spaces, and not configurable: `piton format` writes them whatever the
  " file already uses.
  if l:text =~ ':\s*$'
    return indent(l:previous) + shiftwidth()
  endif

  return indent(l:previous)
endfunction
