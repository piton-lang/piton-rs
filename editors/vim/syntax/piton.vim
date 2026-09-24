" Vim syntax file for Piton.
"
" Piton is whitespace-structured and most of its content is prose, so this
" highlights the structure and leaves the prose alone. Anything deeper — what a
" name resolves to, which base supplied a value — needs the language server.

if exists("b:current_syntax")
  finish
endif

" Fenced and escaped regions are verbatim, so they are matched first and
" everything inside them is left as written.
syn region pitonFence matchgroup=pitonFenceDelim start="^\s*```\+" end="^\s*```\+\s*$" keepend contains=pitonEscapeDelim
syn match pitonEscapeDelim "^\s*\\\+\s*$" contained
syn region pitonEscape matchgroup=pitonEscapeDelim start="^\s*\z(\\\+\)\s*$" end="^\s*\z1\s*$" keepend

" Imports
syn match pitonImport "^\s*\<use\>" nextgroup=pitonPath skipwhite
syn match pitonImport "^\s*\<from\>" nextgroup=pitonPath skipwhite
syn match pitonPath "\S\+" contained nextgroup=pitonImportWord skipwhite
syn keyword pitonImportWord import export contained
syn match pitonStar "\*"

" Declarations: `[export] [abstract] <keyword> Name [as kw] [extends A, B]:`
syn match pitonDeclaration "^\s*\%(export\s\+\)\?\%(abstract\s\+\)\?[a-z][a-z0-9-]*\s\+[A-Za-z_][A-Za-z0-9_]*.*:\s*$"
      \ contains=pitonStorage,pitonModifier,pitonKeywordName,pitonAnchorName,pitonAs,pitonExtends,pitonBase
syn keyword pitonStorage export contained
syn keyword pitonModifier abstract contained
syn keyword pitonAs as contained nextgroup=pitonAlias skipwhite
syn match pitonAlias "[a-z][a-z0-9-]*" contained
syn keyword pitonExtends extends contained
syn match pitonAnchorName "\<[A-Z][A-Za-z0-9_]*\>" contained
syn match pitonKeywordName "\<anchor\>" contained

" A key is any text without spaces followed by a colon. Keywords are keys here
" too: the colon is what separates `anchor Name:` from `anchor: a description`.
syn match pitonProperty "^\s*\%(export\s\+\)\?[^ \t:]\+\ze\%(::[^:]*\)*:\%(\s\|$\)" contains=pitonStorage
syn match pitonConstraint "::\s*\%(extends\s\+\)\?[A-Za-z_][A-Za-z0-9_]*\%(\[\]\)\?" contains=pitonType,pitonExtends
syn keyword pitonType string number boolean null list dictionary anchor reference any simple complex contained

" `pass` is an intentionally empty body.
syn match pitonPass "^\s*\<pass\>\s*$"

" Structure markers
syn match pitonListMarker "^\s*-\ze\%(\s\|$\)"
syn match pitonMergeMarker "^\s*++\?\ze\%(\s\|$\)"

" Interpolation. Four sigils: intrinsic, string, numeric, and reference.
syn region pitonInterp matchgroup=pitonSigil start="\%(\$\|#\|@\)\?{" end="}" contains=pitonConstant,pitonSelf,pitonNumber,pitonString,pitonOperator,pitonInterp,pitonAnchorRef oneline
syn keyword pitonConstant true false null contained
syn keyword pitonSelf this self super contained
syn match pitonNumber "\<\d[0-9_]*\%(\.[0-9_]\+\)\?\>" contained
syn region pitonString start=+"+ skip=+\\.+ end=+"+ contained
syn match pitonOperator "++\|==\|!=\|<=\|>=\|&&\|||\|[-+*/%<>!?:.]" contained
syn match pitonAnchorRef "\<[A-Z][A-Za-z0-9_]*\>" contained

" A comment has to be on its own line. At the end of a line of code `//` is
" just text, so `url: https://example.com // note` is all value. Defined last
" so that it wins over a key or a declaration starting at the same column:
" `//note: x` is a comment, not the key `//note`. Escape blocks and fences are
" regions that contain no comment, so a `//` line inside one stays literal.
syn match pitonComment "^\s*//.*$" contains=pitonTodo
syn keyword pitonTodo TODO FIXME NOTE XXX contained

hi def link pitonComment      Comment
hi def link pitonTodo         Todo
hi def link pitonImport       PreProc
hi def link pitonImportWord   PreProc
hi def link pitonPath         String
hi def link pitonStar         Special
hi def link pitonStorage      Keyword
hi def link pitonModifier     StorageClass
hi def link pitonAs           Keyword
hi def link pitonAlias        Function
hi def link pitonExtends      Keyword
hi def link pitonAnchorName   Type
hi def link pitonKeywordName  Structure
hi def link pitonProperty     Identifier
hi def link pitonConstraint   Typedef
hi def link pitonType         Type
hi def link pitonPass         Keyword
hi def link pitonListMarker   Special
hi def link pitonMergeMarker  Operator
hi def link pitonSigil        Special
hi def link pitonConstant     Constant
hi def link pitonSelf         Identifier
hi def link pitonNumber       Number
hi def link pitonString       String
hi def link pitonOperator     Operator
hi def link pitonAnchorRef    Type
hi def link pitonFence        String
hi def link pitonFenceDelim   Special
hi def link pitonEscape       String
hi def link pitonEscapeDelim  Special

let b:current_syntax = "piton"
