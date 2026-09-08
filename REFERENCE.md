# The Piton Language

## Introduction

Piton is a programming language designed specifically for writing skills,
agents, and reference documents for Agentic Software Development. To call it a
programming language is a bit generous, but well, it's not exactly natural
language either. Semi-natural programming language? A SNPL? Anyway...

The core thesis is that any programming language has always been an abstraction
on top of the layer beneath it, that once a new abstraction is generally
accepted we no longer care about how the compiled artifacts are generated as
long as they're "good", and that agentic development is just another evolution
in that history of abstracted apathy.

I would also suggest that what humans are good at has never been programming
itself, but rather designing coherent (maybe) functioning (hopefully) systems of
little doodads that twirl and whir and click and clack together in an elegant
(occasionally) way. Let's define "traditional programming" as the likes of
Assembler, C, Python, or those handful of awkward years where we thought
delivering precious stones by way of a train was the best way to stop drinking
coffee. Traditional programming is a very clever mechanism in that it allows us
to describe, constrain, and spend time thinking about the systems we're building
while also building the artifactual doodads -- or at least the highly detailed
instructions required for the compiler to build them.

In approaching Agentic Development, my hypothesis is that we need a way to do
exactly the same describing, constraining, and thinking that we are and will
remain skilled at, while letting go of the manual labour that is writing what we
collectively determined to be the latest acceptable abstraction sitting on top
of the little sparks zipping around controlling a computer.

And with that, I posit that you should be able to copy and paste the entirety of
this essay into an agentic coding tool and it will output a compiler for the
Piton language and Belay framework.

## The CNC Comparison

I realize I've probably already driven home my point and tucked it nicely into
bed. However, there's one particular example I've found myself thinking through
regularly, the structure of which I think functions well enough that it can
imbue my thesis with a bit of sound nuance.

My dad is an aerospace engineer, and he was very much present for the transition
from traditional machine operation to CNC (Computer Numerical Control)
machining. He has described many of the same "those damn robots" attitudes we're
seeing now as our industry is facing what the ambulance-chasing among us would
call "extinction." Pish posh, says the piscine aristocracy.

Let's discuss milling as an example; a spinning cutter moves through the
material being machined tiny little bit by tiny little bit, gradually removing
material until a big bland block of aluminum turns into a smaller piece of
aluminum shaped possibly less like a block. In traditional machining, the hand
of a machinist turns dials to move the cutting point along the X, Y, and Z axes.
Involved in this skill is a combination of math, materials science, dexterity,
and the tacit understanding of how operations literally sound and feel when
they're doing well or not so well.

CNC machining replaced the hand. With CNC, a most-uninteresting robot composed
of three motors, one each attached to the X, Y, and Z dials does the turning.
Those motors are controlled by a simple programming language called G-code. And
let's be clear, even BASIC calls G-code basic as it looks it up and down.

```
G1 X40 Y0 F300
G1 X40 Y30
G1 X0  Y30
G1 X0  Y0
```

`G1` means linear movement. Move in a straight line from where you are to where
I tell you.

`X40` means X coordinate of 40 units. `Y0` is Y coordinate of 0 units.

`F300` means "feed rate" of 300 units per unit of time. AKA, how quickly we
move.

So, yes. We made a rectangle in two dimensions. And bear in mind, CNC milling is
usually only useful in three dimensions -- or at minimum what the industry likes
to call 2.5D Machining (they misplaced the other half-dimension kind of like
they misplaced their tape measure).

It's a sort of turtle graphics approach to controlling the robot. And while of
course there's more depth to the language than what I've shown here, even this
simple example demonstrates why the abstraction of G-code itself became a
lower-level substrate for another abstraction. Turtle graphics all the way down.

But what CNC didn't and doesn't replace is the majority skill of the machinist.
Perhaps their specific kind of dexterity becomes less relevant, and certainly
the "literal feel" of the machine operation meets a dead end at the closed-off
entrance of a robot's emotionally damaged heart. But the rest stays, if not also
given the space to gain precision.

I hinted at the problem already. I also gave imprecise clues to it. It's
repetition; if what we did is cut a rectangle, we'll need to repeat that again
and again in the small increments the machine, cutter, and material are able to
physically handle as we move deeper into the billet to achieve the desired depth
of cut. And that's just for an extruded rectangle. Imagine what the code might
look like for carving out a complex three-dimensional curved surface.

There's nothing stopping you from manually writing G-code that complex, just
like there's nothing stopping you from building a banking system in Brainfuck or
building an editor-based operating system in LISP. But why would you do that --
to yourself or us?

And that's where CAM (Computer-Aided Manufacturing) comes in. CAM combines the
input of the engineer's 3D model (the design intent) with the reformed
machinist's expertise in order to output G-code. The CAM engineer may never
touch the machine directly, nor may they ever type a single line of G-code, but
their involvement and knowledge are critical to a functional outcome.

We could also acknowledge that what was once two jobs (engineer and machinist)
is now three jobs (engineer, machinist, and CAM engineer). And I'm doing that by
stating it, but I'll leave it at that. Otherwise I might lose the plot of this
essay, kinda like -- dammit, where's my 10mm socket.

While I've found the CNC comparison a useful framework through which to organize
my thoughts about recent changes in software development, it also serves as a
literal and pragmatic slice of history that's far enough back for the outcomes
to be well-established while also close enough to feel immediately comparable.

## Agent as Compiler

When I worked at a consulting agency, the regular joke among developers was that
95% of software consulting work is rebuilding an Excel spreadsheet that outgrew
itself into code. Hyperbolic? Yes. Untrue? Not so much. And of course if you're
the solve-all-the-problems type you end up spending cycles on "but what if we
could build a software that was powerful like code, but also simple like a
spreadsheet?" And you end up designing a spreadsheet.

The reason so much of software development feels the same is because, quite
simply, a lot of it is. How many CRUD REST APIs have you built, connected to a
data base, implement a job queue? Please make it stop. How many times have you
written an application layout in HTML/CSS > Bootstrap > Backbone > Sass >
Angular > React -- please make it stop. How many times have you typed NavBar?
How many times have you abstracted a list of items with create, edit, delete --
a form that popups up in a modal or drawer. Oh wait, that modal needs a curtain.
Please, Make. It. Stop.

