# Diagnostics

## Description

Reporting each problem once, where it was made, and clearing it once it is fixed

## Source

Diagnostics are the compiler's, from the same compilation `piton build` runs, with one addition: unused imports, which the server finds with the model every other feature uses, as [SymbolIdentity](../model/SymbolIdentity.md) describes.

## Publishing

- A file's diagnostics are published by the project that owns it, once, as [WorkspaceProjects](../model/WorkspaceProjects.md) describes.
- A file whose last problem was fixed receives an empty publish, so the editor clears it.
- A file's diagnostics are sent only when they differ from what was last sent for it.
- Diagnostics are published once the editor has stopped changing documents, not on every keystroke.
- A pull request for a document's diagnostics returns what is published for it.
- Errors are errors and warnings are warnings, and each carries the compiler's code.

## One Problem Once

A problem is reported once, where it was caused. An expression whose operand has already failed reports nothing more about its result, so a name that cannot be found is reported, but the member access on it, the operator applied to it, and the constraint checked against it are not reported again.

## Unused Imports

- An import item whose binding nothing in the file uses is reported as unused, and so is a `use` line none of whose keywords the file uses.
- A binding counts as used when anything in the file resolves to it, interpolations in prose and `export` lines included.
- Unused imports are hints marked as unnecessary, so editors fade them rather than underline them, and carry the code `unused-import`.
- A file with syntax errors reports no unused imports, because what it uses cannot be known.
- A re-export is never unused, because it exists for other files.

Links in this document point at reference files. Read one when the work touches what it describes.
