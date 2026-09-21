# Operators

## Control Flow

### Control Flow

There are no conditionals or loops in Piton with the exception of the [TernaryOperator](conditional/TernaryOperator.md)
There are also no functions.

## Standard Operators

Piton supports a pretty standard if not small set of operators, plus a few slightly more unique ones. Let’s start with the standard ones.

### Arithmetic Operators

[ArithmeticOperators](arithmetic/ArithmeticOperators.md)

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
- description: : Divides two numbers
  symbol: /
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Returns the remainder of two numbers
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
- description: The concatenation operator (`+`) joins two strings into a single string, preserving their order. For example, `"Hello" + " world"` evaluates to `"Hello world"`.
Adheres to rules of [TypeCoercion](../variables/TypeCoercion.md)
  symbol: +
- description: The merge operator (`+`) combines two values into a single value, preserving their distinct elements and removing duplicates.
  symbol: +
- description: null
  symbol: ++
- description: The ternary operator selects between two expressions based on a condition, using the syntax `condition ? consequent : alternative`. If the condition is truthy, the consequent is evaluated and returned; otherwise, the alternative is evaluated and returned. Only the selected expression is evaluated.
Ternary operators can be chained; a ternary can be in the consequent or alternative slots.
  symbol: `<condition> ? <consequent> : <alternative>`
- description: Logical AND operator
  symbol: &&
- description: Logical OR operator
  symbol: ||
- description: Logical negation operator
  symbol: !

Links in this document point at reference files. Read one when the work touches what it describes.