We, the collective developer blob, have contintually invented, reinvented, and
adapted sometimes brilliant and sometimes less brilliant mechanisms to abstract
the very literal step-by-step instructions of G-code, Assembly, or even BASIC
into more conceptual shapes. Functions, classes, interfaces, generics -- none of
these are the actual problem; they are _beautiful_ concepts wrapped around the
process of solving the problem once the problem becomes a pattern.

And please, do not misunderstand me for even a moment; it has been one of the
tremendous joys of my life to have the honour of learning, understanding, and
building with these concepts that so many brilliant people have created over the
years. And perhaps that's even part of my larger point; I don't want to lose
that joy by simply prompting an agent "MAKE IT MORE" -- I want to use those
concepts to focus on building the systems they support so well.

But perhaps I never wanted to write code itself; I want to create. I want to
build. And I think there's many forms of what "creating" looks like if we step
outside the software world. If I sketch out and design a backpack in great
details, then send it to professionals to figure out how to make it, how to cut
the patterns, how to sew it, is it "my creation" or not? I would argue that
sure, yes, it was my idea. I developed the concept, worked out the details, and
then asked for help when I didn't have the expertise to make the patterns or do
the sewing myself.

## The Problem

As potent as the wow-factor is when you prompt an agent with a minimal
description of what you want and it builds it, that process has left me feeling
like a lot is missing -- not strictly from the result, but more from my
involvement in the process and my ability to precisely control the outcome.
"Great job, brother Claude. Now make X work like Y." It does, but also ends up
making C into a Q. So you prompt again, and prompt again, again. Eventually
you're almost certain to arrive on something close enough to correct, and
sometimes this is absolutely fine. If you're building a small, one-off tool then
this is nothing short of incredible.

But, I would argue that for for a sufficiently complex system, something more
robust and traceable is needed, something that is an artifact of the process
itself, not just the output of the process. I believe this becomes even more
relevant as more people work on an agentic codebase; the prompts than let to an
outcome live within the session, and at best are a faint memory in the one mind
of the one developer. Despite how it may seem, nether agents nor humans can read
minds.

Many of the agentic platforms provide mechanisms like `AGENTS.md` and `CLAUD.md`
that provide a level of instruction to the agent. Further still, more granular
skills and agents are written in Markdown and comitted to the repository. Now
you can write durable instructions that the agents will reference when building
the code.

The problem I started to encounter is that in the aim of describing
interconnected systems, pure prose is inefficient and prone to decay. Any author
is likely to inadvently describe subsystems and instructions with phrasing that
is at best ambiguous enough to cause conflict. With traditional programming, we
have all these wonderful tools to promote reuse and avoid duplication. Markdown
doesn't have these tools.

At the same time, pure prose is an incredible way to say

> "the button should be blue with contrasting text."

Compare that to

```css
.button {
  background-color: #00a;
  color: #fff;
}
```

Which one would you rather write?

It's this combination of structure and prose both as first-class citizens that
Piton attempts to solve. So with that, let's take a look at Piton.

## The Language

The Piton language, while primarily intended for writing Agentic constructs, is
also at its core a declarative language for defining data. So before we get into
discussing the agentic stuff, let's take a look at the language itself through
the lens of just data.

There's one thing to make very clear upfront: There is no runtime for Piton.
Piton compiles to data -- JSON, YAML, Markdown, etc.- via adapters. While that
ultimately simplifies the mental model, it's important to keep this in mind as
you're learning how to use the language.

### Source Files and Modules

Piton files carry the `.pi` extension.

Like Python, Node, and others, directories become modules with an `index.pi`
file that `export`s symbols `import`ed from other files. That means that when
doing an `import` you can point at the directory rather than at a specific file.

### Whitespace and Structure

The language is a whitespace-based language. Like Python, you can use tabs or an
arbitrary number of spaces for indentation. However, you must be consistent
throughout the file. And while it's not mandatory, like Python's PEP8, Piton
prefers 4 spaces per indent level, not tabs. That's what `piton format` will
apply to your code, and it's not configurable.

### Comments

Comments are line-only. There are no block comments. Comments and are created
with `//`.

```piton
// This is a comment
myVariable: 42 // This is also a comment
```

`piton format` will always put a space between the `//` and the comment text, so
you might as well get used to doing it yourself.

### Keywords

Keywords are reserved words that have special meaning in Piton. `string`,
`false`, `anchor`, `export`, etc. are all examples. A unique aspect of Piton is
that you can define your own keywords that act as a sort of syntactic sugar for
inheritance. But that's a topic we'll discuss later. Keywords must be all
lowercase and can be kebab-case.

### Values and Types

Piton is loosely typed with optional type constraints.

All the usual suspects are present:

- `string` - A unicode string
- `number` - All-encompassing numeric type; int or float.
- `boolean` - `true` or `false`
- `null` - The abscence of a value
- `list` - A list of values. Value types can be mixed as long as it's not
  contrained by a type annotation.
- `dictionary` - A dictionary of key-value pairs. Key types must be strings, and
  value types can be mixed as long as it's not contrained by a type annotation.
- `anchor` - The core structural building block of Piton. Don't worry, there's a
  whole section on this.

#### Numbers

Numbers are written as literal numbers. Leading 0 is mandatory for decimals, and
you can use underscores to separate large numbers for readability.

```piton
myInt: 123
myFloat: 3.14
mySmallFloat: 0.14
myBigNumber: 1_200_000.00
```

There is a single number type in all of Piton; type-wise there's no difference
between an int, a float, double, etc.

#### Strings

Strings are not quoted. They can appear on the same line as what they're
assigned to, or indented on the next line:

```piton
sameLine: Hello, World!
nextLine:
    Hello, World!
```

The leading whitespace on a string block is discarded, as that's part of the
syntax of the language, not the string. In other words, the string in both
examples is "Hello, World!".

Within a string block, you can add line breaks without affecting the structure
of the string. To create an explicit line break, you must include a blank line.

```piton
myString:
    This broken string is not considered
    a line break.

    While this *is* on a new line because there was a blank line above.
```

When compiled, we'll get two lines:

```
This broken string is not considered a line break.
While this *is* on a new line because there was a blank line above.
```

##### Escaping

