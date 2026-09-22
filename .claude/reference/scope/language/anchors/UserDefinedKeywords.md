# User Defined Keywords

## Description

Now with abstracts defined, we can discuss user-defined keywords. This is really just syntactic sugar equivalent to extends that exists to promote clarity of code design in helping to emphasize certain base anchors as fundamental units.
And it’s very simple to do:
```piton
anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

my-anchor ChildAnchor:
    description:
        ${super.description}
        My additional description
```
That is exactly equivalent to using extends:
```piton
anchor MyAnchor:
    description: This is a description of my anchor

anchor ChildAnchor extends MyAnchor:
    description:
        ${super.description}
        My additional description
```
An anchor can be declared with only one keyword, but it can still use extends to inherit from other anchors as well.
```piton
anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

anchor OtherBase:
    description: Other Base

my-anchor ChildAnchor extends OtherBase:
    description:
        ${super.description}
        My additional description
```
This would be equivalent to:
```piton
anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

anchor OtherBase:
    description: Other Base

anchor ChildAnchor extends OtherBase, MyAnchor:
    description:
        ${super.description}
        My additional description
```
Note that the keyword's anchor is always placed last in the inheritance chain, so it wins collisions against anything listed in extends. Here ChildAnchor's description begins with “This is a description of my anchor”.
A user-defined keyword may not be one of the reserved words.
