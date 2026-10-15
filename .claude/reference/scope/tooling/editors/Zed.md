# Zed

## Description

Editing plugin for Zed. Has full support for the LSP.

## Editor Behavior

Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules

### Enter On Colon

Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.

### Enter On Blank Line

On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.

### Auto Format On Save

Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.

### Strings Stay Strings

Prose is one string, and the syntax highlighter colors it as one. A word in it is never a keyword, constant, number, or type, and characters like `$`, `#`, `@`, a backtick, or brackets that open nothing are part of the string. Imports and declarations are top-level only, so they're recognized at column 0: an indented line like `use the spec to decide` or `export the result` is prose. Only the expressions inside `{...}` forms are highlighted within a value.

## Syntax Highlighter

- tree-sitter
- lsp
