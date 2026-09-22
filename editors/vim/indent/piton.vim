" Indentation for Piton.
"
" A declaration or a key that ends in its colon opens a block, and the block is
" the indented lines beneath it, so pressing enter there lands one level in.
" Every other line continues where the one above it was -- including a line
" whose value merely ends with a colon, because `prompt: Careful: ` ends a
" sentence rather than a property.
"
" Vim is the one editor here with neither a tree-sitter grammar nor a client
" that asks the server to format as you type, so the rule is written out. It
" mirrors `opens_a_block` in the language server; the two move together.

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

  " Four spaces, and not configurable: `piton format` writes them whatever the
  " file already uses.
  if s:opens_block(getline(l:previous))
    return indent(l:previous) + shiftwidth()
  endif

  return indent(l:previous)
endfunction

" Whether the line ends by opening a block for the lines beneath it.
function! s:opens_block(line) abort
  let l:code = s:strip_comment(a:line)
  if l:code !~ ':\s*$'
    return 0
  endif

  " A list item is judged by what it carries: `- key:` opens, and
  " `- a note:` does not.
  let l:body = substitute(l:code, '^\s*++\|^\s*+\|^\s*-\s*', '', '')
  let l:body = substitute(l:body, '^\s*', '', '')

  let l:colon = stridx(l:body, ':')
  if l:colon >= 0 && strpart(l:body, 0, l:colon) =~# '^\S\+$'
    " The line ends where the value would begin: nothing has been written
    " after the colon that opens it.
    return !s:has_value(strpart(l:body, l:colon + 1))
  endif

  " No key on the line. A declaration sits at the margin and opens; indented
  " text without a key is prose, and prose opens nothing.
  return a:line !~# '^\s'
endfunction

" Whether anything but whitespace follows, accounting for the `::` constraint
" chain the way the server does: a colon opens the value, the next closes it,
" and a word written while a chain is open belongs to it rather than being
" the value.
function! s:has_value(after) abort
  if a:after !~# '^:'
    return a:after =~# '\S'
  endif

  let l:open = 0
  let l:i = 0
  while l:i < strlen(a:after)
    let l:ch = a:after[l:i]
    if l:ch ==# ':'
      let l:open = !l:open
    elseif !l:open && l:ch =~# '\S'
      return 1
    endif
    let l:i += 1
  endwhile
  return 0
endfunction

" The line without its comment: a `//` begins one at the line's start or after
" whitespace, which is what keeps the `//` in a URL written in prose from
" swallowing the rest of the line.
function! s:strip_comment(line) abort
  let l:at = match(a:line, '\%(^\|\s\)\zs//')
  return l:at < 0 ? a:line : strpart(a:line, 0, l:at)
endfunction
