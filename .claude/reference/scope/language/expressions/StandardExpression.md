# Standard Expression

## Description

Evaluates an expression and returns its intrinsic result, preserving its type without requesting conversion.

## Syntax

{}

## Evaluation

- Evaluate the enclosed expression using normal expression rules.
- Resolve symbols to their values within the applicable scope.
- Return the resulting value without additional conversion.
- Report an error when the expression is invalid or cannot be resolved.

## Composition

```
description: The result retains its type when used within an enclosing expression or assigned as a property value.
```

## Output

### Description

Serialize the resulting value according to its type and the selected output format.

### Requirements

- Preserve native value types in structured output.
- Serialize an anchor as its resolved content rather than a link to it.
- Apply the output format's serialization rules when textual output is required.

## In Text

If there's other text around it, a simple value gets dropped into the text, and a list, dictionary, or anchor turns the whole thing into an implicit list.
