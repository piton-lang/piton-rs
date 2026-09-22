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
- description: : Divides two numbers
  symbol: /
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Returns the remainder of two numbers
  symbol: %
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.

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

Links in this document point at reference files. Read one when the work touches what it describes.
