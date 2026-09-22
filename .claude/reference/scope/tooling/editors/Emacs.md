# Emacs

## Description

Editing plugin for Emacs Has full support for the LSP

## Editor Behavior

Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules

### Enter On Color

Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.

### Enter On Blank Line

On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.

## Syntax Highlighter

- tree-sitter
- lsp
