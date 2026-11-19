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
" Only a new line is indented: `o` and `O` cover Enter in insert mode as well
" as the normal-mode commands. Nothing a person types closes a block, so no
" other key re-indents the line they are on. (An empty 'indentkeys' would stop
" 'indentexpr' from running on Enter at all, leaving only 'autoindent'.)
setlocal indentkeys=o,O

if exists("*PitonIndent")
  finish
endif

function! PitonIndent() abort
  let l:above = v:lnum - 1

  " Enter on a blank line inside a dictionary or anchor. After one blank
  " line, the new line keeps the depth of that blank line, so a paragraph
  " break stays in the block. After two or more blank lines, the new line
  " moves one shiftwidth back, and stops at the margin. Thus only the third
  " Enter after text moves the line back. The server answers the same moves
  " in `on_type_formatting` for the editors that ask it to format as you type.
  if l:above > 0 && getline(l:above) =~# '^\s*$'
    let [l:depth, l:position] = s:blank_depth(l:above)
    if l:position >= 2
      return max([0, l:depth - shiftwidth()])
    endif
    return l:depth
  endif

  let l:previous = prevnonblank(l:above)
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

" The depth of the blank line `lnum` and its position in its run of blank
" lines (1 for the first blank line after text).
"
" A blank line keeps its depth as its indent, and this function uses that
" indent when it is there. Vim removes the indent from a blank line when Enter
" leaves it, and `gg=G` also removes it. Then the depth comes from the lines
" above: the start of the run (a line with text, or a blank line with an
" indent) and the count of blank lines after it. The step from position `i` to
" position `i + 1` moves one shiftwidth back only when `i` is 2 or more.
function! s:blank_depth(lnum) abort
  let l:position = 1
  let l:line = a:lnum - 1
  while l:line > 0 && getline(l:line) =~# '^\s*$'
    let l:position += 1
    let l:line -= 1
  endwhile

  if indent(a:lnum) > 0
    return [indent(a:lnum), l:position]
  endif

  " Find the start of the run: the nearest line above with text or an indent.
  " `l:start` is the position of that line (0 for a line with text).
  let l:line = a:lnum - 1
  let l:steps = 1
  while l:line > 0 && getline(l:line) =~# '^\s*$' && indent(l:line) == 0
    let l:steps += 1
    let l:line -= 1
  endwhile
  if l:line == 0
    return [0, l:position]
  endif

  if getline(l:line) =~# '^\s*$'
    let l:depth = indent(l:line)
    let l:start = l:position - l:steps
  else
    let l:depth = indent(l:line)
    if s:opens_block(getline(l:line))
      let l:depth += shiftwidth()
    endif
    let l:start = 0
  endif

  " Count the steps from position `l:start` to `l:position` that move back.
  let l:back = max([0, l:position - max([l:start, 2])])
  return [max([0, l:depth - l:back * shiftwidth()]), l:position]
endfunction

" Whether the line ends by opening a block for the lines beneath it.
function! s:opens_block(line) abort
  " A comment is not code, whatever it ends with.
  if s:is_comment(a:line)
    return 0
  endif

  let l:code = a:line
  if l:code !~ ':\s*$'
    return 0
  endif

  " A list item is never a key, even with a colon at the end: `- Settings:`
  " is just the string `Settings:`. A merge line (`+ ...`, `++ ...`) adds an
  " item to a list the same way. Neither opens a block.
  if l:code =~# '^\s*\%(-\|+\|++\)\%(\s\|$\)'
    return 0
  endif

  let l:body = substitute(l:code, '^\s*', '', '')

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

" Whether the line is a comment. A comment has to be on its own line: at the
" end of a line of code `//` is just text, so `key: // note:` is a key with a
" value and opens nothing, the same as `key: note:`.
function! s:is_comment(line) abort
  return a:line =~# '^\s*//'
endfunction
