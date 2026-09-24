# Scope

## Scope

Since variables are defined at the top level of the file in which they’re defined, that file is their scope; they are a “global” within that file. You can export a variable or anchor to make it available to other files and modules, and we’ll cover modules in just a few sections.
Inside an expression, a plain name always means a variable from the file (or something you imported), even inside an anchor. To get at a property on the anchor you're in, use this or self.
```piton
x: top-level

anchor Card:
    x: card
    fromFile: {x}
    fromCard: {this.x}
```
Card.fromFile is “top-level” and Card.fromCard is “card”.
