# Concatenation Operator

## Description

The concatenation operator (`+`) joins two strings into a single string, preserving their order. For example, `{"Hello" + "World"}` evaluates to `HelloWorld`.
When exactly one operand is a string, the other operand is first converted using the [StringExpression](../../expressions/StringExpression.md) rules, so `{2 + "Hello"}` evaluates to the string `2Hello`. An operand with no string representation is a compiler error.
Adheres to rules of [TypeCoercion](../../variables/TypeCoercion.md)

## Symbol

+

Links in this document point at reference files. Read one when the work touches what it describes.
