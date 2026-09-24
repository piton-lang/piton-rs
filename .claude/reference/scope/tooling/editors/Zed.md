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

## Syntax Highlighter

- tree-sitter
- lsp
