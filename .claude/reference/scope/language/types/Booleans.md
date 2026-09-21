# Booleans

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: The concatenation operator (`+`) joins two strings into a single string, preserving their order. For example, `"Hello" + " world"` evaluates to `"Hello world"`.
Adheres to rules of [TypeCoercion](../variables/TypeCoercion.md)
  symbol: +

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md) and [Inference](Inference.md).

## Description

Booleans are represented with lowercase `true` and `false`.

Links in this document point at reference files. Read one when the work touches what it describes.
