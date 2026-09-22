# Strings

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: The concatenation operator (`+`) joins two strings into a single string, preserving their order. For example, `{"Hello" + "World"}` evaluates to `HelloWorld`.
    When exactly one operand is a string, the other operand is first converted using the [StringExpression](../expressions/StringExpression.md) rules, so `{2 + "Hello"}` evaluates to the string `2Hello`. An operand with no string representation is a compiler error.
    Adheres to rules of [TypeCoercion](../variables/TypeCoercion.md)
  symbol: +
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

Quote characters have no special meaning in a value. They are ordinary characters and are kept in the string, so `greeting: "Hello"` holds the seven-character string `"Hello"`, quotes included.
Inside an expression, bare words are symbols, so string literals within braces are written in double quotes: `{"Hello" + name}`. There the quotes delimit the literal and are not part of its value.

## Line Breaks

Within a string block, a single line break joins the two lines with a space. A blank line produces a paragraph break (two newline characters). Several consecutive blank lines collapse into one paragraph break.
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

Piton has a single escape form: wrap the text in backslashes. The opening and closing delimiters are each a run of backslashes followed or preceded by one space, and exactly that one space on each side is removed. Everything between the delimiters is literal.
So \ {1 + 2 + 3} \ would become {1 + 2 + 3}.
There is no single-character escape. To escape one character, wrap it: \ : \ becomes :.

### Stacking

The closing delimiter is the same number of backslashes as the opening one, so the content may contain any shorter run of backslashes. To escape text that itself contains an escape, use a longer delimiter:
\\\ \\ \ {1 + 2 + 3} \ \\ \\\ would become \\ \ {1 + 2 + 3} \ \\.

### Multi Line

A line containing only a run of backslashes opens a multi-line escape block, and the next line containing only the same run closes it. Every line in between is literal, keeping its line breaks and its indentation relative to the delimiter lines.
By convention the specification uses three backslashes for these blocks inside code fences.

## Code Blocks

Markdown code fences are ordinary text to Piton. Their content is parsed like any other string content, so interpolation, comments, list markers, and property syntax inside a fence are still interpreted.
To keep a fence's content literal, wrap it in a multi-line escape block inside the fence, as every example in this specification does.

Links in this document point at reference files. Read one when the work touches what it describes.
