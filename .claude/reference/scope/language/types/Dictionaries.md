# Dictionaries

## What Is A Type

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: Accesses a property of an object.
  symbol: .
- description: The merge operator (`+`) combines two lists or two dictionaries.
    On lists it joins them in order and removes duplicates. If a value shows up more than once, the last one is kept. So `[A, B, C, D] + [A, B, C]` gives you `[D, A, B, C]`.
    On dictionaries it's a shallow merge. You get the keys from both, and if both have the same key, the right side wins.
  symbol: +
- description: The `++` operator is like merge, but it keeps duplicates.
    On lists it joins them in order and keeps everything, so `[A, B, C, D] ++ [A, B, C]` gives you `[A, B, C, D, A, B, C]`.
    On dictionaries it's a deep merge. If both sides have the same key and both values are dictionaries, those get merged too, all the way down. Otherwise the right side wins.
    On strings it joins them with a line break.
  symbol: ++
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md#type-coercion) and [Inference](./Inference.md#inference).

## Description

Dictionaries are nested keys and values, and there is only a single way to define them:
```piton
firstLevel:
    secondLevel:
        thirdLevel: This is a string
```
You can use dot syntax to access keys in a dictionary. firstLevel.secondLevel.thirdLevel would yield “This is a string”.

## Lists In Dictionaries

A list indented under a key is that key's value. If the key also has something above the list, like text or another dictionary, the value becomes an implicit list (see Collections) and the list goes in as a single item:
```piton
plain:
    - x
    - y
mixed:
    text
    - x
    - y
```
```json
{
  "plain": ["x", "y"],
  "mixed": ["text", ["x", "y"]]
}
```
For dictionaries indented under a list item, see the Lists type.

## Valid Keys

Keys can have Unicode letters, Unicode numbers, underscores, and hyphens. Nothing else, and no spaces.
Keys are always strings, even when they look like something else. So `123`, `false`, and `null` are fine as keys, and they're still just strings when you access them.
So for example, `thisIsAKey` and `123` and `foo-bar` and `false` and `null` are all valid keys, while `This is a key` and `a.b` are not.

## Hyphenated Keys

A hyphen in the middle of a name is part of the name, so `{config.foo-bar}` reads the key `foo-bar`. If you want subtraction, put spaces around it: `{a - b}`.
