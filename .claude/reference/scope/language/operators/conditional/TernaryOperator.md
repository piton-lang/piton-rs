# Ternary Operator

## Description

The ternary operator selects between two expressions based on a condition, using the syntax `condition ? consequent : alternative`. If the condition is true, the consequent is evaluated and returned; otherwise, the alternative is evaluated and returned. Only the selected expression is evaluated.
The condition must evaluate to a boolean. Piton has no truthiness, so a condition of any other type is a compiler error.
Ternary operators can be chained; a ternary can be in the consequent or alternative slots.

## Symbol

`<condition> ? <consequent> : <alternative>`
