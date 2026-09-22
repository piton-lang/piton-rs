# Strings

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: The concatenation operator (`+`) joins two strings together. For example, `{"Hello" + "World"}` gives you `HelloWorld`.
    If only one side is a string, a number, boolean, or null on the other side gets turned into a string first, following the [StringExpression](../expressions/StringExpression.md) rules. So `{2 + "Hello"}` gives you `2Hello`, and `{"Enabled: " + true}` gives you `Enabled: true`.
    A list, dictionary, or anchor doesn't get turned into a string. You get an implicit list instead, same as putting `{x}` in the middle of some text. So if tags is `[a, b]`, `{"Tags: " + tags}` gives you `["Tags: ", ["a", "b"]]`. If you want the name, use `${tags}`.
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
- description: Less than operator.
  symbol: <
- description: Less than or equal to operator.
  symbol: <=
- description: Greater than operator.
  symbol: >
- description: Greater than or equal to operator.
  symbol: >=

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md) and [Inference](Inference.md).

## Description

Strings are not quoted. They can appear on the same line as what they’re assigned to, or indented on the next line:
```piton
sameLine: Hello, World!
nextLine:
    Hello, World!
```
The leading whitespace on a string block is discarded, as that’s part of the syntax of the language, not the string. In other words, the string in both examples is “Hello, World!”.

## Quotes

Quotes don't mean anything special. They're just characters, so `greeting: "Hello"` is the string `"Hello"`, quotes and all.
The one exception is inside an expression. There, bare words are symbols, so you write strings in double quotes: `{"Hello" + name}`. In that case the quotes aren't part of the string.

## Line Breaks

Within a string block, you can add line breaks without affecting the structure of the string. To start a new paragraph, you must include a blank line. More than one blank line in a row still counts as one.
```piton
myString:
    This broken string is not considered
    a line break.

    While this *is* a new paragraph because there was a blank line above.
```
When compiled, we’ll get two paragraphs:
```markdown
This broken string is not considered a line break.

While this *is* a new paragraph because there was a blank line above.
```

## Escaping

### Description

Escaping works a little differently in Piton than other languages. To escape special characters, you simply wrap them in backslashes, with a space on each side. The spaces are part of the wrapper, so they get removed. So for example \ {1 + 2 + 3} \ would become {1 + 2 + 3}.
That's the only way to escape. Even a single character gets wrapped: \ : \ becomes :.

### Stacking

You can stack backslashes to escape backslashes themselves. The closing wrapper has to match the opening one, so anything inside can use fewer backslashes:
\\\ \\ \ {1 + 2 + 3} \ \\ \\\ would become \\ \ {1 + 2 + 3} \ \\.

### Multi Line

Multi-line escape blocks are valid. Put the backslashes on a line by themselves to open the block, and again to close it. Everything in between is kept as is, line breaks and indentation included.
This spec uses three backslashes for these.

## Code Blocks

Code blocks are not escaped. To Piton they're just text, so anything inside them still gets parsed: expressions, comments, lists, all of it.
So often, you'll want to put an escape block inside the code block. Every example in this spec does that.

Links in this document point at reference files. Read one when the work touches what it describes.
