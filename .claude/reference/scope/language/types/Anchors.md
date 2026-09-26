# Anchor Type

## What Is A Type

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

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md#type-coercion) and [Inference](./Inference.md#inference).

## Description

There is exactly one user-defined type, and it’s called an anchor. This is something that can be shaped by other types via properties and promotes inheritance. However, it’s a larger and more advanced topic than belongs in this introductory part of the guide, so instead we’ve devoted an entire section to it later on.

## Valid Keys

Keys can have Unicode letters, Unicode numbers, underscores, and hyphens. Nothing else, and no spaces.
Keys are always strings, even when they look like something else. So `123`, `false`, and `null` are fine as keys, and they're still just strings when you access them.
So for example, `thisIsAKey` and `123` and `foo-bar` and `false` and `null` are all valid keys, while `This is a key` and `a.b` are not.
