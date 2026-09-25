# Declaration Operators

## Description

Now let's tackle the weirder ones. These two don't go inside expressions. They're how you write a declaration in the first place, so they aren't part of the precedence table, and no type lists them as supported.

## Operators

- description: So this is just thing equals thing. Like in most languages `var thing = "thing"`, but since Piton is this sort of Python/YAML/JSON hybrid thing, it just makes sense to stick with `:`. There always has to be a space after it. See [VariablesOverview](../../variables/Overview.md#variables-overview).
  symbol: :
- description: While not mandatory or even very relevant unless defining generic anchors, `::` is used for type annotation. The reason it's not very relevant within the current scope of what you've learned about the language is that Piton has no runtime, and variables are immutable. One could argue that defining an explicit type for a variable is a form of self-documenting code. Sure, that's not wrong, but Piton is so forgiving about types that that argument is kind of like saying "don't use a parrot to eat a forklift"; yeah, got it. Wasn't going to.
    Where it earns its keep is in abstract anchors, where a constraint with no value makes a property required. See [TypeConstraints](../../variables/TypeConstraints.md#type-constraints).
  symbol: ::
