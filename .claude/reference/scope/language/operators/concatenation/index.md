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

## Sets And Lists

What do you expect `{[1, 2, 3] + [1, 2, 3]}` to give you? `[1, 2, 3, 1, 2, 3]`? Sorry... It's actually `[1, 2, 3]`. When it's list to list, `+` removes duplicates and keeps the last one, so `{[1, 2, 3, 4] + [1, 2, 3]}` sadly becomes `[4, 1, 2, 3]`. It makes logical sense given that Piton is a left-to-right, right-wins language, but still, it's just generally like **ugh**. I get it, trust me I do.
Then there's `++`, which keeps the duplicates. I'm sorry Ken Thompson and like, the rest of all literal programming history. I think that `+=` is a perfectly efficient way to increment. And yeah, "C plus equals" is a horrible name for a language.
In my defense, is not the inherent duplication of the `+` operator to create the `++` operator a very ergonomic way to signify what would otherwise be distinguished as a set vs. a list? `+` gives you a set, `++` gives you a list.

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
