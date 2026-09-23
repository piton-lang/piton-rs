# Lists

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

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

## Dictionaries In Lists

Anything indented under a list item isn't part of that item. It becomes the next item in the list. So a dictionary indented under an item is its own item, right after it:
```piton
repos:
    - https://github.com/piton-lang/piton-rs
        tag: v1
    - https://github.com/piton-lang/other
```
```json
{
  "repos": [
    "https://github.com/piton-lang/piton-rs",
    { "tag": "v1" },
    "https://github.com/piton-lang/other"
  ]
}
```
A list item is never a key, even with a colon at the end. `- Settings:` is just the string `Settings:`.
For lists indented under a dictionary key, see the Dictionaries type.

## Access

Piton intentionally does not provide a way to access items within a list. Because this is not a runtime-based general purpose language but rather a language designed for description, a list is a construct intended for merging via inheritance; myList[0] is not very descriptive, is it?
