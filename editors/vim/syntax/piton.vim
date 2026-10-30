" Vim syntax file for Piton.
"
" Piton is whitespace-structured and most of its content is prose, so this
" highlights the structure and leaves the prose in the plain text style: words,
" `$5`, `#3`, `@piton/x`, backticks, brackets and `**bold**` are all just text.
" Anything deeper -- what a name resolves to, which base supplied a value --
" needs the language server.
"
" Every group here is a name in editors/fixtures/scopes.toml, and
" crates/piton-syntax/tests/highlight_corpus.rs runs this file in Vim over the
" fixtures and compares each byte's group with what the parser says the byte
" is. Vim's `contains=` names groups, not items, so one group can hold several
" patterns: each pattern that is not `contained` carries its own context as a
" lookbehind, so that it matches only where it should whichever region it is
" tried in.

if exists("b:current_syntax")
  finish
endif

" Names. A hyphen belongs to a name, as it does in a key and in an expression
" (`{config.foo-bar}`): subtraction needs spaces.
let s:name = '[A-Za-z_][A-Za-z0-9_-]*'
" A key: letters, digits, `_` and `-`, Unicode included.
let s:key = '\%([^[:space:][:punct:]]\|_\)\%([^[:space:][:punct:]]\|[_-]\)*'
" A declaration's head: `[export] [abstract] <keyword>`, where the keyword is
" `anchor` or a user keyword, lowercase and optionally kebab-case.
let s:head = '^\%(export\s\+\)\?\%(abstract\s\+\)\?[a-z][a-z0-9-]*'
" What may follow a declaration's name: `extends A, B`, `as kw`, the colon.
let s:tail = '\%(\s\+\%(extends\|as\)\s[^:]*\)\?:\s*$'
" The compiler's whole-value number: a decimal needs its leading digit, and `_`
" separates digits.
let s:number = '-\?\d\%(_\?\d\)*\%(\.\d\%(_\?\d\)*\)\?'

" Declarations are top-level only, so each pattern starts at the margin and
" the whole line has to have a declaration's shape: `anchor: a description`
" is a key, and an indented `use the spec` is prose.
exe 'syn match pitonKeyword "' . s:head . '\ze\s\+' . s:name . s:tail . '"'
exe 'syn match pitonTypeDecl "\%(' . s:head . '\s\+\)\@<=' . s:name . '\ze' . s:tail . '"'
exe 'syn match pitonKeyword "\%(' . s:head . '\s\+' . s:name . '\s\+\%([^:]*\s\)\?\)\@<=\%(extends\|as\)\ze\s[^:]*:\s*$"'
exe 'syn match pitonKeyword "\%(' . s:head . '\s\+' . s:name . '\s.*\<as\s\+\)\@<=[a-z][a-z0-9-]*\ze\s*:\s*$"'
" Bases are types whatever their capitalisation.
exe 'syn match pitonTypeRef "\%(' . s:head . '\s\+' . s:name . '\s\+extends\s\+\%(' . s:name . '\s*,\s*\)*\)\@<=' . s:name . '"'

" `use` brings keywords into scope and nothing else.
syn match pitonKeyword "^use\ze\s"
syn match pitonPath "\%(^use\s\+\)\@<=\S\+"

" `from <path> import|export <names>`. A long list continues on indented
" lines, so the region runs until a line at the margin or a blank line. A name
" in it is an anchor only by convention: a capitalised name is a type.
syn region pitonFrom matchgroup=pitonKeyword start="^from\>" end="^\ze\S" end="^\s*$" transparent contains=pitonPath,pitonKeyword,pitonTypeRef,pitonVariable
syn match pitonTypeRef "\<[A-Z][A-Za-z0-9_-]*" contained
syn match pitonVariable "\<[a-z_][A-Za-z0-9_-]*" contained
syn match pitonPath "\%(^from\s\+\)\@<=\S\+" contained
syn keyword pitonKeyword import export contained

" `export Name` re-exports something declared or imported.
exe 'syn match pitonKeyword "^export\ze\s\+' . s:name . '\s*$"'
syn match pitonTypeRef "\%(^export\s\+\)\@<=[A-Z][A-Za-z0-9_-]*\ze\s*$"
syn match pitonVariable "\%(^export\s\+\)\@<=[a-z_][A-Za-z0-9_-]*\ze\s*$"

" A key is a word followed by a colon and then a space, the end of the line,
" or a constraint's second colon. Keywords are keys here too: the colon is
" what separates `anchor Name:` from `anchor: a description`.
exe 'syn match pitonKeyword "^export\ze\s\+' . s:key . ':\%(:\|\s\|$\)"'
" The key is found by lookbehind rather than `\zs`, because a top-level
" `export` has already been matched as a keyword by the time Vim reaches it.
exe 'syn match pitonKey "\%(^\s*\%(export\s\+\)\?\)\@<=' . s:key . '\ze:\%(:\|\s\|$\)"'