There are two ways to escape strings, both of which are slightly different. The
first is the common backslash `\` character. For example, if you wanted a string
that starts with `//` and don't want it to be treated as a comment, you can
escape the `//` with `\//`.

The second way to escape strings is with quotes. So for example, you should also
escape a comment and treat it as a string with `"// This is a comment"`. The
quote escape only acts as an escape mechanism if it's applied to the entire
string, or if it's actually escaping something. So in the following example:

```piton
stringA: "This is a string"
stringB: "// This looks like a comment but is a string"
stringC: "false"
```

`stringA` becomes a string value of, dropping the quotes `This is a string`.
There's no inherent value in doing this, but it demonstrates behaviour.

`stringB` becomes a string value of `// This looks like a comment but is a
string`.

`stringC` becomes a string value of `false`.

You can also escape quotes with `\"`, however that should rarely be necessary.
Note, you can escape literal values like `false` with `\` as in `\false` in
which case it's treated as a string. Personally, I find that far less ergonomic
than `"false"`.

#### Booleans

Booleans are represented with lowercase `true` and `false`

#### Null

Null is represented with lowercase `null`. Null is a value that means "nothing."
So yes, `null == null`. Piton doesn't have the concept of `undefined` or any
other "nothing" value.

#### Collections

Piton has two core collection types: lists and dictionaries -- or arrays and
objects depending on what language you're coming from.

##### Lists

Lists can be defined two ways:

```piton
markdownStyle:
    - One
    - List
    - Item
    - Per
    - Line

inlineStyle: [This, is, a, list, of, strings]
```

It's possible to do nested lists:

```piton
nestedMarkdownList:
    - Level 1
        - Level 2
            - Level 3
```

Which is equivalent to:

```piton
inlineMultiList: [Level 1, [Level 2, [Level 3]]]
```

Piton intentionally does not provide a way to access items within a list.
Because this is not a runtime-based general purpose language but rather a
language designed for description, a list is a construct intended for merging
via inheretence; `myList[0]` is not very descriptive, is it?

##### Dictionaries

Dictionaries are nested keys and values, and there is only a single way to
define them:

```piton
firstLevel:
    secondLevel:
        thirdLevel: This is a string
```

You can use dot syntax to access keys in a dictionary.
`firstLevel.secondLevel.thirdLevel` would yield "This is a string".

Object keys can include hypens and understore as well as valid Unicode letter
characters and Unicode numeric characters. They cannot include spaces.

##### Combining Collection Types

You _can_ combine lists and dictionaries (and other types for that matter):

```piton
combined:
    This is a string

    - This
    - Is
    - A
    - List

    nestedDictionary:
        deeplyNestedDictionary: This is a string
```

And this scenario is where Piton takes a little liberty in syntax strictness for
the sake of human writability and readability. Because we're mixing types, what
happens here is that `combined` becomes an implicit list that has a `string`,
a `list`, and a `dictionary` inside of it. As JSON that would be:

```json
{
  "combined": [
    "This is a string",
    ["This", "Is", "A", "List"],
    {
      "nestedDictionary": {
        "deeplyNestedDictionary": "This is a string"
      }
    }
  ]
}
```

We're sacrificing the otherwise simple rules of syntax here only because this is
an inherently intuitive form for a human. We'll let the compiler do a bit of
heavy lifting to make the human's job nicer.

wi

One additional gotcha is with Piton, with an implicit list, dictionary
properties declared directly within the mixed block remain addressable; we're
still be able to directly reference
`combined.nestedDictionary.deeplyNestedDictionary` and get back "This is a
string". As I said previously, Piton does not allow for list accessors, so
trying to access either the string or the list is not possible.

Similarly:

```piton
combined:
  - dictionaryInsideAList:
      nested: value
```

Since the `dictionaryInsideAList` is in iniside an explicit list, you cannot
access that dictionary anymore. I'm just pointing this out because it is
syntactically possible, though arguably not a very wise choice in structuring
your data.

#### User-Defined Types

There is exactly one user-defined type, and it's called an `anchor`. This is
something that can shaped by other types via properties and promotes
inheretence. However, it's a larger and more advanced topic than belongs in this
introductory part of the guide, so instead we've devoted an entire section to it
later on.

#### Type Inferrence

Unless specifically constrained to a type, a variable or property can hold any
type. The type is inferred from the value.

| Expression            | Inferred As                                                                        |
| --------------------- | ---------------------------------------------------------------------------------- |
| `true`                | `boolean`                                                                          |
| `true story`          | `string`                                                                           |
| `42`                  | `number`                                                                           |
| `42 things`           | `string`                                                                           |
| `A + B`               | If A and B are the same type, inferred is also that type. Otherwise it's a string. |
| `This costs $5 + tax` | `string`                                                                           |
| `null`                | `null`                                                                             |
| `\// Just Text`       | `string`                                                                           |
| `"// Also Just Text"` | `string`                                                                           |
| `${}`                 | `string`                                                                           |

### Variables

Variables are declared simply by naming them at the top level of a Piton file.
You use `:` to assign a value to a variable.

```piton
myVariable: 42
```

#### Type Constraints

You can add type constraints to variables using `::`

```piton
myVariable:: number: 42
```

> Note that the white space after both `::` and `:` is important.  
> `myVariable::number:42` would cause a compiler error. Why? Because including
> the space makes it more readable.

It's also possible to add multiple type constraints to a variable using an
additional `::`:

```piton
myVariable:: number:: string: 42
```

This will allow `myVariable` to be either a `number` or a `string`, and the
order of preference is constraint fulfillment left to right. In the above
example, `myVariable` will be a `number` since 42 can be evaluated as a number.

However, if you flip it around:

```piton
myVariable:: string:: number: 42
```

myVariable will be a `string` since 42 can also be a string and string is
evaluated first and wins. One final example:

```piton
myVariable:: boolean:: number:: string: "false"
```

Since `"false"` is explicitly quoted, it fails the `boolean` constraint, and it
also failsthe `number` constraint, so `myVariable` is a `string`.

#### List Type Constraints

List type constraints are possible with the `[]` syntax.

```piton
myVariable:: string[]: ["foo", "bar", "baz"]
```

`myVariable` must contain a list of strings.

#### Nested Type Constraints

It is possible to constrain nested keys inside a dictionary.

