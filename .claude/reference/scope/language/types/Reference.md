# Reference

## What Is A Type

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

## Supported Operators

- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

## Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](../variables/TypeCoercion.md#type-coercion) and [Inference](./Inference.md#inference).

## Description

A reference comes from a [ReferenceExpression](../expressions/ReferenceExpression.md#reference-expression). It points at an anchor, or a property on one, instead of copying it. How it ends up looking in the output is up to the renderer, and frameworks like Belay build on top of that.

## Renderers

```
json: A reference becomes a string: the path to the other file (relative to this one), a colon, and the dot path to the value. So `../file.json:Anchor.property`.
yaml: Same as json, with the yaml output file.
markdown: A relative Markdown link to wherever the anchor was rendered, with the anchor's name as the link text. So a reference to Button from a file next to it links Button to ./Button.md#button. A reference to a property links Button.color to ./Button.md#color.
```

## Equality

Two references are equal if they point at the same thing.
