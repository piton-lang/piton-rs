# Special Type Constraints

## Special Type Constraints

There are three additional type annotations that we can use when we want to deal with slightly more fuzzy conditions. These are any, simple, and complex.
The any constraint will allow any type, simple, complex, number, string, boolean, etc.
The simple type will allow any simple type, which we’ve previously defined, but includes string, number, boolean, and null. It specifically avoids lists, dictionaries, and anchors.
The complex type allows for any complex type, in other words, list, dictionary, and anchor.
```piton
a:: complex: [1, 2, 3]
b:: complex:
    a:: number: 1
    b:: number: 2
c:: simple: 1
d:: simple: Hello, World
e:: simple: false
g:: any: 1
h:: any: Hello, World
i:: any:
    - String
    - NestedObject:
        a:: number: 1
        b:: number: 2
```