```piton
myVariable:: dictionary:
    a:: number: 1
    b:: string: "foo"
```

#### Special Type Constraints

There are three additional type annotations that we can use when we want to deal
with slightly more fuzzy conditions. These are `any`, `simple`, and `complex`.

The `any` constraint will allow any type, simple, complex, number, string,
boolean, etc.

The `simple` type will allow any simple type, which we've previously defined,
but includes string, number, boolean, and null. It specifically avoids lists,
dictionaries, and anchors.

The `complex` type allows for any complex type, in other words, list,
dictionary, and anchor.

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

#### Type Coercion

Type coercion in Piton is intentionally limited. When a value is constrained to
multiple types, Piton evaluates the constraints from left to right and uses the
first type the source value can validly represent.

Unquoted literals may be coerced when their syntax is compatible with the target
type. For example:

```piton
value:: string:: number: 42
```

Tihis evaluates to the string "42" because string is the first compatible
constraint.

Quoted values are explicitly strings and are never coerced to another type:

```piton
value:: boolean:: string: "false"
```

This evaluates to the string "false", not the boolean `false`.

Likewise, values that are already structurally typed, such as lists,
dictionaries, anchors, booleans, and null, are not coerced into unrelated types.

If none of the declared constraints can accept the value it is a compiler error.

#### Mutability

Given that there is no runtime for Piton, all variables are immutable. If we
introduce compile-time mutability then I think we're just asking for chaos.

#### Scope

Since variables are defined at the top level of the file in which they're
defined, that file is their scope; they are a "global" within that file. You can
`export` a variable or anchor to make it available to other files and modules,
and we'll cover modules in just a few sections.

### Control Flow and Functions

There are no conditionals or loops in Piton with the exception of the ternary
operator.

There are also no functions.

### Operators

Piton supports a pretty standard if not small set of operators, plus a few
slightly more unique ones. Let's start with the standard ones.

#### Standard Operators

- Arithmetic operators: `+`, `-`, `*`, `/`, `%`
  They do math things.
- Comparison operators: `==`, `!=`, `>`, `<`, `>=`, `<=`
  Introduces FOMO and jealousy.
- Logical operators: `&&` and `||`
  Nerds.
- Ternary operator: `?` `:`
  Ternary is just a fun word to say, dind of like "Guido" is fun to say. But my
  name isn't Guido, so I Have to include the ternary operators in the language.
- Property Access operator: `.`
  If only buying land were as cheap as accessing a value from a dictionary.

Some immediate details to clarify:

- Arithmetic operator precedence follows standard mathematical precedence: `*`,
  `/`, and `%` are evaluated before `+` and `-`. Operators with the same
  precedence are evaluated from left to right. Parentheses may be used to alter
  the order of operations.
- Comparison operators have higher precedence than logical operators.
- Logical operator precedence is `&&` before `||`.
- The ternary operator has lower precedence than `||` and associates from right
  to left.
- We've already detailed how the property access operator `.` works. I'm just
  mentioning it to clarify that yes, it's technically an operator.

#### Assignment and Type Annotation Operators

Now let's tackle the weirder ones. The first one isn't too bad.

- Assignment - `:`

So this is just thing equals thing. Like in most languages `var thing =
"thing"`, but since Piton is this sort of Python/YAML/Json hybrid thing, it just
makes sense to stick with `:`.

And now another weird one:

- Type Annotation - `::`

