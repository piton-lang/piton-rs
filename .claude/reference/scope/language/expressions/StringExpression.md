# String Expression

## Description

Evaluates an expression and converts its result to a string.

## Syntax

${}

## Evaluation

- Evaluate the enclosed expression using normal expression rules.
- Return string results unchanged.
- Convert other results according to the stringification rules for their type.
- Report an error when no string representation is defined.

## Conversion

- Convert numbers to their textual representation.
- Convert booleans to lowercase true or false.
- Convert null to the string null.
- Convert anchors to their name, as written in the source.
- Convert a named list or dictionary to its name, like Anchor.propertyName, or just the variable name at the top of a file.
- Report an error for a list or dictionary that has no name.
- Do not reinterpret the resulting string as source syntax or another expression.

## Composition

```
description: The result is a string that can be embedded in surrounding text or participate in an enclosing expression.
```

## Output

```
description: Preserve the string type in structured output. Apply any escaping required by the output format during serialization.
```
