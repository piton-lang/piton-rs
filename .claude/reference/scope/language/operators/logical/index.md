# Logical Operators

## Description

Combine or negate booleans. Anything that isn't a boolean is a compiler error.
Nerds.

## Evaluation

There's no short-circuiting. Both sides of `&&` and `||` are always evaluated, and both have to be booleans, even when the left side already decides the answer. So `{true || 5}` is a compiler error, and so is `{false && 1 / 0 == 1}`, because of the division by zero. With no side effects in Piton, skipping the right side could only ever hide an error, and a typo shouldn't compile just because it's in a branch that doesn't matter.
The ternary is the one exception: it only evaluates the branch it picks.

## Operators

- description: Logical AND operator
  symbol: &&
- description: Logical OR operator
  symbol: ||
- description: Logical negation operator
  symbol: !
