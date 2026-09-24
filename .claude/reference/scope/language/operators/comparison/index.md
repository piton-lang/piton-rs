# Comparison Operators

## Description

Compares two values and gives you a boolean.

## Equality

`==` and `!=` work on every type. Lists and dictionaries are equal if everything inside them is equal. Anchors are only equal if they're the same anchor, and references are only equal if they point at the same thing.

## Ordering

`<`, `<=`, `>`, and `>=` work on numbers and strings. Strings are compared by Unicode code point. Anything else, or a number against a string, is a compiler error.

## Operators

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
