# Format

## Description

Applies canonical formatting: four spaces for each indent level, a space after `//`, the mandatory spaces around `::` and `:`, sorted import lines without the `.pi` extension, and lines wrapped at 80 columns.
Without a path, it formats every source file beneath the source root, however many there are. Build output, dependency directories and installed packages are left alone.

## Command Name

format

## Positional Arguments

```
path: file or glob
```

## Named Arguments

```
check: Only check the files, and report problems.  Don't write.
```

## Imports

A blank line separates the imports from what follows them. A comment directly under the last import goes below the blank line, with the declaration it describes. A comment between two imports stays where it is.
In a project, unused imports are removed: a name that the file never references comes out of its import list, a list left empty takes its line with it, and a `use` whose keywords the file never uses goes, as does a repeated `use`. It is the same set the language server reports as unused and its organize imports action removes. Star exports and exports stay, since they are what a module publishes.
A file with an error keeps every import. An import that does not resolve looks unused, but a package not yet tethered or a mistyped path can be the cause, and removing it would lose what the author meant. piton.config.pi keeps its imports too. Without a project, and for `piton format -` and the editor's format on save, imports are sorted but none are removed: one file alone cannot tell what is used, and a save must not take away an import the author is about to use.

## Wrapping

Imports and exports break across lines when they have more than two items or pass 80 columns, and join again when they get short.
Prose is reflowed. The lines of each paragraph are joined and filled again, as many words on a line as fit in 80 columns, indentation included. Piton joins the lines of a paragraph with one space, so a reflow never changes a value. A blank line stays, and so does each paragraph break it makes. A comment line also ends a paragraph, so the formatter never moves text across it.
A line breaks only at a single space. A space inside an expression in braces, inside an inline escape, or in a run of two or more spaces never becomes a break, because each of those changes the value. A paragraph whose line ends with a backslash is left as it is: a space after that backslash would open an escape.
A new line never starts like structure: with `- `, `+ ` or `++ `, with `key:` or `key::`, with `//`, as `pass`, or as a run of backslashes. Such a word moves down with the word before it, and the lines after it fill again. When no break avoids it, a line runs past 80 columns. A paragraph that cannot be made safe stays as written.
A `key: value` line that passes 80 columns moves its value into an indented block below the key, wrapped there. Text after the colon with prose directly beneath it moves down too and joins that prose, since the two already make one paragraph. The text stays on the key line when a blank line separates it from the prose below, when the block has `+` or `++` lines, or when it is an inline list.
A long list item keeps its first words after the dash. The rest wrap onto lines indented one level deeper than the dash. Prose indented under an item is more of that item, so it joins the item's text and is filled with it. A nested list or dictionary under an item stays as it is.
Escape blocks, code inside them, comments and files with parse errors are left alone. Formatting twice gives the same file as formatting once.
