# Rename

## Description

Renaming a symbol everywhere it is used, or refusing with a reason

## Summary

Rename changes every declaration and every reference of the symbol under the cursor, as [SymbolIdentity](../model/SymbolIdentity.md) identifies it, in every project that can see it, as [WorkspaceProjects](../model/WorkspaceProjects.md) describes. A rename that would change what the project means, or stop it compiling, is refused before any edit is made, with a message that says why.

## Prepare

Preparing a rename answers with the range of the name under the cursor and its current spelling. It refuses, with a message, when the cursor is on any of these:

- nothing that resolves to a symbol
- `self`, `this`, or `super`
- a built-in type
- a module specifier, because a file is renamed by moving it and its imports follow the move
- a symbol declared by a builtin or framework module, or a property whose family includes such a declaration

## Edits

- The declaration, every reference, and every import and re-export item that names the symbol without an alias are renamed.
- Renaming a symbol that a file imports under an alias renames the item's first name there, and leaves the alias and its uses alone.
- Renaming an alias renames the alias and its uses only.
- Renaming a keyword renames its `as` clause and every declaration written with it.
- Renaming a property renames every declaration in its family and every member access into it, including through `self`, `this`, and `super`.
- Renaming a nested key renames it in every dictionary its owner declares, and every member access that reaches it.
- Every edit is made against the text the editor holds, whether the file is open or not.

## Validation

- An anchor, variable, alias, property, or key takes a name that starts with a letter or `_` and continues with letters, digits, `_`, or a `-` between two of those, and that is not a reserved word.
- An anchor, or an alias of one, cannot take the name of a built-in type either.
- A keyword takes a lowercase name, which may be kebab-case, and that is neither a reserved word nor a built-in type.
- Renaming to the current name changes nothing and is not an error.

## Conflicts

A rename is refused when the new name is already taken where the renamed symbol would be visible, and the message names the file and what takes the name:

- for an anchor, variable, or alias, in the scope of any file whose binding would change, and among the exports of any module that re-exports it
- for a keyword, among the keywords of any file that can see it
- for a property, among the properties visible on any anchor in its family
- for a nested key, among the keys of any dictionary that declares it

Links in this document point at reference files. Read one when the work touches what it describes.
