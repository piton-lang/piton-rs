# Reference Resolution

## Description

How every place a name is written resolves to a symbol

## Summary

Resolution follows the compiler exactly. A place the compiler would resolve to a symbol resolves to that symbol here, and a place the compiler cannot resolve resolves to nothing: no definition, no hover, no references, and no rename. Reporting the problem is the compiler's job, not a navigation feature's. The symbols are the ones [SymbolIdentity](SymbolIdentity.md) describes.

## Names

A bare name inside braces, or inside an interpolation with any sigil, resolves in the scope of the file it is written in, in the order the compiler builds that scope: the file's own anchors and variables first, then its imports, with an imported name replacing a local one spelled the same.
A name bound by an import item resolves to the item's alias when the item has one, and otherwise to whatever the item names. A framework's builtin value, such as `__BELAY_SHAPE__`, wins over any other binding, as it does in the compiler.

## Declarations

- Each name after `extends` resolves as a bare name, and only to an anchor.
- The keyword that introduces a declaration, as `tool` does in `tool Hammer`, resolves to the keyword the file can see under that spelling.
- A named type in a constraint is a built-in type when it names one, as the constraint checker reads it, and otherwise resolves to an anchor in scope, and so does the type inside `T[]`.
- The type inside `extends T` resolves only to an anchor in scope, never to a built-in type.
- The name after `export` resolves as a bare name.
- Nothing inside a fenced code block resolves, an interpolation or an expression in braces written there included, because the compiler keeps a fence's content as text and never reads it.

## Imports

The first name of an import or re-export item resolves to the export it names, followed back to what declared it. A module that re-exports the name, by name or with `*`, is followed through to the module it took the name from. A module that re-exports under an alias is where that spelling is declared, so the chain stops at the alias.
A module specifier resolves to the file the compiler loads for it.

## Members

A member access resolves its left side to a holder, and the member to the holder's property or key of that name.

- `self` and `this` hold the anchor whose body the expression is written in, including inside a nested dictionary of that body.
- `super` holds the same anchor, but reads its members from the bases.
- A name that resolves to an anchor holds that anchor.
- A name that resolves to a variable, and a member that resolves to a property or key, holds that symbol's value.
- A value written as a dictionary holds its keys.
- A value written as a single reference, such as `{Tool}` or `{Tool.head}`, holds whatever that reference holds.
- Anything else, such as an operator, a list, or a framework value, holds nothing that can be followed, and its members resolve to nothing.

On an anchor, a member resolves only when a property of that name is visible on the anchor. Otherwise it resolves to nothing, and the compiler reports it.

## Used Declaration

When one declaration of a property has to be chosen for an anchor, it is the one the compiler reads: the anchor's own key when it writes one, and otherwise the declaration its bases provide, with the bases read left to right, a declaring keyword's anchor first, and the right-most base that provides the property winning. A member read through `super` uses the declaration the bases provide, never the anchor's own. The effective constraint is likewise the right-most constraint the chain provides.

Links in this document point at reference files. Read one when the work touches what it describes.
