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
| `A + B`                 | `string` (no braces, so it is the text "A + B")                    |
| `{A + B}`               | The result type of `+` for the operand types; see ConcatenationOperators |
| `This costs $5 + tax`   | `string`                                                           |
| `Total: {a + b}`        | `string` (braces with surrounding text interpolate)                |
| `null`                  | `null`                                                             |
| `"false"`               | `string` (the quotes are part of the value)                        |
| `\ // \ Just Text`      | `string` (the escaped `//` is not a comment)                       |
| `${}`                   | `string`                                                           |
```
