# Hover

## Description

Explaining the name under the cursor in the compiler's own terms

## Summary

Hover answers from [ReferenceResolution](../model/ReferenceResolution.md), covers exactly the name under the cursor, and answers nothing where nothing resolves. It never guesses, and it never states something the compiler would not.

## Content

- An anchor shows its declaration line as written, its doc comment, the properties visible on it with their effective constraints and the anchor each is inherited from, and, when it is abstract, the concrete anchors that implement it.
- A property shows its name and effective constraint, the anchor whose declaration is used, the base declaration it overrides when it overrides one, its compiled value when the holder is a concrete anchor, and the doc comment of the declaration used.
- A nested key shows its path from its owner, and its compiled value when the owner is a variable; under a property the value depends on which anchor reads it, so none is shown.
- A variable shows its name, its constraints, its compiled value, and its doc comment.
- An alias says which symbol it names and which module it came from, followed by that symbol's hover.
- A keyword says which anchor it declares things as, followed by that anchor's hover.
- A module shows its path relative to its project and the names it exports.
- A built-in type says what the type accepts.
- `self` says it is the most-derived anchor being compiled and names the anchor it is written in, `this` names that anchor exactly, and `super` names the bases it reads from.

## Values

A compiled value is shown as JSON and cut short after twenty lines. No value is shown for an abstract anchor, because the compiler does not compile one.

Links in this document point at reference files. Read one when the work touches what it describes.
