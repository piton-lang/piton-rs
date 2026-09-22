# Dictionaries

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: Accesses a property of an object.
  symbol: .
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

Dictionaries are nested keys and values, and there is only a single way to define them:
```piton
firstLevel:
    secondLevel:
        thirdLevel: This is a string
```
You can use dot syntax to access keys in a dictionary. firstLevel.secondLevel.thirdLevel would yield “This is a string”.

## Valid Keys

A key may contain Unicode letters, Unicode digits, underscores, and hyphens. It may not contain spaces or any other character.
Every key is a string. Keys that look like other literals or reserved words, such as `123`, `false`, `null`, or `type`, are allowed and are always treated as strings, including in property access.
So `thisIsAKey`, `123`, `foo-bar`, `false`, and `null` are valid keys, while `This is a key`, `a.b`, and `x:y` are not.

## Hyphenated Keys

A hyphen between identifier characters is part of the identifier, so `{config.foo-bar}` reads the key `foo-bar`. Subtraction requires spaces around the operator: `{a - b}`.

Links in this document point at reference files. Read one when the work touches what it describes.
