# Find References

## Description

Listing every place a symbol is used, across every project that can see it

## Summary

The references of a symbol are every place that resolves to it through [ReferenceResolution](../model/ReferenceResolution.md), in every file of every project that analyses a file declaring it. When the request asks for declarations, every declaration is included as well. Each location is listed once.

## By Symbol

- An anchor or a variable is referenced by names in expressions and interpolations, by `extends`, by constraints, by `export`, and by every import and re-export item that reaches it without an alias.
- An alias is referenced where its own spelling is used, in the file that declares it and in files that import it from a module that re-exports it.
- A keyword is referenced by every declaration written with it.
- A property is referenced by every member access that resolves into its family, and declared by every key with its name in the family, as [SymbolIdentity](../model/SymbolIdentity.md) describes.
- A module is referenced by every `from` and `use` specifier that loads it, which is how the references to a file are found.

## Never

`self`, `this`, and `super` are never references, and nothing that failed to resolve is ever listed as one.

Links in this document point at reference files. Read one when the work touches what it describes.
