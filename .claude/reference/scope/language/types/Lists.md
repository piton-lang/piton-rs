# Lists

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: The merge operator (`+`) combines two lists or two dictionaries.
    On lists it concatenates the operands in order and removes duplicates. When a value appears more than once, the last occurrence is kept and earlier ones are dropped, so `[A, B, C, D] + [A, B, C]` evaluates to `[D, A, B, C]`.
    On dictionaries it performs a shallow merge. Keys from both operands are kept, and when both operands define the same key the right operand's value replaces the left one wholesale.
  symbol: +
- description: The duplicate-preserving merge operator (`++`) combines two lists or two dictionaries.
    On lists it concatenates the operands in order and keeps every element, including duplicates, so `[A, B, C, D] ++ [A, B, C]` evaluates to `[A, B, C, D, A, B, C]`.
    On dictionaries it performs a deep merge. When both operands define the same key and both values are dictionaries, those dictionaries are merged recursively by the same rule. Otherwise the right operand's value wins.
  symbol: ++
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md) and [Inference](Inference.md).

## Description

Lists can be defined two ways, Markdown/YAML-style or inline.
```piton
markdownStyle:
    - One
    - List
    - Item
    - Per
    - Line

inlineStyle: [This, is, a, list, of, strings]
```

## Nested Lists

It’s possible to do nested lists:
```piton
nestedMarkdownList:
    - Level 1
        - Level 2
            - Level 3
```
Which is equivalent to:
```piton
inlineMultiList: [Level 1, [Level 2, [Level 3]]]
```

## Access

Piton intentionally does not provide a way to access items within a list. Because this is not a runtime-based general purpose language but rather a language designed for description, a list is a construct intended for merging via inheritance; myList[0] is not very descriptive, is it?

Links in this document point at reference files. Read one when the work touches what it describes.
