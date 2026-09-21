# Anchors

## Description

An object is a value. An anchor is a named structural declaration that can participate in inheritance, typing, exports, and language-level composition.
In Piton, the anchor is the core construct, the building block of the entire system. It shares some loose heritage with classes in OOP (instantiation not being one of them), but does have its own unique characteristics as well.
```piton
anchor MyFirstAnchor:
  whatIsAnAnchor:
    An anchor is a kind of object or document that is structured via
    properties and values.

  stringValue: String types are supported.

  listTypes:
    - List types
    - are
    - supported.

  nestedLists:
    - Nested list types are
      - also supported.
      - [And, With, Bracket, Syntax]

  numberTypes: {3.14 - 3.14}

  booleanTypes: true

  nullType: null

  // This evaluates to false
  booleanOperatorsAnd: {this.booleanTypes} && false
  // This evaluates to true
  booleanOperatorsOr: {this.booleanTypes} || false

  thisKeyword:
      As you may have noticed, Piton supports the `this` keyword for
      accessing properties on the current anchor. We'll talk about this
      later.
```
If we look at how this would compile to JSON, it would look like this:
```json
{
  "whatIsAnAnchor": "An anchor is a kind of object or document that is structured via properties and values.",
  "stringValue": "String types are supported.",
  "listTypes": ["List types", "are", "supported."],
  "nestedLists": [
    "Nested list types are",
    ["also supported.", ["And", "With", "Bracket", "Syntax"]]
  ],
  "numberTypes": 0,
  "booleanTypes": true,
  "nullType": null,
  "booleanOperatorsAnd": false,
  "booleanOperatorsOr": true,
  "thisKeyword": "As you may have noticed, Piton supports the `this` keyword for accessing properties on the current anchor. We'll talk about this later."
}
```

### Structural Inheritance

[StructuralInheritance](StructuralInheritance.md)

### Super

[Super](Super.md)

### Super On Lists

[SuperOnLists](SuperOnLists.md)

### Self Reference

[SelfReference](SelfReference.md)

### Abstract

[Abstract](Abstract.md)

### User Defined Keywords

[Keywords](Keywords.md)

Links in this document point at reference files. Read one when the work touches what it describes.
