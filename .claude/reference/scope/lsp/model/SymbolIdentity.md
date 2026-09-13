# Symbol Identity

## Description

What counts as one symbol, so that every feature agrees on what a name means

## Summary

Every feature that follows a name first asks which symbol it is, and a symbol is identified by what declared it, never by its spelling. Two names spelled alike are different symbols when different declarations introduce them, and one symbol can be spelled differently in different places through an alias.

## Kinds

- An anchor, declared by `anchor Name`, by `abstract anchor Name`, or by a user-defined keyword.
- A variable, declared at the top level of a file.
- A user-defined keyword, declared by the `as` clause of an anchor.
- An import alias, declared by the second name of an import or re-export item, as `Hammer` is in `import Tool Hammer`.
- A module, which is a file; a directory with an `index.pi` is that file.
- A property, identified by its name together with its property family.
- A nested key, identified by the symbol whose dictionary holds it together with its name.
- A built-in type, which can be hovered but never changed.

## Property Families

A property family is every anchor that shares one property through inheritance. It starts from an anchor on which the property is visible, which means the anchor or one of its ancestors writes a key with that name. Every anchor that extends a member joins the family, because it inherits the property. Every base of a member joins the family when the property is visible on that base. Joining repeats until nothing more joins.
Every key with the property's name written in the body of a member is a declaration of the property, including a key that only states a constraint. Every member access that resolves to a member of the family is a reference to it.
So renaming `description` on a base renames it on every anchor that overrides or implements it, and on any other base that a shared child merges it with, because the compiler treats each of those as the same property.

## Nested Keys

A key written inside a dictionary value is a nested key. The dictionary belongs to the variable, property, or nested key whose value it is, and a nested key's identity is that owner together with its name. Every declaration of the owner that writes a dictionary with a key of that name declares the nested key.

## Self References

`self`, `this`, and `super` are not symbols. They navigate to the anchors they stand for and hover to explain them, but they are never listed as references, never highlighted as one, and never renamed.

## Builtins

A symbol declared by a builtin or framework module, such as the anchors of `@piton/belay` or the properties of `@piton/config`, has no file an editor can change. It can be hovered and its references found, but it cannot be renamed, and neither can a property whose family includes one of its declarations.