While not mandatory or even very relevant unless defining generic anchors
(anchors being an entire topic we'll cover later), `::` is used for type
annotation. The reason it's not very relevant within the current scope of what
you've learned about the language is that Piton has no runtime, and variables
are immutable. One could argue that defining an explicit type for a variable is
a form of self-documenting code. Sure, that's not wrong, but Piton is so
forgiving about types that that argument is kind of like saying "don't use a
parot to eat a forklift"; yeah, got it. Wasn't going to.

And this one will take you for a rollercoaster, because it starts off normal,
then gets really weird.

#### Concatenation Operators

- Concatenation operators: `+`, `++`

The little secret here to keep in mind is that the "normal" doesn't even last
very long.

```pition
myVariable: "This is a" + "string"
```

Yeah, cool. That makes sense, right? The one thing worth noting is that we're
quoting the strings so that `+` is not treated as just a string. It is now a
distinct operator operating on strings.

```piton
myVariable: "The number is" + 5
```

Per our previous rules about mixing types, the `5` turns into a string, so the
final string is "The number is 5". Making sense so far, right?

Now let's get a little weird.

```piton
myVariable: "Hello, World" + [1, 2, 3]
```

Yeah... this turns `myVariable` into a list. The first item of which is a string
"Hello, World" and the second item is a list `[1, 2, 3]`. In JSON that would be:

```json
["Hello, World", [1, 2, 3]]
```

Or even as Markdown (which I know we haven't really talked about much yet) but
this would compile to:

```markdown
Hello World

- 1
- 2
- 3
```

If you're pissed off now, just wait. I'm going to make you a lot more angry. And
let's just dive into it.

```piton
myVariable: [1, 2, 3] + [1, 2, 3]
```

What do you expect? `[1, 2, 3, 1, 2, 3]`? Sorry... It's actually `[1, 2, 3]`.
When it's list to list, it's concatenation plus deduplication. The deduplication
is carried out in the order of the operands. So for example:

```piton
myVariable: [1, 2, 3, 4] + [1, 2, 3]
```

Uh... so, that sadly would become `[4, 1, 2, 3]`. And I mean it makes logical
sense given that Piton is a left-to-right, right winning language which we'll
talk a lot more about in the anchors and inheretence section. But still, it's
just generally like **ugh**. I get it, trust me I do.

So you good? You need a second to prepare for what's next? The `++` operator.
I'm sorry Ken Thompson and like, the rest of all literal programming history. I
think that `+=` is a perfectly efficient way to increment. And yeah, "C plus
equals" is a horrible name for a language.

Alright, the `++` operator:

```piton
myVariable: [1, 2, 3] ++ [1, 2, 3]
```

That results in `[1, 2, 3, 1, 2, 3]`. It functions the same as the `+`
concatenation operator but without the deduplication. In my defense, is not the
inherent duplication of the `+` operator to create the `++` operator a very
ergonimic way to signify what would otherwise be distinguised as a set vs. a
list?

#### Notes on Concatenation

`+` and `++` both perform list concatenation and dictionary merging.

For lists, `+` performs deduplication while `++` does not. Concatenation
is performed left to right, with the right-hand operand taking precedence over
the left-hand operand. When deduplication is performed, left-side operand items
will be removed and right-side operand items will be added in the order of
concatenation. Deduplication on lists is shallow.

Merging on dictionaries is shallow with `+` and deep with `++`.

#### Additional Notes on Operator Behaviour

- Concatenation operators `+` and `++` have the same precedence and associate
  from left to right.

- `true` is truthy and `false` is falsy. Obviously.
- All non-zero numbers are considered truthy. Zero is falsy.
- An empty string is falsy.
- `null` is considered falsy.
- `null == null` evaluates to `true`, and `null != null` evaluates to `false`.

- Using `+` with a `string` and a `number` will automatically cast the
- `number` to a `string` and the results will be concatenated. With anything
  else, the containing variable will become a mixed list.
- Using `+` with two or more lists will concatenates the lists into a single
  list, deduplicating any items with left to right, latest winning.
- Using `++` on lists will concatenate the lists into a single list and will
  keep duplicates.
- Comparison operators on mismatched types are a compiler error.
- Division by zero is a compiler error.

- Logical operators short-circuit. The right-hand operand of `&&` is only
  evaluated if the left-hand operand is truthy, and the right-hand operand of
  `||` is only evaluated if the left-hand operand is falsy.
- Modulo follows floor-division semantics. The result has the same sign as the
  divisor, or is zero.

### Expressions

An open question currently is what happens if a variable references another
variable. We've hinted at this in a previous example where we added `A` and `B`
but now let's be clear about it.

Easy expression using literals:

```piton
myVariable: 1 + 2
```

`myVariable` will be evaluated to `3`. Evaluation will of course follow all the
rules we've previously defined about types as operators; `2 + Hello` will
evaluate to a string `2Hello`

Let's look at an example that uses other variables:

```piton
a: 1
b: 2
result: a + b
```

`result` will be evaluated to the `string` `a + b`. Not quite what you were
expecting, huh? In order for this to evaluate to `3` you'll need to use the
`{}` expression syntax:

```piton
result: {a + b}
```

This makes the job of the compiler much easier, and let's us avoid the
problematic situation of string fallback in case a symbol isn't recognized.

Forward references are fully resolved. Unresolved references are a compiler
error. Cyclic references are also a compiler error.

We encounter a probably intuitive but perhaps less obvious scenario when we use
complex types like `list`, `dictionary`, and the yet-to-be-discussed `anchor`.

```piton
myList: [1, 2, 3]

myDictionary:
    list: {myList}

newList: {myDictionary.list + [4, 5, 6]}
```

`newList` will evaluate to `[1, 2, 3, 4, 5, 6]`.

In this case, `{myList}` resolves to the value of `myList`, which is then
assigned to `myDictionary.list`. The `newList` expression resolves
`myDictionary.list`, combines that value with `[4, 5, 6]`, and evaluates to `[1, 2,
3, 4, 5, 6]`.

We haven't discussed anchors yet, but they resolve by reference, and the
original identity is preserved.

### String Interpolation and Framework Extensibility

We've mentioned string interpolation already with the `${}` syntax. There's some
nuance to discuss here, as well as some options for extensibility.

Let's look at some examples:

```piton
name: Piton
message: Hello, World from ${name}
```

In this example `message` will compile to "Hello, World from Piton".

```piton
version: 1.0
message: Piton is at version ${version}
```

`message` will compile to "Piton is at version 1.0".

In more general terms, string interpolation will replace `${}`-enclosed
expressions with their evaluated literals so long as those values are simple
types. What's a simple type? `string`, `number`, `boolean`, `null`. In Piton,
those are the only simple types. Complex types include a `list`, `dictionary`,
and `anchor`.

In the case of complex types string interpolation will be handled by a Piton
framework, which is a concept we haven't discussed yet. The short version is
that a framework is a combination of Piton modules as well as a "plugin" for the
compiler that helps compile Piton into an appropriate target.

So let's look at an example:

```piton
metadata:
  name: Piton
  version: 1.0
message: Information about Piton ${metadata}
```

If we were compiling this into JSON using the JSON framework it might look like:

```json
{
  "message": [
    "Information about Piton",
    {
      "name": "Piton",
      "version": "1.0"
    }
  ]
}
```

But if we were compiling into Markdown it might look like:

```markdown
Information about Piton [metadata](#metadata)

# metadata

name: Piton
version: 1.0
```

This may seem a touch convoluted, but the core ability to interlink structures
in a way that's meaningful to the context of the target output is part of the
entire point of Piton. That's why string interpolation for complex types is part
of the framework, not core to the language itself.

Which now brings up an additional feature that's handled by frameworks. `${}` is
the default syntax for string interpolation, and it will always be the fallback
behaviour. However, that `$` sigil can be arbitrary; you could have `@{}` or
`reference{}` `link-to{}` all of which are defined and handled by the framework.
In order to maintain cross-framework compatibility, if a sigil is not
recognized, it will be treated as `${}` while throwing a compiler warning.

As an example of why this might be useful, consider a framework that defines
`reference{}` as a custom sigil for instructing an agent to go read a file.
We're jumping ahead and showing some features we haven't really talked about,
but bear with us.

```piton
// This is an externally defined Piton file that describes how to style a
// button component
from ./ButtonDesign import ButtonDesign

export anchor ButtonComponent:
    description:
        The button component should be clickable, should have hover state, etc.

    design:
        For details about the design, reference{ButtonDesign}
```

Would compile to agentic Markdown as:

```markdown
# ButtonComponent

## Description

The button component should be clickable, should have hover state, etc.

## Design

For details about the design, read `../reference/ButtonDesign.md`
```

So rather an massively duplicating text everywhere, we're able to reference the
reused bits even from the compiled output in a way that's appropriate for the
output target.

### Anchors

An object is a value. An anchor is a named structural declaration that can
participate in inheritance, typing, exports, and language-level composition.

In Piton, the `anchor` is the core construct, the building block of the entire
system. It shares some loose heritage with classes in OOP (instantiation not
being one of them), but does have its own unique characteristics as well.

```piton
anchor MyFirstAnchor:
  whatIsAnAnchor:
    An anchor is a kind of object or document that is structured via properties
    and values.

  stringValue: String types are supported.

  listTypes:
    - List types
    - are
    - supported.

  nestedLists:
    - Nested list types are
      - also supported.
      - [And, With, Bracket, Syntax]

  numberTypes: 3.14 - 3.14

  booleanTypes: true

  nullType: null

  // This evaluates to false
  booleanOperatorsAnd: {this.booleanTypes} && false
  // This evaluates to true
  booleanOperatorsOr: {this.booleanTypes} || false

  thisKeyword:
      As you may have noticed, Piton supports the `this` keyword for accessing
      properties on the current anchor. We'll talk about this later.
```

If we look at how this would compile to JSON, it would look like this:

```json
{
  "whatIsAnAnchor": "An anchor is a kind of object or document that is structured via properties and values.",
  "stringValue": "String types are supported.",
  "listTypes": ["List types", "are", "supported."],
  "nestedLists": [
    "List types are",
    ["also supported.", [["And", "With", "Bracket", "Syntax"]]]
  ],
  "numberTypes": 0,
  "booleanTypes": true,
  "nullType": null,
  "booleanOperatorsAnd": false,
  "booleanOperatorsOr": true,
  "thisKeyword": "As you may have noticed, Piton supports the `this` keyword for accessing properties on the current anchor. We'll talk about this later."
}
```

> Even as we're about to dive into the syntax of the language, it's worth noting
> that the nested list example is a bit contrived just to show the possibility
> of inline list syntax and how it pairs with line-based lists. That's why the
> deepest level of nesting in the JSON output is actually a double array.
> Normally, the inline array syntax would not be used inside of a list.

### Structural Inheretence

Piton uses structural inheritance, not polymorphism. Support for inheritance
including multiple bases is accessed via the `extends` keyword.

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

One particular thing to note here is that both `FirstBaseAnchor` and
`SecondBaseAnchor` include a `description` property, and the way that got
inherited by `ChildAnchor` is a simple left to right where the last in line
wins.

Type constraints participate in the same left-to-right collision resolution as
property values; the right-most inherited definition wins.

This brings up the question of what happens when a child anchor declares a
property that is inherited from the base anchor, and how do we retrieve values
from the base?

```piton
anchor BaseAnchor:
    description: Description from BaseAnchor

anchor ChildAnchor extends BaseAnchor:
    description: Description from ChildAnchor
```

This of course will simply use the child anchor's value.

```json
{
  "description": "Description from ChildAnchor"
}
```

But if we wanted to specifically pull from the parent, that is possible using
the `super` keyword.

```piton
anchor ChildAnchor extends BaseAnchor:
    description:
        ${super.description} and Description from ChildAnchor
```

As with the previous example of multiple inheritance, the `anchor` could be
inhereting from multiple bases, in which case what does `super` point to? It
follows the same authority as property inheritance in that left to right, last
in line wins.

It's worth noting the small detail here that we borrow the `${}` syntax from
other languages for string interpolation.

So the resulting JSON would be:

```json
{
  "description": "Description from BaseAnchor and Description from ChildAnchor"
}
```

`super` on lists gets some extra attention that's worth noting.

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

This example with yield a final `items` of `["A", "B", "C", "D", "E", "F"]`.
We're applying the `+` concatenation operator here to indicate spreading the
`super.items` list into the current list. Without that we'd end up with `[["A",
"B", "C"], "D", "E", "F"]`.

But let's modify that example slightly to see a specific feature of the `+`
concatenation operator.

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

The change is that `ChildAnchor` now also contains `A`, `B`, `C`, and `D`, and
we're also bring `super.items` in at the end of the list. If you remember from
when we discussed the `+` concatenation operator, it performs deduplication and
concatenates in the order of the operands. So `ChildAnchor` ends up with `["D",
"A", "B", "C"]`.

One last example uses the `++` operator.

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

Exactly the same as the `+` operator but no deduplication. So we end up with
`["A", "B", "C", "D", "A", "B", "C"]`.

### Self-reference

Piton supports self-reference within anchors via both the `self` and `this`
keywords, and there's an important distinction between the two. Consider the
following example:

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

This is reasonable and expected behaviour, but I've often found when building
inheritance hierarchies that I want a bit more control. That's where `this`
comes in. Where `self` will reference to the most descendant anchor, `this` will
reference to the exact anchor. So in the above example, imagine we change `self`
to `this`:

```piton
anchor Base:
    name: Base Anchor
    baseDescription: This is ${this.name}
