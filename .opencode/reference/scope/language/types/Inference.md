# Inference

## Description

Unless specifically constrained to a type, a variable or property can hold any type. The type is inferred from the value.
```markdown
| Expression            | Inferred As                                                                        |
| --------------------- | ---------------------------------------------------------------------------------- |
| `true`                | `boolean`                                                                          |
| `true story`          | `string`                                                                           |
| `42`                  | `number`                                                                           |
| `42 things`           | `string`                                                                           |
| `A + B`               | If A and B are the same type, inferred is also that type. Otherwise it's a string. |
| `This costs $5 + tax` | `string`                                                                           |
| `null`                | `null`                                                                             |
| `\// Just Text`       | `string`                                                                           |
| `"// Also Just Text"` | `string`                                                                           |
| `${}`                 | `string`                                                                           |
```
