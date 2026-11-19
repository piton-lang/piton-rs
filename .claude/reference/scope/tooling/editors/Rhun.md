# Rhun

## Description

Editing plugin for Rhun. Has full support for the LSP.

## Editor Behavior

Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules

### Enter On Colon

Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.

### Enter On Blank Line

On the first blank line inside a dictionary or anchor, enter should insert a new line at the same indent level, so a paragraph break stays in the block. On a second blank line in sequence, enter should insert a new line and dedent it by 1 level. Thus the first enter after text opens a new line, the second keeps the level, and only the third dedents.

### Auto Format On Save

Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.

### Strings Stay Strings

Prose is one string, and the syntax highlighter shows it in the editor's plain text style, so the structure around it stands out. A word in it is never a keyword, constant, number, or type, and characters like `$`, `#`, `@`, a backtick, or brackets that open nothing are part of the string. Imports and declarations are top-level only, so they're recognized at column 0: an indented line like `use the spec to decide` or `export the result` is prose. Only the expressions inside `{...}` forms are highlighted within a value, and a quoted string inside one is coloured as a string.

### Values Keep Their Type

A value that is exactly a number, `true`, `false` or `null` is that value, and is highlighted as one. With any other text beside it, it is prose.

### Fences Are Text

A code fence is text. The compiler reads what is inside it like any other Piton, so it is highlighted like any other Piton. An escape block is literal and shown as verbatim, and when it sits directly inside a fence that names a language, an editor that can may highlight it as that language.

### One Classification

Every syntax definition is tested against what the parser makes of the same files. The parser's classification of each token is the expected answer, and a definition may differ from it only where a known, listed gap says why.

## Syntax Highlighter

- rhun-grammar
- lsp

## Url

https://github.com/vshvedov/rhun/

## Highlighting

A rhun grammar (a `.syn` file) gives the syntax highlighting. It claims `.pi` files, gives `//` to Toggle Comment and indents the line after a colon. A rhun grammar has no context, so it colours a word by its spelling alone. Thus the grammar lists no keywords, and it does not give the keyword colour to a word in a string body: `from` in prose is plain. The language server gives the keyword colour to the keywords at column 0 and in a declaration header, where the parser reads them as keywords, through its semantic tokens. It also gives the colours that need the resolved program. Every line where the grammar and the parser disagree is a known, listed gap, as `oneClassification` asks.

## Language Server

rhun talks to its extensions in its own protocol, not in LSP. The Piton extension for rhun is the first-party rhun-editor/lsp bridge, changed to serve Piton files only and to start `piton lsp`. It gives rhun the full LSP support of `piton lsp`: problems, completion, hover, navigation, rename, formatting, folding, semantic colours and inlay hints.
The extension shows what the server shows, so it shows no JSON hints when the build writes no JSON. A Belay project that sets no `output` or `renderer` writes no JSON. There, no hover, code lens or inlay hint names a JSON file or shows a value as JSON. They name the files of the Belay targets.