```

While the rest of the example remains the same. The JSON output will now be:

```json
{
  "name": "Child Anchor",
  "baseDescription": "This is Base Anchor",
  "childDescription": "This is Child Anchor"
}
```

And just for absolute clarity, let's take this example one step further:

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

Will output:

```json
{
  "name": "FinalChild Anchor",
  "baseDescription": "Override on baseDescription MidChild Anchor",
  "childDescription": "This is FinalChild Anchor"
}
```

As you can see, `this` gets pinned to wherever it's used, while `self` travels
through the hierarchy.

### Abstracts Anchors

Abstracts allow us to define the shape of an anchor without providing values.  
An abstract anchor alone will never compile; it must be extended by a non-
abstract anchor, and that non-abstract anchor must implement all undefined
abstract properties.

```piton
abstract anchor Skill:
  description:: string


anchor ConcreteSkill extends Skill:
  description: This must be a string as defined by the abstract
```

We'll introduce a new bit of terminology here in that a concrete anchor that
extends an abstract anchor is said to be "implementing" the abstract anchor.

A concrete anchor can only extend a single abstract anchor. It is in fact good
practice to export an abstract anchor as a keyword to enforce this
ergonomically.

In the case of multiple inheritance on abstracts, type constraint conflicts will
throw a compiler error.

#### Special Type Constraints

We are also able to use the specialized type constraints within abstracts like
`simple`, `complex`, `any`, `[]`, etc.

However, only within abstract anchor definitions, we introduce an additional
type constraint keyword `extends` that allows us to represent inheritance
hierarchy. Take the following example.

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

here, we have defined an anchor `A` with a `description` and an anchor `B` which
extends `A` and adds a `name` property. So `B` has both `description` and `name`
while `A` only has a `description`.

We have then defined an anchor `C`. Let's go one property at a time.

`listOfA` will be satisfied by anything that directly implements the abstract
`A`.

`listOfB` will be satisfied by anything that directly implements the abstract
`B`.

Where it gets a little more interesting is in the third property,
`listOfExtendsA`.

Within abstract anchor definitions, `extends` on an anchor type reference means
that the constraint may be satisfied by any concrete anchor whose inheritance
chain includes that anchor.

So in this example, that constraint is satisfied by a list of anything that has
`A` in its inheritance hierarchy. So in this case, you'd be able to pass
concrete anchors that extend either `A` or `B` given that `B` has `A` in its
inheritance chain.

### User-defined Keywords

Now with abstracts defined, we can discuss user-defined keywords. This is really
just syntactic sugar equivalent to `extends` that exists to pomote clarity of
code design in helping to emphasize certain base anchors as fundamental units.

And it's very simple to do:

```piton
anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

