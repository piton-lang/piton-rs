# Numeric Expression

## Description

Evaluates an expression and converts its result to a number.

## Syntax

#{expression}

## Evaluation

- Evaluate the enclosed expression using normal expression rules.
- Return numeric results unchanged.
- Convert strings containing a valid numeric representation to a number.
- Report an error when the result cannot be converted to a number.

## Conversion

- Interpret numeric strings according to Piton's numeric literal rules.
- Require the entire string to represent a number.
- Do not interpret a numeric string as an expression.
- Do not implicitly convert booleans, null, collections, or anchors to numbers.

## Composition

```
description: The result is a numeric value that can participate in enclosing expressions, including arithmetic and further conversions.
```

## Output

```
description: Preserve the numeric type in structured output. Convert it to text only when required by the surrounding output format.
```
