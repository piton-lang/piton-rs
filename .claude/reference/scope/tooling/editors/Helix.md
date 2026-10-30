# Helix

## Description

Editing plugin for Helix. Has full support for the LSP.

## Editor Behavior

Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules

### Enter On Colon

Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.

### Enter On Blank Line

On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.

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

- tree-sitter
- lsp
