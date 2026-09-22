# Type Coercion

## Description

Type coercion in Piton is intentionally limited. When a value is constrained to multiple types, Piton evaluates the constraints from left to right and uses the first type the source value can validly represent.
A number literal may be coerced to a string when a string constraint comes first. For example:
```piton
value:: string:: number: 42
```
This evaluates to the string “42” because string is the first compatible constraint.
Booleans and null are never coerced. Under a string constraint the literals true, false, and null are a compiler error rather than text:
```piton fragment
flag:: string: false
```
To get the text, stringify explicitly with `${false}`.
Quote characters have no special meaning in a value, so a quoted value is simply a string that includes its quotes. `value:: boolean:: string: "false"` evaluates to the seven-character string `"false"`, because `"false"` is not a boolean literal.
Likewise, values that are already structurally typed, such as lists, dictionaries, and anchors, are not coerced into unrelated types.
If none of the declared constraints can accept the value it is a compiler error.
