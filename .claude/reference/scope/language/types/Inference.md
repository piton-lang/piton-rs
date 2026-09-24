# Inference

## Description

Unless specifically constrained to a type, a variable or property can hold any type. The type is inferred from the value.
```markdown
| Value                   | Inferred As                                                        |
| ----------------------- | ------------------------------------------------------------------ |
| `true`                  | `boolean`                                                          |
| `true story`            | `string`                                                           |
| `42`                    | `number`                                                           |
| `-42`                   | `number`                                                           |
| `42 things`             | `string`                                                           |
| `A + B`                 | `string` (no braces, so it's just text)                             |
| `{A + B}`               | Depends on what A and B are; see ConcatenationOperators            |
| `This costs $5 + tax`   | `string`                                                           |
| `Total: {a + b}`        | `string` (there's other text around it)                             |
| `null`                  | `null`                                                             |
| `"false"`               | `string` (the quotes are part of it)                               |
| `\ // \ Just Text`      | `string` (the `//` is escaped, so it's not a comment)               |
| `${a + b}`              | `string` (always, whatever a and b are)                            |
```
