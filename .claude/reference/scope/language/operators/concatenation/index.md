# Concatenation Operators

## Description

Operators that join strings, lists, and dictionaries.

## Operators

- description: The concatenation operator (`+`) joins two strings together. For example, `{"Hello" + "World"}` gives you `HelloWorld`.
    If only one side is a string, a number, boolean, or null on the other side gets turned into a string first, following the [StringExpression](../../expressions/StringExpression.md#string-expression) rules. So `{2 + "Hello"}` gives you `2Hello`, and `{"Enabled: " + true}` gives you `Enabled: true`.
    A list, dictionary, or anchor doesn't get turned into a string. You get an implicit list instead, same as putting `{x}` in the middle of some text. So if tags is `[a, b]`, `{"Tags: " + tags}` gives you `["Tags: ", ["a", "b"]]`. If you want the name, use `${tags}`.
  symbol: +
- description: The merge operator (`+`) combines two lists or two dictionaries.
    On lists it joins them in order and removes duplicates. If a value shows up more than once, the last one is kept. So `[A, B, C, D] + [A, B, C]` gives you `[D, A, B, C]`.
    On dictionaries it's a shallow merge. You get the keys from both, and if both have the same key, the right side wins.
  symbol: +
- description: The `++` operator is like merge, but it keeps duplicates.
    On lists it joins them in order and keeps everything, so `[A, B, C, D] ++ [A, B, C]` gives you `[A, B, C, D, A, B, C]`.
    On dictionaries it's a deep merge. If both sides have the same key and both values are dictionaries, those get merged too, all the way down. Otherwise the right side wins.
    On strings it joins them with a line break.
  symbol: ++

## Plus Dispatch

The `+` symbol is shared by [AdditionOperator](../arithmetic/AdditionOperator.md#addition-operator), [ConcatenationOperator](./ConcatenationOperator.md#concatenation-operator), and [MergeOperator](./MergeOperator.md#merge-operator). Which one you get depends on the types on each side.
```markdown
| Left       | Right      | Operation                              |
| ---------- | ---------- | -------------------------------------- |
| number     | number     | AdditionOperator                       |
| string     | simple     | ConcatenationOperator                  |
| simple     | string     | ConcatenationOperator                  |
| string     | complex    | implicit list                          |
| complex    | string     | implicit list                          |
| list       | list       | MergeOperator                          |
| dictionary | dictionary | MergeOperator                          |
| other      | other      | compiler error                         |
```
