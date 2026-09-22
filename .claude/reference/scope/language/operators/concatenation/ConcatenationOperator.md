# Concatenation Operator

## Description

The concatenation operator (`+`) joins two strings together. For example, `{"Hello" + "World"}` gives you `HelloWorld`.
If only one side is a string, a number, boolean, or null on the other side gets turned into a string first, following the [StringExpression](../../expressions/StringExpression.md) rules. So `{2 + "Hello"}` gives you `2Hello`, and `{"Enabled: " + true}` gives you `Enabled: true`.
A list, dictionary, or anchor doesn't get turned into a string. You get an implicit list instead, same as putting `{x}` in the middle of some text. So if tags is `[a, b]`, `{"Tags: " + tags}` gives you `["Tags: ", ["a", "b"]]`. If you want the name, use `${tags}`.

## Symbol

+

Links in this document point at reference files. Read one when the work touches what it describes.
