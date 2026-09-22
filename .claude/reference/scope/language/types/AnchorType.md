# Anchor Type

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: Accesses a property of an object.
  symbol: .
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md) and [Inference](Inference.md).

## Description

There is exactly one user-defined type, and it’s called an anchor. This is something that can be shaped by other types via properties and promotes inheritance. However, it’s a larger and more advanced topic than belongs in this introductory part of the guide, so instead we’ve devoted an entire section to it later on.

## Valid Keys

A key may contain Unicode letters, Unicode digits, underscores, and hyphens. It may not contain spaces or any other character.
Every key is a string. Keys that look like other literals or reserved words, such as `123`, `false`, `null`, or `type`, are allowed and are always treated as strings, including in property access.
So `thisIsAKey`, `123`, `foo-bar`, `false`, and `null` are valid keys, while `This is a key`, `a.b`, and `x:y` are not.

Links in this document point at reference files. Read one when the work touches what it describes.
