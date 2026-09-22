# Numbers

## What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

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
- description: Returns the remainder of dividing two numbers. The result takes the sign of the dividend, so `-7 % 3` evaluates to `-1`. A zero divisor is a compiler error.
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

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md) and [Inference](Inference.md).

## Description

Numbers are written as literal numbers. Leading 0 is mandatory for decimals, and you can use underscores to separate large numbers for readability.
```piton
myInt: 123
myFloat: 3.14
mySmallFloat: 0.14
myBigNumber: 1_200_000.00
```
There is a single number type in all of Piton; type-wise there’s no difference between an int, a float, double, etc.

## Representation

Numbers are IEEE 754 64-bit floating point values.
Negative literals are written with a leading minus, such as `-5`. A list item marker is always followed by a space (`- 5`), so `-5` is a number and `- 5` is a list item containing 5.
Exponent notation such as `1e3` is not supported.
When serialized, a number uses its shortest round-trip form, so `1_200_000.00` is written as `1200000` and `0.50` as `0.5`.

Links in this document point at reference files. Read one when the work touches what it describes.