" Constraints: `name:: type`, `name:: extends Type[]`, chained with `::`. A
" built-in type name is only a type here; anywhere else it is a word.
exe 'syn match pitonTypeRef "\%(::\s*\%(extends\s\+\)\?\)\@<=' . s:name . '"'
syn match pitonBuiltinType "\%(::\s*\%(extends\s\+\)\?\)\@<=\%(string\|number\|boolean\|null\|list\|dictionary\|anchor\|reference\|any\|simple\|complex\)\>"
" Last, so that `extends` is not taken for the type it introduces.
syn match pitonKeyword "\%(::\s*\)\@<=extends\>"

" A value that is only a literal is that literal: `42`, `-1_000.50`, `true`.
" With anything else on the line it is text, so `3 apples` and `true story`
" stay plain, and so does `+ 3`, because a merge line's value is never one.
" The colon is a value's, not the second of a constraint's `::`, where `null`
" is a type.
exe 'syn match pitonNumber "\%(:\@<!:\s\+\|^\s*-\s\+\)\@<=' . s:number . '\ze\s*$"'
syn match pitonConstant "\%(:\@<!:\s\+\|^\s*-\s\+\)\@<=\%(true\|false\|null\)\ze\s*$"

" `pass` is an intentionally empty body.
syn match pitonKeyword "^\s*\zspass\ze\s*$"

" Structure markers. A list item is never a key: `- Settings:` is a marker and
" the text `Settings:`.
syn match pitonListMarker "^\s*\zs-\ze\%(\s\|$\)"
syn match pitonOperator "^\s*\zs++\?\ze\%(\s\|$\)"

" Interpolation. Four sigils: intrinsic, string, numeric, and reference. Inside
" the braces a bare name is a variable, or an anchor if it is capitalised, and
" a name after `.` is a key whatever its case.
syn region pitonInterp matchgroup=pitonSigil start="\%(\$\|#\|@\)\?{" end="}" oneline contains=pitonInterp,pitonConstant,pitonSelf,pitonNumber,pitonString,pitonOperator,pitonTypeRef,pitonVariable,pitonKey
syn keyword pitonConstant true false null contained
syn keyword pitonSelf this self super contained
syn match pitonNumber "\<\d\%(_\?\d\)*\%(\.\d\%(_\?\d\)*\)\?\>" contained
syn region pitonString start=+"+ skip=+\\.+ end=+"+ contained oneline
syn match pitonOperator "++\|==\|!=\|<=\|>=\|&&\|||\|[-+*/%<>!?:.]" contained
syn match pitonKey "\.\@<=[A-Za-z_][A-Za-z0-9_-]*" contained

" `\ {1 + 2} \` is the text `{1 + 2}`: the backslashes are the escape and what
" they hold is plain, so nothing inside opens an interpolation. A lone
" backslash (`\:`) is literal, so the delimiter is a single backslash with a
" space on its inner side.
syn region pitonInlineEscape matchgroup=pitonEscape start="\%(\\\)\@<!\\\%(\\\)\@! " end=" \\\%(\\\)\@!" end="$" oneline

" A line of backslashes opens a block whose contents are literal, closed by a
" line of the same run. A code fence is not one: the lines inside a fence are
" Piton like any others, so a fence gets no group at all.
syn region pitonVerbatim matchgroup=pitonEscape start="^\s*\z(\\\+\)\s*$" end="^\s*\z1\s*$" keepend

" A comment has to be on its own line. At the end of a line of code `//` is
" just text, so `url: https://example.com // note` is all value. Defined last
" so that it wins over anything else starting at the same column. Escape blocks
" are regions that contain no comment, so a `//` line inside one stays literal.
syn match pitonComment "^\s*\zs//.*$"

hi def link pitonComment      Comment
hi def link pitonKeyword      Keyword
hi def link pitonTypeDecl     Type
hi def link pitonTypeRef      Type
hi def link pitonVariable     Identifier
hi def link pitonKey          Identifier
hi def link pitonPath         String
hi def link pitonBuiltinType  Type
hi def link pitonNumber       Number
hi def link pitonConstant     Constant
hi def link pitonString       String
hi def link pitonSelf         Special
hi def link pitonOperator     Operator
hi def link pitonListMarker   Special
hi def link pitonSigil        Special
hi def link pitonEscape       SpecialChar
hi def link pitonVerbatim     String

let b:current_syntax = "piton"
