# Presentation

## Description

Semantic highlighting, folding, and links

## Semantic Tokens

- Tokens are classified from the same syntax tree the compiler reads.
- A name is classified by what it resolves to, an anchor as a class, a keyword as a function, a property or key as a property, and a variable as a variable, and a module specifier is a namespace.
- A declaration carries the declaration modifier, and an abstract anchor carries the abstract modifier as well.
- `self`, `this`, and `super` are read-only keywords, and `true`, `false`, and `null` are read-only enum members.
- The opening and closing lines of a fenced code block are operators and every line inside it is a string, because nothing inside a fence is a name, a key, or a comment.

## Folding

- Every indented block folds from the line that opens it.
- A wrapped import list folds as imports.
- A fenced code block folds from its opening fence to its closing one.

## Links

- Every module specifier links to the file it loads.
