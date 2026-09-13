# Go To Implementation

## Description

Jumping from a shape to the anchors that fill it in

## Targets

- An anchor, or a keyword that declares things as that anchor, goes to every concrete anchor whose inheritance chain includes it.
- A property goes to every declaration of it, in its family, that a concrete anchor writes in its own body.
- Nothing else has implementations, and asking returns nothing.
