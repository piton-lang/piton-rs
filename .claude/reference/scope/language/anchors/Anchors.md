# Anchors

## Type Reference

[AnchorType](../types/AnchorType.md)

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
  booleanOperatorsAnd: {this.booleanTypes && false}
  // This evaluates to true
  booleanOperatorsOr: {this.booleanTypes || false}

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

#### Description

Piton uses structural inheritance, not polymorphism. Support for inheritance including multiple bases is accessed via the extends keyword.
```piton
anchor FirstBaseAnchor:
    firstBaseAnchorProperty: Hello from the First Base Anchor
    description: Description from FirstBaseAnchor

anchor SecondBaseAnchor:
    secondBaseAnchorProperty: Hello from the Second Base Anchor
    description: Description from SecondBaseAnchor

anchor ChildAnchor extends FirstBaseAnchor, SecondBaseAnchor:
    childAnchorProperty: Hello from Child Anchor
```
And as JSON:
```json
{
  "firstBaseAnchorProperty": "Hello from the First Base Anchor",
  "secondBaseAnchorProperty": "Hello from the Second Base Anchor",
  "description": "Description from SecondBaseAnchor",
  "childAnchorProperty": "Hello from Child Anchor"
}
```
One particular thing to note here is that both FirstBaseAnchor and SecondBaseAnchor include a description property, and the way that got inherited by ChildAnchor is a simple left to right where the last in line wins.
Type constraints inherited from concrete anchors participate in the same left-to-right collision resolution as property values; the right-most inherited definition wins and no error is reported. Conflicts between implemented abstracts follow the rules in the abstract anchors section.
An inheritance cycle, where an anchor directly or indirectly extends itself, is a compiler error.

### Super

#### Description

This brings up the question of what happens when a child anchor declares a property that is inherited from the base anchor, and how do we retrieve values from the base?
```piton
anchor BaseAnchor:
    description: Description from BaseAnchor

anchor ChildAnchor extends BaseAnchor:
    description: Description from ChildAnchor
```
This of course will simply use the child anchor’s value.
```json
{
  "description": "Description from ChildAnchor"
}
```
But if we wanted to specifically pull from the parent, that is possible using the super keyword.
```piton
anchor BaseAnchor:
    description: Description from BaseAnchor

anchor ChildAnchor extends BaseAnchor:
    description:
        ${super.description} and Description from ChildAnchor
```
As with the previous example of multiple inheritance, the anchor could be inheriting from multiple bases, in which case what does super point to? super is the merged view of everything the anchor inherits, built with the same authority as property inheritance: left to right, last in line wins. So when the right-most base does not define a property, super still finds it on an earlier base.
```piton
anchor Left:
    d: from-left

anchor Right:
    other: x

anchor Child extends Left, Right:
    d: ${super.d} plus child
```
Here Child.d is “from-left plus child”. Referring to a property that no base defines is a compiler error.
It’s worth noting the small detail here that we borrow the ${} syntax from other languages for string interpolation.
So the resulting JSON would be:
```json
{
  "description": "Description from BaseAnchor and Description from ChildAnchor"
}
```

### Super On Lists

#### Description

super on lists gets some extra attention that’s worth noting.
```piton
anchor BaseAnchor:
    items:
        - A
        - B
        - C

anchor ChildAnchor extends BaseAnchor:
    items:
        + {super.items}
        - D
        - E
        - F
```
This example will yield a final items of [A, B, C, D, E, F].
A block is built from top to bottom. A line beginning with + or ++ applies that operator between the value accumulated so far and the line's value, using the operand types to choose the operation, and the following items continue from the result. Here the block starts empty, the + line merges in super.items, and D, E, and F are appended. Without the + we’d end up with a nested list: [[A, B, C], D, E, F].
The same rule applies in string blocks, where + concatenates, and in dictionaries, where + and ++ merge. A block containing only a single `+ {x}` line is equivalent to `{x}`.
But let’s modify that example slightly to see a specific feature of the + concatenation operator.
```piton
anchor BaseAnchor:
    items:
        - A
        - B
        - C

anchor ChildAnchor extends BaseAnchor:
    items:
        - A
        - B
        - C
        - D
        + {super.items}
```
The change is that ChildAnchor now also contains A, B, C, and D, and we’re also bringing super.items in at the end of the list. The + merge operator concatenates in the order of the operands and removes duplicates, keeping the last occurrence of each value. So the accumulated [A, B, C, D] merged with [A, B, C] gives ChildAnchor [D, A, B, C].
One last example uses the ++ operator.
```piton
anchor BaseAnchor:
    items:
        - A
        - B
        - C

anchor ChildAnchor extends BaseAnchor:
    items:
        - A
        - B
        - C
        - D
        ++ {super.items}
```
The ++ operator keeps duplicates, so ChildAnchor ends up with [A, B, C, D, A, B, C].

### Self Reference

#### Description

