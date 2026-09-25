# Conditional Operators

## Description

Picks between two values based on a boolean.
Ternary is just a fun word to say, kind of like "Guido" is fun to say. But my name isn't Guido, so I have to include the ternary operators in the language.

## Operators

- description: The ternary operator selects between two expressions based on a condition, using the syntax `condition ? consequent : alternative`. If the condition is true, the consequent is evaluated and returned; otherwise, the alternative is evaluated and returned. Only the selected expression is evaluated.
    The condition has to be a boolean. There's no truthiness in Piton, so anything else is a compiler error.
    Ternary operators can be chained; a ternary can be in the consequent or alternative slots.
  symbol: `<condition> ? <consequent> : <alternative>`
