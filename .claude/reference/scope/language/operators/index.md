# Operators

## Control Flow

[ControlFlow](./ControlFlow.md#control-flow)

## Standard Operators

Piton supports a pretty standard if not small set of operators, plus a few slightly more unique ones. Let’s start with the standard ones.

## Access Operators

[AccessOperators](./access/index.md#access-operators)

## Arithmetic Operators

[ArithmeticOperators](./arithmetic/index.md#arithmetic-operators)

## Comparison Operators

[ComparisonOperators](./comparison/index.md#comparison-operators)

## Concatenation Operators

[ConcatenationOperators](./concatenation/index.md#concatenation-operators)

## Conditional Operators

[ConditionalOperators](./conditional/index.md#conditional-operators)

## Logical Operators

[LogicalOperators](./logical/index.md#logical-operators)

## Precedence

Higher rows go first. Operators on the same row go left to right, except the ternary, which goes right to left. Parentheses change the order.
```markdown
| Precedence | Operators          |
| ---------- | ------------------ |
| 1 highest  | `.`                |
| 2          | `!`                |
| 3          | `*` `/` `%`        |
| 4          | `+` `-` `++`       |
| 5          | `<` `<=` `>` `>=`  |
| 6          | `==` `!=`          |
| 7          | `&&`               |
| 8          | `||`               |
| 9 lowest   | `? :`              |
```
So `{1 + 2 == 3 && !false}` is `{((1 + 2) == 3) && (!false)}`, and `{a ? b : c ? d : e}` is `{a ? b : (c ? d : e)}`.

## All Operators

- description: Accesses a property of an object.
  symbol: .
- description: Adds two numbers together
  symbol: +
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Subtracts two numbers
  symbol: -
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Multiplies two numbers together
  symbol: *
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Divides two numbers. Dividing by zero is a compiler error.
  symbol: /
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Returns the remainder of two numbers. The result keeps the sign of the left side, so `-7 % 3` is `-1`. Modulo by zero is a compiler error.
  symbol: %
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
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
- description: The concatenation operator (`+`) joins two strings together. For example, `{"Hello" + "World"}` gives you `HelloWorld`.
    If only one side is a string, a number, boolean, or null on the other side gets turned into a string first, following the [StringExpression](../expressions/StringExpression.md#string-expression) rules. So `{2 + "Hello"}` gives you `2Hello`, and `{"Enabled: " + true}` gives you `Enabled: true`.
    A list, dictionary, or anchor doesn't get turned into a string. You get an implicit list instead, same as putting `{x}` in the middle of some text. So if tags is `[a, b]`, `{"Tags: " + tags}` gives you `["Tags: ", ["a", "b"]]`. If you want the name, use `${tags}`.
  symbol: +
- description: The merge operator (`+`) combines two lists or two dictionaries.
    On lists it joins them in order and removes duplicates. If a value shows up more than once, the last one is kept. So `[A, B, C, D] + [A, B, C]` gives you `[D, A, B, C]`.
    On dictionaries it's a shallow merge. You get the keys from both, and if both have the same key, the right side wins.
  symbol: +
- description: The `++` operator is like merge, but it keeps duplicates.
    On lists it joins them in order and keeps everything, so `[A, B, C, D] ++ [A, B, C]` gives you `[A, B, C, D, A, B, C]`.
    On dictionaries it's a deep merge. If both sides have the same key and both values are dictionaries, those get merged too, all the way down. Otherwise the right side wins.
    On strings it joins them with a line break.
  symbol: ++
- description: The ternary operator selects between two expressions based on a condition, using the syntax `condition ? consequent : alternative`. If the condition is true, the consequent is evaluated and returned; otherwise, the alternative is evaluated and returned. Only the selected expression is evaluated.
    The condition has to be a boolean. There's no truthiness in Piton, so anything else is a compiler error.
    Ternary operators can be chained; a ternary can be in the consequent or alternative slots.
  symbol: `<condition> ? <consequent> : <alternative>`
- description: Logical AND operator
  symbol: &&
- description: Logical OR operator
  symbol: ||
- description: Logical negation operator
  symbol: !