Piton supports self-reference within anchors via both the self and this keywords, and there’s an important distinction between the two. Consider the following example:
```piton
anchor Base:
    name: Base Anchor
    baseDescription: This is ${self.name}

anchor Child extends Base:
    name: Child Anchor
    childDescription: This is ${self.name}
```
As JSON this would compile to:
```json
{
  "name": "Child Anchor",
  "baseDescription": "This is Child Anchor",
  "childDescription": "This is Child Anchor"
}
```
This is reasonable and expected behaviour, but I’ve often found when building inheritance hierarchies that I want a bit more control. That’s where this comes in. Where self will reference the most descendant anchor, this will reference the exact anchor. So in the above example, imagine we change self to this:
```piton
anchor Base:
    name: Base Anchor
    baseDescription: This is ${this.name}

anchor Child extends Base:
    name: Child Anchor
    childDescription: This is ${self.name}
```
While the rest of the example remains the same. The JSON output will now be:
```json
{
  "name": "Child Anchor",
  "baseDescription": "This is Base Anchor",
  "childDescription": "This is Child Anchor"
}
```
And just for absolute clarity, let’s take this example one step further:
```piton
anchor Base:
    name: Base Anchor
    baseDescription: This is ${this.name}

anchor MidChild extends Base:
    name: MidChild Anchor
    baseDescription: Override on baseDescription ${this.name}
    childDescription: This is ${self.name}

anchor FinalChild extends MidChild:
    name: FinalChild Anchor
```
Will output
```json
{
  "name": "FinalChild Anchor",
  "baseDescription": "Override on baseDescription MidChild Anchor",
  "childDescription": "This is FinalChild Anchor"
}
```
As you can see, this gets pinned to wherever it’s used, while self travels through the hierarchy.
self only travels through inheritance. Reading another anchor's property, without extending it, gives that anchor's own value with self bound to that anchor:
```piton
anchor Precedence:
    note: ${self} rules apply

anchor Addition:
    copied: ${Precedence.note}
```
Addition.copied is “Precedence rules apply”.
this and self always refer to anchors, never to a nested dictionary. Inside a nested dictionary they still refer to the enclosing anchor.

### Abstract

#### Description

Abstracts allow us to define the shape of an anchor. An abstract anchor alone will never compile; it must be extended by a non-abstract anchor, and that non-abstract anchor must give a value to every required property of the abstract.
```piton
abstract anchor Skill:
    description:: string


anchor ConcreteSkill extends Skill:
    description: This must be a string as defined by the abstract
```
We’ll introduce a new bit of terminology here in that a concrete anchor that extends an abstract anchor is said to be “implementing” the abstract anchor.
An abstract may give a property a default value. A property with a default is optional for the implementer; one without a value is required. See the required and optional rules under type constraints.

#### Abstract Chains

An abstract anchor may extend another abstract anchor. The implementing anchor must satisfy every required property anywhere in the chain.
```piton
abstract anchor Construct:
    description:: string
    prompt:: string

abstract anchor Skill extends Construct as skill:
    useWhen:: string

skill Review:
    description: Reviews code
    prompt: Review the diff
    useWhen: Asked for a review
```

#### Multiple Abstracts

An anchor may implement more than one abstract, including abstracts that share an ancestor. It is good practice to export an abstract anchor as a keyword, and a keyword's anchor is always placed last in the inheritance chain, so the keyword's abstract wins.
When two implemented abstracts constrain the same property:
If the constraints have no type in common, such as string and number, it is a compiler error. If they share at least one type, the right-most abstract's constraint wins.
You are still free to extend concrete anchors in addition to implementing abstracts. Constraints inherited from concrete anchors follow ordinary right-most-wins inheritance and never error.

#### Special Type Constraints

We are also able to use the specialized type constraints within abstracts like simple, complex, any, [], etc.
The extends type constraint keyword represents the inheritance hierarchy. It can be used in any type constraint, not only within abstracts. Take the following example.
```piton
abstract anchor A:
    description:: string

abstract anchor B extends A:
    name:: string

abstract anchor C:
    listOfA:: A[]
    listOfB:: B[]

    listOfExtendsA:: extends A[]
```
Here, we have defined an anchor A with a description and an anchor B which extends A and adds a name property. So B has both description and name while A only has a description.
We have then defined an anchor C. Let’s go one property at a time.
A plain anchor type accepts an anchor whose nearest abstract is that anchor. The nearest abstract is the abstract that wins in the anchor's inheritance chain: its keyword's abstract if it uses one, otherwise the right-most abstract it extends.
listOfA will be satisfied by anchors whose nearest abstract is A.
listOfB will be satisfied by anchors whose nearest abstract is B. An implementer of B does not satisfy listOfA, even though B extends A.
Where it gets a little more interesting is in the third property, listOfExtendsA.
extends on an anchor type reference means that the constraint may be satisfied by any concrete anchor whose inheritance chain includes that anchor.
So in this example, that constraint is satisfied by a list of anything that has A in its inheritance hierarchy. So in this case, you’d be able to pass concrete anchors that implement either A or B given that B has A in its inheritance chain.

### User Defined Keywords

#### Description

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

Links in this document point at reference files. Read one when the work touches what it describes.