my-anchor ChildAnchor:
    description:
        + {super.description}
        My additional description
```

That is exactly equivalent to using `extends`:

```piton
anchor ChildAnchor extends MyAnchor:
    description:
        + {super.description}
        My additional description
```

By its nature, a user-defined keyword does not allow for multiple inheritance
chains, however you can still use `extends` to achieve the same effect.

```piton
anchor OtherBase:
    description: Other Base

my-anchor ChildAnchor extends OtherBase:
    description:
        + {super.description}
        My additional description
```

This would be equivalent to:

```piton
anchor ChildAnchor extends MyAnchor, OtherBase:
    description:
        + {super.description}
        My additional description
```

Note that the user-defined keyword will be the first anchor in the inheritance
chain, and so be overruled during collisions by anything further right in the
chain.

### Import, Export, Modules, and Use

Fundamental to Piton is the ability to compose a larger codebase from smaller
focused pieces, so we need a way to reuse code across files.

#### Import/Export

To make something defined within a file available to other files, you use
the `export` keyword. For example:

```piton
export pi: 3.14
myVariable: 42
export anchor MyAnchor:
    description: This is my anchor
```

To bring those into another file you use the `from...import` syntax:

```piton
from ./FirstFile import pi, MyAnchor
```

Note that we can't import `myVariable` because it wasn't exported.

Paths for `from...import` are relative to the current file. If the project is
configured with a `piton.config.pi` file, you can also use absolute path imports
relative to the `root` value defined in the project config.

```piton
from /subdir/subdir/file import AnAnchor
```

It's also possible to import under an alias by placing the alias after the
imported symbol:

```piton
from ./FirstFile import pi SliceOf, MyAnchor MyAliasedAnchor
```

Worth clarifying that `pi` will be imported as and only as `SliceOf` (i.e. `pi`
will not be available in scope).

#### Use

When you want to use a user-defined keyword, you'll need to apply the `use`
keyword. It brings into scope of the current file any exported user-defined
keywords.

```piton
use ./CustomKeywords

my-custom-keyword Wow:
    description: amazing
```

`use` only brings in keywords. It does not imports anything else that was
exported, just as `from...import` does not import keywords.

#### Modules

In a complex project, it's likely that you'll end up with hundreds of files that
all work together, often grouped by domain or structured for reusability. Every
complex layer or a codebase should be as self-contained as possible, requiring
minimal inputs and outputs, and that even extends to imports.

Consider a situation where you're importing twenty anchors from one directory.
That's a lot of boilerplate, and it's something likely to be repeated every time
use want to import that functionality.

To solve for this annoyance piton supports folders-as-modules with an `index.pi`
file. When an `index.pi` file is present in a directory, you can now import
anything exported from that file simply by pointing to the directory.

```
myCurrentFile.pi

path/
  to/
    directory/
      index.pi
```

And in `myCurrentFile.pi` you could have:

```piton
from ./path/to/directory import MyAnchor
```

There are several ways to build an `index.pi` file.

```piton
from ./MyAnchor import MyAnchor
export MyAnchor
```

That functions, but it's a bit verbose. There is a modification of the
`from...import` syntax that allows you to be a bit for concise:

```piton
from ./MyAnchor export MyAnchor
```

And a slight modification of that that's even more concise:

```piton
from ./MyAnchor export *
```

`from...export` also supports renaming exports:

```piton
from ./MyAnchor export MyAnchor MyAliasedAnchor, MyOtherAnchor MyOtherAliasedAnchor
```

#### Linebreaks on Imports/Exports

Linebreaks are allowed on imports/exports:

```piton
from ./file import
    FirstThing,
    SecondThing,
    ThirdThing
```

`piton format` will automatically add linebreaks to imports/exports if there are
more than 2 items or if the lien exceeds 80 columns, and it will sort the
imports.

## CLI Compiler

The CLI compiler is a command-line tool that allows you to compile Piton files
into output. It also includes helper commands like `format`.

| Command             | Description                                                            | Inputs             |
| ------------------- | ---------------------------------------------------------------------- | ------------------ |
| `piton compile`     | Compiles a Piton file into output                                      | Piton file or glob |
| `piton check`       | Checks a Piton file for errors                                         | Piton file or glob |
| `piton build`       | Builds a Piton project as configured by the piton.config.pi            | none               |
| `piton build check` | Builds and checks a Piton project as configured by the piton.config.pi | none               |
| `piton format`      | Formats a Piton file                                                   | Piton file or glob |

### Project Config

If the compiler finds a `piton.config.pi` file in the current working directory,
it will read that configuration file to configure a project. Realitically, this
is how any Piton project will usually be used. Through a project you can
configure entry points, output directories, roots, and other build options.

Configuration anchors are provided by the `@piton/config` `use`/`import` which
is bundled into the compiler:

```piton
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi // This is optional and defaults to the root

    frameworks:
        - {BelayFrameworkConfig}

belay-config BelayFrameworkConfig:
    codeRoot: ./src/
    adapters:
        - {ClaudeAdapter}
