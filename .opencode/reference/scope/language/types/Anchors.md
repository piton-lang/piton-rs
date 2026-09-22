# Anchors

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

null

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md) and [Inference](Inference.md).

## Description

There is exactly one user-defined type, and it’s called an anchor. This is something that can be shaped by other types via properties and promotes inheritance. However, it’s a larger and more advanced topic than belongs in this introductory part of the guide, so instead we’ve devoted an entire section to it later on.

## Valid Keys

Anything that can support [TypeCoercion](../variables/TypeCoercion.md) to a string is a valid key, so long as it doesn't contain spaces.
So for example, `thisIsAKey` and `123` and `foo-bar` and `false` and `null` are all valid keys and will be treated as strings, while `This is a key` and `1 2 3` are not.

Links in this document point at reference files. Read one when the work touches what it describes.
