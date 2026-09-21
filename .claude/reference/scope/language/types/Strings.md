# Strings

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

Strings are not quoted. They can appear on the same line as what they’re assigned to, or indented on the next line:
```piton
sameLine: Hello, World!
nextLine:
    Hello, World!
```
The leading whitespace on a string block is discarded, as that’s part of the syntax of the language, not the string. In other words, the string in both examples is “Hello, World!”.
Within a string block, you can add line breaks without affecting the structure of the string. To create an explicit line break, you must include a blank line.
```
myString:
    This broken string is not considered
    a line break.

    While this *is* on a new line because there was a blank line above.
```
When compiled, we’ll get two lines:
```markdown
This broken string is not considered a line break.
While this *is* on a new line because there was a blank line above.
```

### Escaping

[Escaping](Escaping.md)

### Code Blocks

```
TODO: Write stuff about this
```

Links in this document point at reference files. Read one when the work touches what it describes.