```

## Frameworks

Frameworks are a construct within a piton project that allow the inclusion of
piton modules globally as well as the extension of what the compiler outputs
through adapters. We have already discussed, for example, how string
interpolation can be given nuance through extensible sigils, and specifically
how that can be relevant for JSON vs. agentic output.

As of the current version of Piton there is one framework bundled with the
language: the Belay framework. In future iterations of the language, we will
build out much more functionality in the frameworks concept.

Frameworks are included in a project through the `frameworks` property in the
project config. You'll see a concrete example of this when we talk about the
Belay framework. Once you've included a framework, you'll be able to `use` any
keywords and `import` any modules it exports. You'll still have to `use` and
`import` in each file, but adding the framework to the project config makes
those pieces available.

## The Belay Framework

The Belay framework is bundled with Piton, and is the specific framework that
enhances the language in such a way that it can build agentic skills, commands,
instructions, and agents. And it exports exactly those four new anchors and
exposes them as keywords.

| Anchor      | Keyword       |
| ----------- | ------------- |
| Agent       | `agent`       |
| Instruction | `instruction` |
| Skill       | `skill`       |
| Command     | `command`     |

### Project Configuration

In order to use the Belay framework you must include it in the project config.
We already showed this in the project config example, but now we'll highlight
the Belay-specific items and fill it out with more detail.

```piton
use @piton/belay

export piton-config Config:
    // Resolve the anchor that implements belay-config
    frameworks:
        - {BelayFrameworkConfig}

// Implement belay-config with required configuration
belay-config BelayFrameworkConfig:
    // Where the source code of your application lives
    codeRoot: ./src/
    shapeRoot: ./spec/shape/

    // Adapters are how the compiler knows what to output and where. In this
    // case, we're outputting agentic coding files.
    adapters:
        - {AgentAdapter}

// The agent adapter knows how to output Markdown for all of Belay's constructs
// that will work with agentic coding tools like OpenCode, Claude Code, etc.
belay-agent-adapter AgentAdapter:
    claude: true
```

### Agent

Belay exports the `agent` keyword and this is used to define agents.

The shape of an agent is:

```piton
abstract anchor Agent as agent:
    description:: string
    role:: string
    prompt:: string
```

And it will output into the correct agent directory for agents in the shape of:

```markdown
---
name: { anchor name in kebab-case }
description: { description }
tools: if provided in the agent anchor, tools will list here
model: if provided in the agent anchor, the model list here
---

You are a {role}

{prompt}
{string serialized version of the agent}
```

### Skill

```piton
abstract anchor Skill as skill:
    description:: string
    prompt:: string
    useWhen:: string
```

And it will output into the correct agent directory for skills in the shape
of:

```markdown
---
name: { anchor name }
description: { description } Use when { useWhen }
---

{prompt}
{string serialized version of the skill}
```

### Command

```piton
abstract anchor Command as command:
    description:: string
    prompt:: string
```

And it will output into the correct agent directory for commands in the shape
of:

```markdown
---
description: { description }
allowed-tools: If provided, list
model: If provided, list
---

{prompt}
{string serialized version of the command}
```

All commands with be prefixed with `x-`

### Instruction

Instructions are a special one. They look like:

```piton
abstract anchor Instruction as instruction:
    description:: string
    prompt:: string
```

The purpose of an instruction is primarily to be output into the correct
`codeRoot` directory as a `CLAUDE.md` that `@imports` an `AGENTS.md` file. In
Belay, we have a concept of a `codeRoot` directory, as well as a `shapeRoot`
directory; `shapeRoot` is an approximate mirror of the generated `codeRoot`
structure. So for example, if you have a `codeRoot` that looks like:

```
src/
  components/
    button/
    input/
```

The expectation is that your `shapeRoot` will mirror this structure:

```
spec/
  shape/
    components/
      button/
        Button.pi
      input/
        Input.pi
        InputDesign.pi
```

When compiled, the `Button.pi` file will be transformed into
`src/components/button/CLAUDE.md` while the `Input.pi` and `InputDesign.pi` file
will be transformed into `src/components/input/CLAUDE.md`. The specific note
there is that files at the same scope will be concatenated into the same output
file. Structurally this fine because it's all just markdown and prose.

If the `codeRoot` structure does not match the `shapeRoot` structure, the
instructions will be concatenated to the next level up that does match, ended at
a `codeRoot/AGENTS.md` and `codeRoot/CLAUDE.md` file.

In addition to this, all instructions contained under `shapeRoot` will be
compiled into the agent directory (`.claude` for example) under the
`reference/shape` directory maintaining their relative structure.

### Special **BELAY_SHAPE** Variable

Belay exports a special `__BELAY_SHAPE__` variable that contains the compiled
shape root directory -- what we previously referenced in an example as
`.claude/reference/shape`.

We can import that to write specific skills:

```piton
use @piton/belay

from @piton/belay import ___BELAY_SHAPE___

// Incomplete and very basic example
export skill BuildSpec:
    prompt:
        Build following the structure outlined in {__BELAY_SHAPE__}.
```

### String Serialization and Special Reference Sigil

Given that the output of Belay is Markdown, ultimately everything will need to
be serialized into a string during compilation.

Simple types serialize directly as strings. `false` becomes "false", `42`
becomes "42" etc.

Complex types take a bit more work.

Lists will be formatted as:

```
- List
- of
- Items
```

A "pure" dictionary, in that it's a nested series of key/value pairs, will be
serialized like with indentation matching structure:

```
firstProperty:
  secondProperty:
    thirdProperty: value
```

Anchor properties will be serialized as headers according to their level of
depth.

However in the more complex case of an implicit list, it will be serialized as
headers matching hierarchy level, with exceeded depth of six levels simply made
to be **bold**. Property names will be split and expanded into words;
`myProperty`, becomes `My Property`.

```
# First Property

## Second Property

### Third Property

String item

- List
- of
- items

and:
  even:
    nested: objects
```

As you can see in that example, nested objects inside an implicit list will be
serialized as Markdown indentation-based objects.

`${}` string interpolation will take the value of the simple type, while it will
take the compiled name of the complex type.

There's a special reference sigil for string interpolation `@{}` that will
inform the agent to go reference the compiled file of what's being referenced.

### Anchors

Any referenced anchors that are reached will be serialized according to the
serialization rules into the `<.agent>/references/` directory mirroring their
original relative directory structure.

### Reachability

Only files that are reachable from the entrypoints will be compiled.
