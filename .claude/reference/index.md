# Specification

## Founding Thesis

### Introduction

#### Description

Piton is a programming language designed specifically for writing skills, agents, and reference documents for Agentic Software Development. To call it a programming language is a bit generous, but well, it’s not exactly natural language either. Semi-natural programming language? A SNPL? Anyway…
The core thesis is that any programming language has always been an abstraction on top of the layer beneath it, that once a new abstraction is generally accepted we no longer care about how the compiled artifacts are generated as long as they’re “good”, and that agentic development is just another evolution in that history of abstracted apathy.
I would also suggest that what humans are good at has never been programming itself, but rather designing coherent (maybe) functioning (hopefully) systems of little doodads that twirl and whir and click and clack together in an elegant (occasionally) way. Let’s define “traditional programming” as the likes of Assembler, C, Python, or those handful of awkward years where we thought delivering precious stones by way of a train was the best way to stop drinking coffee. Traditional programming is a very clever mechanism in that it allows us to describe, constrain, and spend time thinking about the systems we’re building while also building the artifactual doodads – or at least the highly detailed instructions required for the compiler to build them.
In approaching Agentic Development, my hypothesis is that we need a way to do exactly the same describing, constraining, and thinking that we are and will remain skilled at, while letting go of the manual labour that is writing what we collectively determined to be the latest acceptable abstraction sitting on top of the little sparks zipping around controlling a computer.
And with that, I posit that you should be able to copy and paste the entirety of this essay into an agentic coding tool and it will output a compiler for the Piton language and Belay framework.

### Cnc Comparison

#### Description

I realize I’ve probably already driven home my point and tucked it nicely into bed. However, there’s one particular example I’ve found myself thinking through regularly, the structure of which I think functions well enough that it can imbue my thesis with a bit of sound nuance.
My dad is an aerospace engineer, and he was very much present for the transition from traditional machine operation to CNC (Computer Numerical Control) machining. He has described many of the same “those damn robots” attitudes we’re seeing now as our industry is facing what the ambulance-chasing among us would call “extinction.” Pish posh, says the piscine aristocracy.
Let’s discuss milling as an example; a spinning cutter moves through the material being machined tiny little bit by tiny little bit, gradually removing material until a big bland block of aluminum turns into a smaller piece of aluminum shaped possibly less like a block. In traditional machining, the hand of a machinist turns dials to move the cutting point along the X, Y, and Z axes. Involved in this skill is a combination of math, materials science, dexterity, and the tacit understanding of how operations literally sound and feel when they’re doing well or not so well.
CNC machining replaced the hand. With CNC, a most-uninteresting robot composed of three motors, one each attached to the X, Y, and Z dials does the turning. Those motors are controlled by a simple programming language called G-code. And let’s be clear, even BASIC calls G-code basic as it looks it up and down.
```
G1 X40 Y0 F300
G1 X40 Y30
G1 X0 Y30
G1 X0 Y0
```
G1 means linear movement. Move in a straight line from where you are to where I tell you.
X40 means X coordinate of 40 units. Y0 is Y coordinate of 0 units.
F300 means “feed rate” of 300 units per unit of time. AKA, how quickly we move.
So, yes. We made a rectangle in two dimensions. And bear in mind, CNC milling is usually only useful in three dimensions – or at minimum what the industry likes to call 2.5D Machining (they misplaced the other half-dimension kind of like they misplaced their tape measure).
It’s a sort of turtle graphics approach to controlling the robot. And while of course there’s more depth to the language than what I’ve shown here, even this simple example demonstrates why the abstraction of G-code itself became a lower-level substrate for another abstraction. Turtle graphics all the way down.
But what CNC didn’t and doesn’t replace is the majority skill of the machinist. Perhaps their specific kind of dexterity becomes less relevant, and certainly the “literal feel” of the machine operation meets a dead end at the closed-off entrance of a robot’s emotionally damaged heart. But the rest stays, if not also given the space to gain precision.
I hinted at the problem already. I also gave imprecise clues to it. It’s repetition; if what we did is cut a rectangle, we’ll need to repeat that again and again in the small increments the machine, cutter, and material are able to physically handle as we move deeper into the billet to achieve the desired depth of cut. And that’s just for an extruded rectangle. Imagine what the code might look like for carving out a complex three-dimensional curved surface.
There’s nothing stopping you from manually writing G-code that complex, just like there’s nothing stopping you from building a banking system in Brainfuck or building an editor-based operating system in LISP. But why would you do that – to yourself or us?
And that’s where CAM (Computer-Aided Manufacturing) comes in. CAM combines the input of the engineer’s 3D model (the design intent) with the reformed machinist’s expertise in order to output G-code. The CAM engineer may never touch the machine directly, nor may they ever type a single line of G-code, but their involvement and knowledge are critical to a functional outcome.
We could also acknowledge that what was once two jobs (engineer and machinist) is now three jobs (engineer, machinist, and CAM engineer). And I’m doing that by stating it, but I’ll leave it at that. Otherwise I might lose the plot of this essay, kinda like – dammit, where’s my 10mm socket.
While I’ve found the CNC comparison a useful framework through which to organize my thoughts about recent changes in software development, it also serves as a literal and pragmatic slice of history that’s far enough back for the outcomes to be well-established while also close enough to feel immediately comparable.

### Agent As Compiler

#### Description

When I worked at a consulting agency, the regular joke among developers was that 95% of software consulting work is rebuilding an Excel spreadsheet that outgrew itself into code. Hyperbolic? Yes. Untrue? Not so much. And of course if you’re the solve-all-the-problems type you end up spending cycles on “but what if we could build a software that was powerful like code, but also simple like a spreadsheet?” And you end up designing a spreadsheet.
The reason so much of software development feels the same is because, quite simply, a lot of it is. How many CRUD REST APIs have you built, connected to a database, implemented a job queue? Please make it stop. How many times have you written an application layout in HTML/CSS > Bootstrap > Backbone > Sass > Angular > React – please make it stop. How many times have you typed NavBar? How many times have you abstracted a list of items with create, edit, delete – a form that pops up in a modal or drawer. Oh wait, that modal needs a curtain. Please, Make. It. Stop.
We, the collective developer blob, have continually invented, reinvented, and adapted sometimes brilliant and sometimes less brilliant mechanisms to abstract the very literal step-by-step instructions of G-code, Assembly, or even BASIC into more conceptual shapes. Functions, classes, interfaces, generics – none of these are the actual problem; they are beautiful concepts wrapped around the process of solving the problem once the problem becomes a pattern.
And please, do not misunderstand me for even a moment; it has been one of the tremendous joys of my life to have the honour of learning, understanding, and building with these concepts that so many brilliant people have created over the years. And perhaps that’s even part of my larger point; I don’t want to lose that joy by simply prompting an agent “MAKE IT MORE” – I want to use those concepts to focus on building the systems they support so well.
But perhaps I never wanted to write code itself; I wanted to create.

### The Problem

#### Description

As potent as the wow-factor is when you prompt an agent with a minimal description of what you want and it builds it, that process has left me feeling like a lot is missing – not strictly from the result, but more from my involvement in the process and my ability to precisely control the outcome. “Great job, brother Claude. Now make X work like Y.” It does, but also ends up making C into a Q. So you prompt again, and prompt again, again. Eventually you’re almost certain to arrive on something close enough to correct, and sometimes this is absolutely fine. If you’re building a small, one-off tool then this is nothing short of incredible.
But, I would argue that for a sufficiently complex system, something more robust and traceable is needed, something that is an artifact of the process itself, not just the output of the process. I believe this becomes even more relevant as more people work on an agentic codebase; the prompts that led to an outcome live within the session, and at best are a faint memory in the one mind of the one developer. Despite how it may seem, neither agents nor humans can read minds.
Many of the agentic platforms provide mechanisms like AGENTS.md and CLAUDE.md that provide a level of instruction to the agent. Further still, more granular skills and agents are written in Markdown and committed to the repository. Now you can write durable instructions that the agents will reference when building the code.
The problem I started to encounter is that in the aim of describing interconnected systems, pure prose is inefficient and prone to decay. Any author is likely to inadvertently describe subsystems and instructions with phrasing that is at best ambiguous enough to cause conflict. With traditional programming, we have all these wonderful tools to promote reuse and avoid duplication. Markdown doesn’t have these tools.
At the same time, pure prose is an incredible way to say
```
“the button should be blue with contrasting text.”
```
Compare that to
```css
.button {
  background-color: #00a;
  color: #fff;
}
```
Which one would you rather write?
It’s this combination of structure and prose both as first-class citizens that Piton attempts to solve. So with that, let’s take a look at Piton.

## Language

### Overview

#### Introduction

##### Introduction

The Piton language, while primarily intended for writing Agentic constructs, is also at its core a declarative language for defining data. So before we get into discussing the agentic stuff, let's take a look at the language itself through the lens of just data.
There's one thing to make very clear upfront: There is no runtime for Piton. Piton compiles to data -- JSON, YAML, Markdown, etc. -- via renderers. While that ultimately simplifies the mental model, it's important to keep this in mind as you're learning how to use the language.

#### Files And Modules

##### Source Files And Modules

Piton files carry the .pi extension. Directories with an index.pi file become modules, which we cover in [Modules](./scope/language/reuse/Modules.md#modules).

#### Whitespace

##### Whitespace And Structure

The language is a whitespace-based language. Like Python, you can use tabs or an arbitrary number of spaces for indentation. However, you must be consistent throughout the file. And while it's not mandatory, like Python's PEP8, Piton prefers 4 spaces per indent level, not tabs. That's what piton format will apply to your code, and it's not configurable.

#### Comments

##### Comments

Comments are line-only. There are no block comments. Comments are created with "//".
```piton
// This is a comment
myVariable: 42 // This is also a comment
```
`piton format` will always put a space between the "//" and the comment text, so you might as well get used to doing it yourself. It won't touch anything else in the comment though, so commented-out code stays as it is.

#### Keywords

##### Keywords

Keywords are reserved words that have special meaning in Piton. string, false, anchor, export, etc. are all examples. A unique aspect of Piton is that you can define your own keywords that act as a sort of syntactic sugar for inheritance. But that's a topic we'll discuss later in [UserDefinedKeywords](./scope/language/anchors/Keywords.md#user-defined-keywords). Keywords must be all lowercase and can be kebab-case.

##### Reserved

These are all reserved, so you can't use them for your own keywords.
```
anchor abstract export from import use as extends pass
this self super
true false null
any simple complex
string number boolean list dictionary
```
You can still use them as keys though. As keys they're just strings.

##### Pass

Use `pass` when an anchor doesn't have any properties of its own. An anchor always needs something indented under it, so pass fills that spot.
```piton
anchor Base:
    name: Base

anchor Child extends Base:
    pass
```

### Types

#### Overview

##### Overview

Piton is loosely typed with optional type constraints.

##### Types

All the usual suspects are present.

###### String

A unicode string

###### Number

All-encompassing numeric type; int or float.

###### Boolean

true or false

###### Null

The absence of a value

###### List

A list of values. Value types can be mixed as long as it’s not constrained by a type annotation.

###### Dictionary

A dictionary of key-value pairs. Key types must be strings, and value types can be mixed as long as it’s not constrained by a type annotation.

###### Anchor

The core structural building block of Piton. Don't worry, there's a whole section on this.

#### Numbers

##### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

##### Supported Operators

- description: Adds two numbers together
  symbol: +
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Subtracts two numbers
  symbol: -
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Multiplies two numbers together
  symbol: *
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Divides two numbers. Dividing by zero is a compiler error.
  symbol: /
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Returns the remainder of two numbers. The result keeps the sign of the left side, so `-7 % 3` is `-1`. Modulo by zero is a compiler error.
  symbol: %
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=
- description: Less than operator.
  symbol: <
- description: Less than or equal to operator.
  symbol: <=
- description: Greater than operator.
  symbol: >
- description: Greater than or equal to operator.
  symbol: >=

##### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

##### Description

Numbers are written as literal numbers. Leading 0 is mandatory for decimals, and you can use underscores to separate large numbers for readability.
```piton
myInt: 123
myFloat: 3.14
mySmallFloat: 0.14
myBigNumber: 1_200_000.00
```
There is a single number type in all of Piton; type-wise there’s no difference between an int, a float, double, etc.

##### Representation

Under the hood, numbers are 64-bit floats (IEEE 754).
Negative numbers are written with a minus, like `-5`. List items always have a space after the dash, so `-5` is a number and `- 5` is a list item.
There's no exponent notation, so no `1e3`.
When compiled, numbers are written in their shortest form, so `1_200_000.00` becomes `1200000` and `0.50` becomes `0.5`.

#### Strings

##### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

##### Supported Operators

- description: The concatenation operator (`+`) joins two strings together. For example, `{"Hello" + "World"}` gives you `HelloWorld`.
    If only one side is a string, a number, boolean, or null on the other side gets turned into a string first, following the [StringExpression](./scope/language/expressions/StringExpression.md#string-expression) rules. So `{2 + "Hello"}` gives you `2Hello`, and `{"Enabled: " + true}` gives you `Enabled: true`.
    A list, dictionary, or anchor doesn't get turned into a string. You get an implicit list instead, same as putting `{x}` in the middle of some text. So if tags is `[a, b]`, `{"Tags: " + tags}` gives you `["Tags: ", ["a", "b"]]`. If you want the name, use `${tags}`.
  symbol: +
- description: The `++` operator is like merge, but it keeps duplicates.
    On lists it joins them in order and keeps everything, so `[A, B, C, D] ++ [A, B, C]` gives you `[A, B, C, D, A, B, C]`.
    On dictionaries it's a deep merge. If both sides have the same key and both values are dictionaries, those get merged too, all the way down. Otherwise the right side wins.
    On strings it joins them with a line break.
  symbol: ++
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=
- description: Less than operator.
  symbol: <
- description: Less than or equal to operator.
  symbol: <=
- description: Greater than operator.
  symbol: >
- description: Greater than or equal to operator.
  symbol: >=

##### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

##### Description

Strings are not quoted. They can appear on the same line as what they’re assigned to, or indented on the next line:
```piton
sameLine: Hello, World!
nextLine:
    Hello, World!
```
The leading whitespace on a string block is discarded, as that’s part of the syntax of the language, not the string. In other words, the string in both examples is “Hello, World!”.

##### Quotes

Quotes don't mean anything special. They're just characters, so `greeting: "Hello"` is the string `"Hello"`, quotes and all.
The one exception is inside an expression. There, bare words are symbols, so you write strings in double quotes: `{"Hello" + name}`. In that case the quotes aren't part of the string.

##### Line Breaks

Within a string block, you can add line breaks without affecting the structure of the string. To start a new paragraph, you must include a blank line. More than one blank line in a row still counts as one.
```piton
myString:
    This broken string is not considered
    a line break.

    While this *is* a new paragraph because there was a blank line above.
```
When compiled, we’ll get two paragraphs:
```markdown
This broken string is not considered a line break.

While this *is* a new paragraph because there was a blank line above.
```

##### Escaping

###### Description

Escaping works a little differently in Piton than other languages. To escape special characters, you simply wrap them in backslashes, with a space on each side. The spaces are part of the wrapper, so they get removed. So for example \ {1 + 2 + 3} \ would become {1 + 2 + 3}.
That's the only way to escape. Even a single character gets wrapped: \ : \ becomes :.

###### Stacking

You can stack backslashes to escape backslashes themselves. The closing wrapper has to match the opening one, so anything inside can use fewer backslashes:
\\\ \\ \ {1 + 2 + 3} \ \\ \\\ would become \\ \ {1 + 2 + 3} \ \\.

###### Multi Line

Multi-line escape blocks are valid. Put the backslashes on a line by themselves to open the block, and again to close it. Everything in between is kept as is, line breaks and indentation included.
This spec uses three backslashes for these.

##### Code Blocks

Code blocks are not escaped. To Piton they're just text, so anything inside them still gets parsed: expressions, comments, lists, all of it.
So often, you'll want to put an escape block inside the code block. Every example in this spec does that.

#### Booleans

##### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

##### Supported Operators

- description: Logical AND operator
  symbol: &&
- description: Logical OR operator
  symbol: ||
- description: Logical negation operator
  symbol: !
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

##### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

##### Description

Booleans are represented with lowercase `true` and `false`.

#### Null

##### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

##### Supported Operators

- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

##### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

##### Description

Null is represented with lowercase null. Null is a value that means "nothing" So yes, `{null == null}` is true. Piton doesn’t have the concept of undefined or any other "nothing" value.

#### Collections

##### Collections

Piton has two core collection types: lists and dictionaries – or arrays and objects depending on what language you’re coming from.

##### Lists

###### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

###### Supported Operators

- description: The merge operator (`+`) combines two lists or two dictionaries.
    On lists it joins them in order and removes duplicates. If a value shows up more than once, the last one is kept. So `[A, B, C, D] + [A, B, C]` gives you `[D, A, B, C]`.
    On dictionaries it's a shallow merge. You get the keys from both, and if both have the same key, the right side wins.
  symbol: +
- description: The `++` operator is like merge, but it keeps duplicates.
    On lists it joins them in order and keeps everything, so `[A, B, C, D] ++ [A, B, C]` gives you `[A, B, C, D, A, B, C]`.
    On dictionaries it's a deep merge. If both sides have the same key and both values are dictionaries, those get merged too, all the way down. Otherwise the right side wins.
    On strings it joins them with a line break.
  symbol: ++
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

###### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

###### Description

Lists can be defined two ways, Markdown/YAML-style or inline.
```piton
markdownStyle:
    - One
    - List
    - Item
    - Per
    - Line

inlineStyle: [This, is, a, list, of, strings]
```

###### Nested Lists

It’s possible to do nested lists:
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

###### Dictionaries In Lists

Anything indented under a list item isn't part of that item. It becomes the next item in the list. So a dictionary indented under an item is its own item, right after it:
```piton
repos:
    - https://github.com/piton-lang/piton-rs
        tag: v1
    - https://github.com/piton-lang/other
```
```json
{
  "repos": [
    "https://github.com/piton-lang/piton-rs",
    { "tag": "v1" },
    "https://github.com/piton-lang/other"
  ]
}
```
A list item is never a key, even with a colon at the end. `- Settings:` is just the string `Settings:`.
For lists indented under a dictionary key, see the Dictionaries type.

###### Access

Piton intentionally does not provide a way to access items within a list. Because this is not a runtime-based general purpose language but rather a language designed for description, a list is a construct intended for merging via inheritance; myList[0] is not very descriptive, is it?

##### Dictionaries

###### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

###### Supported Operators

- description: Accesses a property of an object.
  symbol: .
- description: The merge operator (`+`) combines two lists or two dictionaries.
    On lists it joins them in order and removes duplicates. If a value shows up more than once, the last one is kept. So `[A, B, C, D] + [A, B, C]` gives you `[D, A, B, C]`.
    On dictionaries it's a shallow merge. You get the keys from both, and if both have the same key, the right side wins.
  symbol: +
- description: The `++` operator is like merge, but it keeps duplicates.
    On lists it joins them in order and keeps everything, so `[A, B, C, D] ++ [A, B, C]` gives you `[A, B, C, D, A, B, C]`.
    On dictionaries it's a deep merge. If both sides have the same key and both values are dictionaries, those get merged too, all the way down. Otherwise the right side wins.
    On strings it joins them with a line break.
  symbol: ++
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

###### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

###### Description

Dictionaries are nested keys and values, and there is only a single way to define them:
```piton
firstLevel:
    secondLevel:
        thirdLevel: This is a string
```
You can use dot syntax to access keys in a dictionary. firstLevel.secondLevel.thirdLevel would yield “This is a string”.

###### Lists In Dictionaries

A list indented under a key is that key's value. If the key also has something above the list, like text or another dictionary, the value becomes an implicit list (see Collections) and the list goes in as a single item:
```piton
plain:
    - x
    - y
mixed:
    text
    - x
    - y
```
```json
{
  "plain": ["x", "y"],
  "mixed": ["text", ["x", "y"]]
}
```
For dictionaries indented under a list item, see the Lists type.

###### Valid Keys

Keys can have Unicode letters, Unicode numbers, underscores, and hyphens. Nothing else, and no spaces.
Keys are always strings, even when they look like something else. So `123`, `false`, and `null` are fine as keys, and they're still just strings when you access them.
So for example, `thisIsAKey` and `123` and `foo-bar` and `false` and `null` are all valid keys, while `This is a key` and `a.b` are not.

###### Hyphenated Keys

A hyphen in the middle of a name is part of the name, so `{config.foo-bar}` reads the key `foo-bar`. If you want subtraction, put spaces around it: `{a - b}`.

##### Combining Collection Types

You **can** combine lists and dictionaries (and other types for that matter):
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
And this scenario is where Piton takes a little liberty in syntax strictness for the sake of human writability and readability. Because we’re mixing types, what happens here is that combined becomes an implicit list that has a string, a list, and a dictionary inside of it. As JSON that would be:
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
We’re sacrificing the otherwise simple rules of syntax here only because this is an inherently intuitive form for a human. We’ll let the compiler do a bit of heavy lifting to make the human’s job nicer.
You'll get the same implicit list if you put a list, dictionary, or anchor in the middle of some text with {}. So See {Foo} for details becomes a list with “See”, a copy of Foo, and “for details”.
One additional gotcha with Piton is that, with an implicit list, dictionary properties declared directly within the mixed block remain addressable; we’re still able to directly reference combined.nestedDictionary.deeplyNestedDictionary and get back “This is a string”. As I said previously, Piton does not allow for list accessors, so trying to access either the string or the list is not possible.
The same is true here:
```piton
combined:
    - First item
        nested: value
```
Since the dictionary is an item in an explicit list, you cannot access that dictionary anymore. I’m just pointing this out because it is syntactically possible, though arguably not a very wise choice in structuring your data.

#### User Defined

##### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

##### Supported Operators

- description: Accesses a property of an object.
  symbol: .
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

##### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

##### Description

There is exactly one user-defined type, and it’s called an anchor. This is something that can be shaped by other types via properties and promotes inheritance. However, it’s a larger and more advanced topic than belongs in this introductory part of the guide, so instead we’ve devoted an entire section to it later on.

##### Valid Keys

Keys can have Unicode letters, Unicode numbers, underscores, and hyphens. Nothing else, and no spaces.
Keys are always strings, even when they look like something else. So `123`, `false`, and `null` are fine as keys, and they're still just strings when you access them.
So for example, `thisIsAKey` and `123` and `foo-bar` and `false` and `null` are all valid keys, while `This is a key` and `a.b` are not.

#### Inference

##### Description

Unless specifically constrained to a type, a variable or property can hold any type. The type is inferred from the value.
```markdown
| Value                   | Inferred As                                                        |
| ----------------------- | ------------------------------------------------------------------ |
| `true`                  | `boolean`                                                          |
| `true story`            | `string`                                                           |
| `42`                    | `number`                                                           |
| `-42`                   | `number`                                                           |
| `42 things`             | `string`                                                           |
| `A + B`                 | `string` (no braces, so it's just text)                             |
| `{A + B}`               | Depends on what A and B are; see ConcatenationOperators            |
| `This costs $5 + tax`   | `string`                                                           |
| `Total: {a + b}`        | `string` (there's other text around it)                             |
| `null`                  | `null`                                                             |
| `"false"`               | `string` (the quotes are part of it)                               |
| `\ // \ Just Text`      | `string` (the `//` is escaped, so it's not a comment)               |
| `${}`                   | `string`                                                           |
```

#### Reference

##### What Is AType

A type describes what kind of value something is. It defines which values are allowed, which operations they support, and where they can be used.
The compiler uses types to check whether expressions and assignments make sense. Each expression produces a value of a particular type, which may be explicitly declared or inferred by the compiler. When a value is used where its type is not allowed, the compiler reports a type error.

##### Supported Operators

- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=

##### Unsupported Operators

Any operator not explicitly listed as supported will throw a compiler error.  Before throwing an error, the compiler will follow the rules of [TypeCoercion](./scope/language/variables/TypeCoercion.md#type-coercion) and [Inference](./scope/language/types/Inference.md#inference).

##### Description

A reference comes from a [ReferenceExpression](./scope/language/expressions/ReferenceExpression.md#reference-expression). It points at an anchor, or a property on one, instead of copying it. How it ends up looking in the output is up to the renderer, and frameworks like Belay build on top of that.

##### Renderers

```
json: By default a reference becomes a string: the path to the other file (relative to this one), a colon, and the dot path to the value. So `../file.json:Anchor.property`.
  If there's no dot path to it, you get whatever the property turned into instead, like `../file.txt:Something`. And if that doesn't work either, you get the line number, like `../file.json:42`.
  You can change this with a renderer option.
yaml: Same as json, with the yaml output file.
markdown: A relative Markdown link to wherever the anchor was rendered, with the anchor's name as the link text. So a reference to Button from a file next to it links Button to ./Button.md#button. A reference to a property links Button.color to ./Button.md#color.
```

##### Equality

Two references are equal if they point at the same thing.

#### Simple Types

##### List Of Simple Types

- [Numbers](./scope/language/types/Numbers.md#numbers)
- [Strings](./scope/language/types/Strings.md#strings)
- [Booleans](./scope/language/types/Booleans.md#booleans)
- [Null](./scope/language/types/Null.md#null)

#### Complex Types

##### List Of Complex Types

- [Lists](./scope/language/types/Lists.md#lists)
- [Dictionaries](./scope/language/types/Dictionaries.md#dictionaries)
- [AnchorType](./scope/language/types/Anchors.md#anchor-type)

### Variables

#### Overview

##### Overview

Variables are declared simply by naming them at the top level of a Piton file. You use : to assign a value to a variable.
```piton
myVariable: 42
```

#### Type Constraints

##### Type Constraints

You can add type constraints to variables using ::
```piton
myVariable:: number: 42
```
Note that the whitespace after both :: and : is important. myVariable::number:42 would cause a compiler error. Why? Because including the space makes it more readable.

##### Multiple Type Constraints

It’s also possible to add multiple type constraints to a variable using an additional ::
```piton
myVariable:: number:: string: 42
```
This will allow myVariable to be either a number or a string, and the order of preference is constraint fulfillment left to right. In the above example, myVariable will be a number since 42 can be evaluated as a number.
However, if you flip it around:
```piton
myVariable:: string:: number: 42
```
myVariable will be a string since 42 can also be a string and string is evaluated first and wins. One final example:
```piton
myVariable:: boolean:: number:: string: "false"
```
The quotes are part of the value, so it isn't a boolean or a number. myVariable is the string `"false"`, quotes and all.

##### Required And Optional

In an abstract anchor, if a property has a type constraint but no value, it's required. Anything that extends the anchor has to give it a value. Adding null to the constraint means that value can be null, but you still have to write it.
If it has a default value, it's optional. The usual way to write an optional property is to allow null and default to null:
```piton
abstract anchor Card:
    title:: string
    subtitle:: string:: null
    footer:: string:: null: null

anchor MyCard extends Card:
    title: Hello
    subtitle: null
```
Here title and subtitle are required (subtitle can be null), and footer is optional. The language server won't complain about a missing optional property.

##### Empty Values

A property has to have a value. A name with nothing after it and nothing indented under it is a compiler error. If you mean nothing, write null.
The only exception is a required property in an abstract anchor, like title above. Anywhere else, a type constraint with no value is still a compiler error.

#### List Type Constraints

##### List Type Constraints

List type constraints are possible with the [] syntax.
```piton
myVariable:: string[]: [foo, bar, baz]
```
`myVariable` must contain a list of strings.

#### Nested Type Constraints

##### Nested Type Constraints

It is possible to constrain nested keys inside a dictionary.
```piton
myVariable:: dictionary:
    a:: number: 1
    b:: string: foo
```

#### Special Type Constraints

##### Special Type Constraints

There are three additional type annotations that we can use when we want to deal with slightly more fuzzy conditions. These are any, simple, and complex.
The any constraint will allow any type, simple, complex, number, string, boolean, etc.
The simple type will allow any simple type, which we’ve previously defined, but includes string, number, boolean, and null. It specifically avoids lists, dictionaries, and anchors.
The complex type allows for any complex type, in other words, list, dictionary, and anchor.
There are two more you can use with any type. Add [] to the end for a list of that type, like `string[]`. And put extends in front of an anchor type to accept anything that inherits from it, like `extends Operator[]`. We'll cover that one with abstract anchors.
```piton
a:: complex: [1, 2, 3]
b:: complex:
    a:: number: 1
    b:: number: 2
c:: simple: 1
d:: simple: Hello, World
e:: simple: false
f:: any: 1
g:: any: Hello, World
h:: any:
    - String
    - [1, 2, 3]
```

#### Type Coercion

##### Description

Type coercion in Piton is intentionally limited. When a value is constrained to multiple types, Piton evaluates the constraints from left to right and uses the first type the source value can validly represent.
Numbers can be coerced into strings. For example:
```piton
value:: string:: number: 42
```
This evaluates to the string “42” because string is the first compatible constraint.
Coercing a literal keeps it exactly as written. An expression gets evaluated first, so its result is what gets coerced:
```piton
written:: string: 1.0
evaluated:: string: {1.0}
```
written is the string “1.0”. evaluated is the string “1”, because {1.0} is the number 1 before it becomes a string.
Booleans and null are never coerced. So this is a compiler error, not the string “false”:
```piton fragment
flag:: string: false
```
If you want the string, use `${false}`.
Quotes don't do anything special, so a quoted value is just a string with quotes in it. `value:: boolean:: string: "false"` is the string `"false"`, quotes and all, because `"false"` isn't a boolean.
Likewise, lists, dictionaries, and anchors are never coerced into unrelated types.
If none of the declared constraints can accept the value it is a compiler error.
This is all about type constraints. Operators like + have their own rules.

#### Mutability

##### Mutability

Given that there is no runtime for Piton, all variables are immutable. If we introduce compile-time mutability then I think we’re just asking for chaos.

#### Scope

##### Scope

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

### Operators

#### Control Flow

##### Control Flow

There are no conditionals or loops in Piton with the exception of the [TernaryOperator](./scope/language/operators/conditional/TernaryOperator.md#ternary-operator)
There are also no functions.

#### Standard Operators

Piton supports a pretty standard if not small set of operators, plus a few slightly more unique ones. Let’s start with the standard ones.

#### Access Operators

##### Operators

- description: Accesses a property of an object.
  symbol: .

#### Arithmetic Operators

##### Description

They do math things when operating on numbers.

##### Operators

- description: Adds two numbers together
  symbol: +
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Subtracts two numbers
  symbol: -
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Multiplies two numbers together
  symbol: *
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Divides two numbers. Dividing by zero is a compiler error.
  symbol: /
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Returns the remainder of two numbers. The result keeps the sign of the left side, so `-7 % 3` is `-1`. Modulo by zero is a compiler error.
  symbol: %
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.

##### Order Of Precedence

ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.

#### Comparison Operators

##### Description

Compares two values and gives you a boolean.

##### Equality

`==` and `!=` work on every type. Lists and dictionaries are equal if everything inside them is equal. Anchors are only equal if they're the same anchor, and references are only equal if they point at the same thing.

##### Ordering

`<`, `<=`, `>`, and `>=` work on numbers and strings. Strings are compared by Unicode code point. Anything else, or a number against a string, is a compiler error.

##### Operators

- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=
- description: Less than operator.
  symbol: <
- description: Less than or equal to operator.
  symbol: <=
- description: Greater than operator.
  symbol: >
- description: Greater than or equal to operator.
  symbol: >=

#### Concatenation Operators

##### Description

Operators that join strings, lists, and dictionaries.

##### Operators

- description: The concatenation operator (`+`) joins two strings together. For example, `{"Hello" + "World"}` gives you `HelloWorld`.
    If only one side is a string, a number, boolean, or null on the other side gets turned into a string first, following the [StringExpression](./scope/language/expressions/StringExpression.md#string-expression) rules. So `{2 + "Hello"}` gives you `2Hello`, and `{"Enabled: " + true}` gives you `Enabled: true`.
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

##### Plus Dispatch

The `+` symbol is shared by [AdditionOperator](./scope/language/operators/arithmetic/AdditionOperator.md#addition-operator), [ConcatenationOperator](./scope/language/operators/concatenation/ConcatenationOperator.md#concatenation-operator), and [MergeOperator](./scope/language/operators/concatenation/MergeOperator.md#merge-operator). Which one you get depends on the types on each side.
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

#### Conditional Operators

##### Description

Picks between two values based on a boolean.

##### Operators

- description: The ternary operator selects between two expressions based on a condition, using the syntax `condition ? consequent : alternative`. If the condition is true, the consequent is evaluated and returned; otherwise, the alternative is evaluated and returned. Only the selected expression is evaluated.
    The condition has to be a boolean. There's no truthiness in Piton, so anything else is a compiler error.
    Ternary operators can be chained; a ternary can be in the consequent or alternative slots.
  symbol: `<condition> ? <consequent> : <alternative>`

#### Logical Operators

##### Description

Combine or negate booleans. Anything that isn't a boolean is a compiler error.

##### Operators

- description: Logical AND operator
  symbol: &&
- description: Logical OR operator
  symbol: ||
- description: Logical negation operator
  symbol: !

#### Precedence

Higher rows go first. Operators on the same row go left to right, except the ternary, which goes right to left. Parentheses change the order.
```markdown
| Precedence | Operators          |
| ---------- | ------------------ |
| 1 highest  | `.`                |
| 2          | `!`                |
| 3          | `*` `/` `%`        |
| 4          | `+` `-` `++`       |
| 5          | `<` `<=` `>` `>=`  |
| 6          | `==` `!=`          |
| 7          | `&&`               |
| 8          | `||`               |
| 9 lowest   | `? :`              |
```
So `{1 + 2 == 3 && !false}` is `{((1 + 2) == 3) && (!false)}`, and `{a ? b : c ? d : e}` is `{a ? b : (c ? d : e)}`.

#### All Operators

- description: Accesses a property of an object.
  symbol: .
- description: Adds two numbers together
  symbol: +
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Subtracts two numbers
  symbol: -
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Multiplies two numbers together
  symbol: *
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Divides two numbers. Dividing by zero is a compiler error.
  symbol: /
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Returns the remainder of two numbers. The result keeps the sign of the left side, so `-7 % 3` is `-1`. Modulo by zero is a compiler error.
  symbol: %
  orderOfOperations: ArithmeticOperators precedence follows standard mathematical precedence: MultiplicationOperator, DivisionOperator, and ModuloOperator are evaluated before AdditionOperator and SubtractionOperator.
    Operators with the same precedence are evaluated from left to right. Parentheses may be used to alter the order of operations.
- description: Equality operator.
  symbol: ==
- description: Inequality operator.
  symbol: !=
- description: Less than operator.
  symbol: <
- description: Less than or equal to operator.
  symbol: <=
- description: Greater than operator.
  symbol: >
- description: Greater than or equal to operator.
  symbol: >=
- description: The concatenation operator (`+`) joins two strings together. For example, `{"Hello" + "World"}` gives you `HelloWorld`.
    If only one side is a string, a number, boolean, or null on the other side gets turned into a string first, following the [StringExpression](./scope/language/expressions/StringExpression.md#string-expression) rules. So `{2 + "Hello"}` gives you `2Hello`, and `{"Enabled: " + true}` gives you `Enabled: true`.
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
- description: The ternary operator selects between two expressions based on a condition, using the syntax `condition ? consequent : alternative`. If the condition is true, the consequent is evaluated and returned; otherwise, the alternative is evaluated and returned. Only the selected expression is evaluated.
    The condition has to be a boolean. There's no truthiness in Piton, so anything else is a compiler error.
    Ternary operators can be chained; a ternary can be in the consequent or alternative slots.
  symbol: `<condition> ? <consequent> : <alternative>`
- description: Logical AND operator
  symbol: &&
- description: Logical OR operator
  symbol: ||
- description: Logical negation operator
  symbol: !

### Expressions

#### Description

Expressions combine values and operators to produce a result. Nothing gets evaluated unless it's in braces, like {a + b} or {1 + 2}. Without braces, 1 + 2 is just text. Evaluation follows Piton’s type and operator rules and resolves forward references. Unresolved or cyclic references are compiler errors.

#### Story

A question you might currently have is how can a variable references another variable. We’ve hinted at this in a previous example where we added A and B but now let’s be clear about it.
Easy expression using literals:
```piton
myVariable: {1 + 2}
```
`myVariable` will be evaluated to 3. Evaluation will of course follow all the rules we’ve previously defined about types as operators; {2 + "Hello"} will evaluate to a string 2Hello. Inside braces, bare words are treated as symbols, so strings go in double quotes.
An important thing to note is that an expression must be wrapped in curly braces, otherwise it'll be interpreted as a [Strings](./scope/language/types/Strings.md#strings).
Let’s look at this example:
```piton
a: 1
b: 2
result: a + b
```
`result` will be evaluated to the string "a + b". In order for this to evaluate to 3 you’ll need to use the {} expression syntax:
```piton
a: 1
b: 2

result: {a + b}
```
This makes the job of the compiler much easier, and lets us avoid the problematic situation of string fallback in case a symbol isn’t recognized.
Forward references are fully resolved. Unresolved references are a compiler error. Cyclic references are also a compiler error.
We encounter a probably intuitive but perhaps less obvious scenario when we use [ComplexTypes](./scope/language/types/ComplexTypes.md#complex-types) like [Lists](./scope/language/types/Lists.md#lists), [Dictionaries](./scope/language/types/Dictionaries.md#dictionaries), and [Anchors](./scope/language/anchors/index.md#anchors).
```piton
myList: [1, 2, 3]

myDictionary:
    list: {myList}

newList: {myDictionary.list + [4, 5, 6]}
```
newList will evaluate to [1, 2, 3, 4, 5, 6].
In this case, {myList} resolves to the value of myList, which is then assigned to myDictionary.list. The newList expression resolves myDictionary.list, combines that value with [4, 5, 6], and evaluates to [1, 2, 3, 4, 5, 6].
Anchors get copied in too, but the copy still knows it's Foo. So if x is {Foo}, then {x == Foo} is true, ${x} is “Foo”, and x passes a Foo type constraint. If you want a link instead of a copy, use @{Foo}.

#### Expressions In Text

If the whole value is just an expression, like total: {a + b}, it keeps its type. If there's other text around it, it depends.
${x} turns the result into a string, so the whole thing is just a string.
{x} with a simple value (string, number, boolean, or null) drops the value into the text, so Total: {a + b} is “Total: 3”.
{x} with a list, dictionary, or anchor copies it in. That turns the property into an implicit list, same as when you mix types in a collection. So See {Foo} for details becomes a list with “See”, a copy of Foo, and “for details”.
@{x} puts in a reference, which ends up as a link or a path.

#### Expression Types

- description: Evaluates an expression and returns its intrinsic result, preserving its type without requesting conversion.
  syntax: {}
  evaluation:
    - Evaluate the enclosed expression using normal expression rules.
    - Resolve symbols to their values within the applicable scope.
    - Return the resulting value without additional conversion.
    - Report an error when the expression is invalid or cannot be resolved.
  composition:
    description: The result retains its type when used within an enclosing expression or assigned as a property value.
  output:
    description: Serialize the resulting value according to its type and the selected output format.
    requirements:
      - Preserve native value types in structured output.
      - Serialize an anchor as its resolved content rather than a link to it.
      - Apply the output format's serialization rules when textual output is required.
  inText: If there's other text around it, a simple value gets dropped into the text, and a list, dictionary, or anchor turns the whole thing into an implicit list.
- description: Evaluates an expression and converts its result to a number.
  syntax: #{expression}
  evaluation:
    - Evaluate the enclosed expression using normal expression rules.
    - Return numeric results unchanged.
    - Convert strings containing a valid numeric representation to a number.
    - Report an error when the result cannot be converted to a number.
  conversion:
    - Interpret numeric strings according to Piton's numeric literal rules.
    - Require the entire string to represent a number.
    - Do not interpret a numeric string as an expression.
    - Do not implicitly convert booleans, null, collections, or anchors to numbers.
  composition:
    description: The result is a numeric value that can participate in enclosing expressions, including arithmetic and further conversions.
  output:
    description: Preserve the numeric type in structured output. Convert it to text only when required by the surrounding output format.
- description: Evaluates an expression and produces a reference to its result, preserving the identity of the referenced anchor or property rather than embedding its value or converting it to a string.
  syntax: @{}
  evaluation:
    - Evaluate the enclosed expression using normal expression rules.
    - Require the result to identify an anchor or a property on one.
    - Preserve that anchor's or property's identity until output serialization.
    - Report an error if the expression cannot resolve to an anchor or a property on one.
  compilation:
    - Include the referenced anchor in the compilation dependency graph. For a property, that's the anchor it's on.
    - Resolve its output location through the renderer, or the framework adapter on top of it.
    - Resolve each reference independently for each configured output target.
    - Report an error if the target cannot represent or resolve the reference.
  markdown:
    description: Render a Markdown link to the referenced anchor's compiled representation, relative to the file containing the reference.
    requirements:
      - Use the referenced anchor's name as the link text, or Anchor.property for a property.
      - Link to the specific anchor when several anchors share an output file.
      - For a property, link to its heading.
      - Preserve lazy access rather than automatically including the referenced content.
  otherFormats:
    description: Each renderer decides what a reference looks like. See the Reference type for the defaults. A reference must not silently become an embedded copy or a plain name string when the target has no defined reference representation.
- description: Evaluates an expression and converts its result to a string.
  syntax: ${}
  evaluation:
    - Evaluate the enclosed expression using normal expression rules.
    - Return string results unchanged.
    - Convert other results according to the stringification rules for their type.
    - Report an error when no string representation is defined.
  conversion:
    - Convert numbers to their textual representation.
    - Convert booleans to lowercase true or false.
    - Convert null to the string null.
    - Convert anchors to their name, as written in the source.
    - Convert a named list or dictionary to its name, like Anchor.propertyName, or just the variable name at the top of a file.
    - Report an error for a list or dictionary that has no name.
    - Do not reinterpret the resulting string as source syntax or another expression.
  composition:
    description: The result is a string that can be embedded in surrounding text or participate in an enclosing expression.
  output:
    description: Preserve the string type in structured output. Apply any escaping required by the output format during serialization.

### Anchors

#### Type Reference

[AnchorType](./scope/language/types/Anchors.md#anchor-type)

#### Description

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
        accessing properties on the current anchor. We'll talk about
        this later.
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

##### Structural Inheritance

###### Description

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
Type constraints from the bases work the same way as values; the right-most one wins. Abstracts have their own rules, which we'll get to.
You can't redeclare a constraint you inherit, you can only give it a value. So if subtitle is already constrained, `subtitle: Hi` is fine and `subtitle:: string: Hi` is a compiler error.
An anchor can't extend itself, directly or through other anchors. That's a compiler error.

##### Super

###### Description

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
It’s worth noting the small detail here that we borrow the ${} syntax from other languages for string interpolation.
So the resulting JSON would be:
```json
{
  "description": "Description from BaseAnchor and Description from ChildAnchor"
}
```
As with the previous example of multiple inheritance, the anchor could be inheriting from multiple bases, in which case what does super point to? super is everything the anchor inherits, merged together the same way as property inheritance: left to right, last in line wins. So if the last base doesn't have the property, super still finds it on an earlier one.
```piton
anchor Left:
    d: from-left

anchor Right:
    other: x

anchor Child extends Left, Right:
    d: ${super.d} plus child
```
Here Child.d is “from-left plus child”. If none of the bases have the property, that's a compiler error.

##### Super On Lists

###### Description

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
Blocks are built from top to bottom. A line starting with + or ++ takes everything above it and combines it with that line's value. So here we start with nothing, the + brings in super.items, and then D, E, and F get added.
Without the + we’d end up with
```piton fragment
anchor ChildAnchor extends BaseAnchor:
    items:
        - {super.items}
        - D
        - E
        - F
```
turning into
```
[[A, B, C], D, E, F]
```
This works the same way in dictionaries, where + and ++ merge them. And a block with just `+ {x}` in it is the same as `{x}`.
In strings, + joins them with nothing in between, and ++ puts a line break between them:
```piton
anchor Base:
    d: Base text.

anchor Plus extends Base:
    d:
        Child text.
        + {super.d}

anchor DoublePlus extends Base:
    d:
        Child text.
        ++ {super.d}
```
Plus.d is “Child text.Base text.” DoublePlus.d is the same, but on two lines.
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
The change is that ChildAnchor now also contains A, B, C, and D, and we’re also bringing super.items in at the end of the list. If you remember from when we discussed the + merge operator, it removes duplicates and keeps the last one. So ChildAnchor ends up with [D, A, B, C].
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
This time duplicates are kept, so ChildAnchor ends up with [A, B, C, D, A, B, C].

##### Self Reference

###### Description

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
self only travels through inheritance though. If you just read a property from another anchor, you get that anchor's value, with self being that anchor:
```piton
anchor Precedence:
    note: ${self} rules apply

anchor Addition:
    copied: ${Precedence.note}
```
Addition.copied is “Precedence rules apply”.
this and self always mean an anchor, never a dictionary. Even inside a nested dictionary, they mean the anchor it's in.

##### Abstract

###### Description

Abstracts allow us to define the shape of an anchor without providing values. An abstract anchor alone will never compile; it must be extended by a non-abstract anchor, and that non-abstract anchor must implement all undefined abstract properties.
```piton
abstract anchor Skill:
    description:: string


anchor ConcreteSkill extends Skill:
    description: This must be a string as defined by the abstract
```
We’ll introduce a new bit of terminology here in that a concrete anchor that extends an abstract anchor is said to be “implementing” the abstract anchor.
An abstract can give a property a default value. If it does, that property is optional. If it doesn't, it's required.

###### Abstract Chains

An abstract can extend another abstract. Whatever implements it has to fill in the required properties from the whole chain.
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

###### Multiple Abstracts

An anchor can implement more than one abstract, even ones that share a base. It is in fact good practice to export an abstract anchor as a keyword, and the keyword's anchor always goes last in the inheritance chain, so it wins.
If two abstracts have type constraints on the same property and the types don't overlap at all (say string and number), that's a compiler error. If they do overlap, the right-most one wins.
You are still free to extend other concrete anchors in addition to basing off an abstract. Type constraints from concrete anchors just follow normal inheritance: right-most wins, no error.

###### Special Type Constraints

We are also able to use the specialized type constraints within abstracts like simple, complex, any, [], etc.
There's also the extends type constraint keyword, which lets us represent inheritance hierarchy. You can use it anywhere, but it's most at home in abstracts. Take the following example.
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
A plain anchor type only matches anchors that directly implement it. If an anchor has more than one abstract, the one that counts is the one that wins: the keyword's abstract, or else the right-most one.
listOfA will be satisfied by anything that directly implements the abstract A.
listOfB will be satisfied by anything that directly implements the abstract B. Something implementing B won't fit in listOfA, even though B extends A.
Where it gets a little more interesting is in the third property, listOfExtendsA.
extends on an anchor type means that the constraint may be satisfied by any concrete anchor whose inheritance chain includes that anchor.
So in this example, that constraint is satisfied by a list of anything that has A in its inheritance hierarchy. So in this case, you’d be able to pass concrete anchors that implement either A or B given that B has A in its inheritance chain.

##### User Defined Keywords

###### Description

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
You can only use one keyword on an anchor, however you can still use extends to inherit from other anchors as well.
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
Note that the user-defined keyword will be the last anchor in the inheritance chain, and so it wins any collisions with what's in extends. So here ChildAnchor's description starts with “This is a description of my anchor”.
You can't use a reserved word as a keyword.

### Reuse

#### Description

Fundamental to Piton is the ability to compose a larger codebase from smaller focused pieces, so we need a way to reuse code across files.

#### Import Export

##### Description

To make something defined within a file available to other files, you use the export keyword. For example:
```piton
export pi: 3.14
myVariable: 42
export anchor MyAnchor:
    description: This is my anchor
```
To bring those into another file you use the from...import syntax:
```piton fragment
from ./FirstFile import pi, MyAnchor
```
Note that we can’t import myVariable because it wasn’t exported.
Paths for from...import are relative to the current file. The .pi extension is optional, and piton format removes it. Just . or .. points at the index.pi in that directory. If the project is configured with a piton.config.pi file, you can also use absolute path imports relative to the root value defined in the project config.
```piton fragment
from /subdir/subdir/file import AnAnchor
```
It’s also possible to import under an alias by placing the alias after the imported symbol:
```piton fragment
from ./FirstFile import pi SliceOf, MyAnchor MyAliasedAnchor
```
Worth clarifying that pi will be imported as and only as SliceOf (i.e. pi will not be available in scope).

#### Import Line Breaks

##### Description

Linebreaks are allowed on imports/exports:
```piton fragment
from ./file import
    FirstThing,
    SecondThing,
    ThirdThing
```
piton format will automatically add linebreaks to imports/exports if there are greater than 2 items or if the line exceeds 80 columns, and it will sort the imports.

#### Modules

##### Description

In a complex project, it’s likely that you’ll end up with hundreds of files that all work together, often grouped by domain or structured for reusability. Every complex layer of a codebase should be as self-contained as possible, requiring minimal inputs and outputs, and that even extends to imports.
Consider a situation where you’re importing twenty anchors from one directory. That’s a lot of boilerplate, and it’s something likely to be repeated every time you want to import that functionality.
To solve for this annoyance Piton supports folders-as-modules with an index.pi file. When an index.pi file is present in a directory, you can now import anything exported from that file simply by pointing to the directory.
```
myCurrentFile.pi

path/
  to/
    directory/
      index.pi
```
And in myCurrentFile.pi you could have:
```piton fragment
from ./path/to/directory import MyAnchor
```
There are several ways to build an index.pi file.
```piton fragment
from ./MyAnchor import MyAnchor
export MyAnchor
```
That functions, but it’s a bit verbose. There is a modification of the from...import syntax that allows you to be a bit more concise:
```piton fragment
from ./MyAnchor export MyAnchor
```
And a slight modification of that that’s even more concise:
```piton fragment
from ./MyAnchor export *
```
from...export also supports renaming exports:
```piton fragment
from ./MyAnchor export MyAnchor MyAliasedAnchor, MyOtherAnchor MyOtherAliasedAnchor
```

#### Use

##### Description

When you want to use a user-defined keyword, you’ll need to apply the use keyword. It brings into scope of the current file any exported user-defined keywords.
```piton fragment
use ./CustomKeywords

my-custom-keyword Wow:
    description: amazing
```
`use` only brings in keywords. It does not import anything else that was exported, just as from...import does not import keywords.

#### Circular Imports

##### Description

Circular imports are supported and will not throw a compiler error. Circular references are fine as well as long as it does not create something impossible to resolve.
For example
```piton
anchor A:
    description: This anchor talks about ${B}

anchor B:
    description: This anchor talks about ${A}
```
Is perfectly fine because ${A} and ${B} both settle to a string (the anchor's name).
However
```piton fragment
A: {B}
B: {A}
```
Is a compile error because it simply cannot resolve.
The only cycles that are errors are ones like this, that can't resolve, and an anchor that ends up extending itself.

## Tooling

### Cli

#### Description

The CLI compiler is a command-line tool that allows you to compile Piton files into output. It also includes helper commands like format.

#### Commands

- description: Launches the specified agent with Piton fluency using the output of the [GenerateFluencyPrompt](./agent/skills/GenerateFluencyPrompt.md#generate-fluency-prompt) skill, which is FLUENCY_PROMPT.md in the project root.
  commandName: agent
  positionalArguments:
    agent: Which agent to run [ claude ]
  namedArguments:
    print-fluency: Print the entire fluency prompt
- description: Builds the project as configured by piton.config.pi
  commandName: build
  positionalArguments:
    config: optional path to piton.config.pi file
  namedArguments: null
  entry: The entry file is set by entry in the project config. If it's left out, it's index.pi inside root. If that doesn't exist, it's an error.
  emitted: Anything that is exported from the entry file will be compiled; it doesn't have to be explicitly *used* to be compiled. Anything those exports reference also gets compiled, so the links have somewhere to go. Everything else is left out, and abstract anchors never compile.
  output: Without a framework, build writes to the output directory set in the project config. That's the entry file, plus any file with something the entry's exports reference. Files keep their place under root and get the renderer's extension, so with the defaults spec/components/Button.pi becomes dist/components/Button.json. Frameworks add their own output on top of that.
  manifest: In a .piton directory that lives alongside the piton.config.pi file, a manifest.json file will be written with the build output.
    ```
    {
        "generated": [ pathToGeneratedFiles ]
    }
    ```
- description: Checks specific files or the project and reports errors
  commandName: check
  positionalArguments:
    path: Optional file or glob. Defaults to the project.
  namedArguments: null
  emit: false
  validate:
    - syntax
    - imports
    - references
    - types
    - inheritance
    - composition
    - exports
    - circular-dependencies
  diagnostics:
    errors: true
    warnings: true
  exitCode:
    success: 0
    errors: 1
- description: Compiles Piton
    Pointing at a single file, compile will output the compiled result to stdout. Glob-based paths won't work unless we also use Write the result next to the input file.
    If Write the result next to the input file. is set, the compiled result will be written to a file with the appropriate extension for the selected renderer that lives next to the input file.
  commandName: compile
  positionalArguments:
    path: file or glob
  namedArguments:
    renderer: Valid options are [ json, yaml, markdown ]. Defaults to json.
    write: Write the result next to the input file.
    dependencies: Include the source files it was compiled from.
  output: A compiled file is an object with one key per export. Anything that isn't exported is left out, and so are abstract anchors. The JSON examples elsewhere in this spec skip the exports and just show what's being discussed, to keep things short.
  dependencies: If Include the source files it was compiled from. is set, the result is wrapped with the source files it was compiled from. A build tool that imports a Piton file has to know which other files to watch, and only the compiler knows: imports resolve through the module graph, and a package may keep a file somewhere the importing text never names.
    It comes out as a JSON object with two keys: value, which is the compiled output as a string, and dependencies, which is a list of the source files' absolute paths.
    Bundled package files are left out, since they live inside the compiler rather than on disk.
- description: Applies canonical formatting.
  commandName: format
  positionalArguments:
    path: file or glob
  namedArguments:
    check: Only check the files, and report problems.  Don't write.
- description: Counts lines of Piton source for specific files or the project
  commandName: loc
  positionalArguments:
    path: Optional file or glob. Defaults to the project.
  namedArguments: null
  emit: false
  count:
    - total
    - code
    - comments
    - blank
  groupBy: file
  summary: true
  diagnostics:
    errors: false
    warnings: false
  exitCode:
    success: 0
- description: Runs the Piton language server. For what enter and indenting should do, see the editor behavior rules in the editors section.
  commandName: lsp
  positionalArguments: null
  namedArguments: null
  features:
    diagnostics: Validate Piton syntax and Belay semantics continuously, reporting invalid constructs, unresolved symbols, inheritance problems, type mismatches, circular dependencies, and invalid compositions directly in the editor. Don't report a missing optional property (one with a default value).
    completion: Suggest anchors, skills, agents, properties, keywords, imports, inherited members, and valid values based on the current scope and semantic context.
      However, don't autocomplete things that don't exist.
      Don't propose autocomplete on nothing. If I type `property: ` it shouldn't propose anything because the likely intent is to type unstructured next.
      Typing a [PropertyAccessOperator](./scope/language/operators/access/PropertyAccessOperator.md#property-access-operator) in the middle of a string shouldn't autocomplete because there's nothing to complete on a string.
    autoImport: When a referenced symbol exists elsewhere in the specbase, offer to automatically add the appropriate `use` or import declaration.
    hoverInformation: Show the resolved definition of a symbol, including its type, source, documentation, inheritance chain, exported status, and where applicable its compiled interpretation.
    goToDefinition: Navigate from any reference to the anchor, property, skill, agent, import, or other symbol that defines it.
    findReferences: Show every place a symbol is referenced, inherited, composed, interpolated, exported, or otherwise depended upon across the specbase.
    renameSymbol: Rename a symbol safely across the entire specbase while updating all imports, references, expressions, inheritance relationships, and compositions that depend on it.
    documentSymbols: Expose the structural contents of the current Piton file as an outline of anchors, skills, agents, properties, exports, and other named constructs.
    workspaceSymbols: Allow fast searching across all named constructs in the entire specbase regardless of which file defines them.
    semanticHighlighting: Highlight Piton constructs according to their semantic meaning rather than syntax alone, distinguishing anchors, references, properties, inherited values, exports, imports, expressions, types, and keywords.
    inlayHints: Show useful inferred information inline, such as resolved types, inherited origins, composition sources, or the anchor from which a value ultimately derives.
    codeActions: Offer context-sensitive fixes and transformations such as importing a missing symbol, creating an unresolved anchor, adding an export, qualifying an ambiguous reference, or resolving a simple inheritance conflict.
    importOrganization: Detect unused, duplicate, invalid, or unnecessarily broad imports and provide an action to clean and normalize them.
    formatting: Format Piton source according to the canonical language style, particularly indentation, spacing, declaration layout, expressions, imports, and multiline structures.
    inheritanceResolution: Understand `extends` relationships and expose the fully resolved inheritance chain, including which parent contributed each inherited property.
    compositionResolution: Understand `+`, `++`, and other Piton composition semantics and show how multiple inputs combine into the resulting construct.
    overrideTracking: Identify when a property overrides an inherited value and allow navigation between the overriding declaration and the declaration it replaces.
    conflictDetection: Report incompatible inherited or composed values where Piton or Belay cannot resolve the result deterministically.
    referenceResolution: Resolve symbolic references such as anchors and `@{...}` expressions to their actual target and report unresolved or ambiguous references.
    expressionValidation: Parse and validate Piton expressions inside `{...}`, `${...}`, `#{...}`, and `@{...}` expression forms according to their expected output type.
    expressionTypeInformation: Show the inferred output type of an expression and warn when the expression cannot produce the type required by its interpolation form or destination.
    exportValidation: Track explicit exports and report attempts to import or reference symbols that are not visible outside their defining module.
    moduleResolution: Resolve relative and root-based Piton imports according to the project configuration and report missing modules and invalid paths. Circular imports are fine, so don't report them.
    circularDependencyDetection: Find cycles that can't resolve, and anchors that end up extending themselves, and show the cycle that caused the error. Circular imports are fine and aren't reported.
    relatedSymbolNavigation: Provide navigation between closely related constructs such as an abstract and its implementations, a base anchor and its extensions, or a symbol and the constructs that compose it.
    hierarchyView: Expose inheritance and composition relationships as a hierarchy so the editor can show parents, children, extensions, and implementations of a selected construct.
    resolvedValueInspection: Allow the editor to show the final resolved value of a property after inheritance, overrides, composition, and expressions have been applied.
    provenanceInspection: For any resolved value, show exactly where it came from: its original declaration, inheritance path, composition step, override, or expression.
    compiledOutputPreview: Allow a Piton construct or file to be previewed as its compiled representation, such as Markdown, JSON, agent instructions, or another Belay target.
    sourceToOutputMapping: Maintain mappings between Piton source and compiled output so an editor can identify which source construct produced a particular section of generated output.
    unusedSymbolDetection: Warn about anchors, imports, properties, exports, or other declarations that are never referenced or contribute nothing to the compiled result.
    duplicateRedundantDefinitionDetection: Identify declarations that unnecessarily repeat inherited or composed values and could be removed without changing the resolved specification.
    documentationIntegration: Surface documentation comments and descriptions through hover, completion, symbol search, and hierarchy views so the specbase remains understandable while navigating it.
    incrementalAnalysis: Only re-evaluate the portions of the dependency graph affected by an edit rather than recompiling the entire specbase after every keystroke.
    workspaceIndexing: Maintain an index of symbols, relationships, references, exports, inheritance, and composition across the project so navigation and completion remain fast.
    editorSelectionRanges: Understand Piton's semantic structure so expanding selection moves naturally from a value to a property, declaration, and enclosing anchor.
    folding: Provide folding ranges for anchors, skills, agents, multiline values, documentation blocks, and other structural Piton constructs.
- description: Analyzes which parts of the specbase are reachable from specific files, anchors, or the project
  commandName: reach
  positionalArguments:
    target: Optional file, glob, or anchor. Defaults to the project.
  namedArguments: null
  emit: false
  traverse:
    - imports
    - references
    - inheritance
    - composition
  direction: outgoing
  include:
    - direct
    - transitive
  report:
    - reachable
    - unreachable
    - depth
    - paths
  groupBy: source
  summary: true
  diagnostics:
    errors: false
    warnings: false
  exitCode:
    success: 0
- description: Remove a package from the project
  commandName: remove
  positionalArguments:
    package: name of the package to remove
  namedArguments: null
- description: Clones and "un-gits" a repository into the tethers directory. Run it on its own and it installs everything in piton.config.pi's dependencies. Give it a source and it adds that to piton.config.pi and installs it.
  commandName: tether
  positionalArguments:
    source: Optional. URL of the git repository
  namedArguments: null
- description: Moves an installed package from tethers/ to untethered/ inside root, and rewrites any imports.
  commandName: untether
  positionalArguments:
    packageName: the name of the package to untether
  namedArguments:
    as: the name to give the untethered package
    no-rewrite: Don't rename any imports, simply move the package
- description: Using the dependencies in the piton.config.pi, update all packages
  commandName: update
  positionalArguments:
    packages: optional list of package names to update; if not provided, all packages will be updated
  namedArguments: null

### Project Config

#### Description

If the compiler finds a piton.config.pi file in the current working directory, it will read that configuration file to configure a project. Realistically, this is how any Piton project will usually be used. Through a project you can configure the root, the entry point, where the output goes and which renderer it uses, frameworks, packages, and dependencies. By default output goes to ./dist as JSON. Frameworks decide where their own output goes.
Configuration anchors are provided by the @piton/config use/import which is bundled into the compiler:
```piton
use @piton/config
use @piton/belay

from @piton/belay import ClaudeCodeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi // Optional; defaults to index.pi in root
    output: ./dist // Optional; defaults to ./dist
    renderer: json // Optional; json, yaml, or markdown

    frameworks:
        - {BelayFrameworkConfig}

belay-config BelayFrameworkConfig:
    codeRoot: ./src/
    adapters:
        - {ClaudeCodeAdapter}
```

#### Exports

The piton.config.pi file exports its main piton-config anchor, and the [Lsp](./scope/tooling/cli/Lsp.md#lsp) needs to be aware that it's a project config, not just a regular Piton file.

### Frameworks

#### Description

Frameworks are a construct within a Piton project that extend what the compiler outputs. Piton itself outputs data through renderers, and a framework can add adapters on top of them. Belay's adapters, for example, build on the Markdown renderer.
As of the current version of Piton there is one framework bundled with the language: the Belay framework. In future iterations of the language, we will build out much more functionality in the frameworks concept.
Frameworks are included in a project through the frameworks property in the project config. You’ll see a concrete example of this when we talk about the Belay framework. You can use and import a bundled framework's keywords and modules whether you've included it or not, and you still have to do that in each file. Including the framework is what turns on its output.

### Editors

- description: Editing plugin for Emacs. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter:
    - tree-sitter
    - lsp
- description: Editing plugin for Helix. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter:
    - tree-sitter
    - lsp
- description: Editing plugin for JetBrains. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter: jetbrains
- description: Editing plugin for Kate. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter: KSyntaxHighlighting
- description: Editing plugin for NeoVim. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter: vim
- description: Editing plugin for Sublime. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter: sublime-syntax
- description: Editing plugin for Vim. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter: vim
- description: Editing plugin for VsCode. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter:
    - textmate
    - lsp
- description: Editing plugin for Zed. Has full support for the LSP.
  editorBehavior:
    - Whether it's via the LSP or the editor extension, the editors need to behave in a sane and predictable way according to the following rules
    - enterOnColon: Enter on colon (on a new dictionary or anchor property) should do a new line and indent the new line to the correct +1 level.
      enterOnBlankLine: On a blank line inside a dictionary or anchor, enter should insert a new line and dedent it by 1 level.
      autoFormatOnSave: Autoformat on save is an option. Autoformat should add the space after `//`, but not touch anything else that's commented.
  syntaxHighlighter:
    - tree-sitter
    - lsp

### Consumption

- description: Astro plugin that brings in and adds supports for [VitePlugin](./scope/tooling/vite/index.md#vite-plugin)
- features:
    - imports .pi files
    - HMR
    - dependency tracking
    - virtual modules
  description: Should be a Vite plugin that allows loading a Piton file and reading properties from it.
    For example
    ```typescript
    import { SaveButton, pi } from '../spec/app.pi';
    import spec from '../spec/app.pi';
    
    const button = SaveButton;
    const cancel = spec.CancelButton;
    ```
    Each export in the Piton file is a named export. The default export is the whole file, same as what piton compile gives you.
  renderers: You should be able to configure the plugin with a default renderer (markdown, JSON, etc.) but you should also be able to import those renderers as functions and use them inline.

## Belay

### Description

A Piton framework for structured agent instructions, skills, commands, and agents, compiled into the formats used by agentic coding tools.

### Special Imports

```
description: The following special imports can be brought in from `@piton/belay` and used. They're filled in at compile time, for each adapter, as a path relative to the file they end up in.
BELAY_AGENT_ROOT: The agent's directory, so that might be .claude or .opencode, etc.
BELAY_PROJECT_ROOT: The project, where piton.config.pi lives.
BELAY_SHAPE_ROOT: The shape as configured in the project config. If not specified, it's the project root.
BELAY_CODE_ROOT: The code as configured in the project config.
BELAY_COMPILED_SHAPE: The compiled copy of the shape for the agent, under its reference directory.
```

### Framework

- description: Belay is the framework bundled with Piton for describing agentic instructions, skills, commands, and agents, and compiling them into artifacts understood by configured agentic platforms.
  requirements:
    - Express reusable agent behavior as structured Piton source.
    - Connect descriptions of what should exist with guidance for building it.
    - Preserve composition through Piton anchors and inheritance.
    - Produce platform-specific artifacts through configured adapters.
- description: Piton source and project configuration define the intended system and agent guidance. Compiled artifacts are derived representations of that source, rather than an independent specification.
  requirements:
    - Make lasting specification changes in the originating Piton source.
    - Derive generated guidance from the resolved source and configuration.
    - Treat implementation as something to assess against the specification.
    - Distinguish predictable artifact generation from probabilistic agent behavior.
    - Load the four constructs from their own .pi files in this spec, so the compiler and the spec can't disagree.
- description: Piton handles the language and the renderers. Belay adds the agentic pieces and the adapters that turn them into files for each platform.
  requirements:
    - Defer parsing and expression evaluation to Piton.
    - Defer imports, exports, inheritance, and reachability to Piton.
    - Use adapters to translate resolved Belay constructs into target artifacts.
    - Leave execution of generated instructions to the consuming agentic platform.
    - Do not treat successful compilation as proof of correct agent execution.
- description: A project turns on Belay by adding a belay-config anchor to the frameworks property of its Piton config.
  requirements:
    - Register the Belay configuration in the project frameworks list.
    - Use codeRoot to identify the application's source-code root.
    - Use shapeRoot, when configured, to identify the architectural instruction root.
    - Use adapters to select the output targets for the project.
    - Require individual files to use or import the framework definitions they need.
    - Do not interpret framework registration as an implicit import in every file.
  package: @piton/belay
  configuration:
    keyword: belay-config
    fields:
      codeRoot: Required. Where the app's code lives, relative to the project config.
      shapeRoot: Optional. Where the shape instructions live. Without it, BELAY_SHAPE_ROOT is the project root and nothing is placed by shape.
      adapters: Required. The list of adapters to build for.
      crossDiscovery: Optional. allow or separate. Required when one tool would pick up another tool's output and change how it activates.
      instructionByteLimit: Optional. The instruction byte limit to check against, for targets that have one.
    example:
      codeRoot: ./src/
      shapeRoot: ./spec/shape/
- description: Belay constructs are ordinary Piton anchors with framework-specific meaning. Their keyword forms provide reusable vocabulary without introducing a separate composition language.
  requirements:
    - Expose Agent, Instruction, Skill, and Command as the four agentic constructs.
    - Associate those anchors with agent, instruction, skill, and command.
    - Allow abstract anchors to provide shared structure and behavior.
    - Resolve inherited properties according to the Piton language specification.
    - Validate required properties on concrete constructs after inheritance resolves.
    - Preserve additional properties for serialization into the generated guidance.
    - Emit only what the build command emits: the entry file's exports and anything they reference.
  otherAnchors: Belay has more than the four constructs. The config anchor, the adapters, and the special imports are part of it too.

### Anchors

- description: An instruction supplies persistent guidance associated with an application scope. Its placement is part of its meaning.
  requirements:
    - Treat description and prompt as optional, and leave them out of the output when missing.
    - Map instructions under shapeRoot to the corresponding codeRoot scope.
    - Combine instructions assigned to the same scope into one guidance file per target.
    - Use the target's supported scoped guidance filename and representation.
    - Also preserve shape instructions in the target's compiled shape-reference tree.
    - Serialize additional properties according to the common serialization rules.
- description: A skill is a selectively loaded set of instructions for a particular kind of work. Its discovery metadata explains when it is useful.
  requirements:
    - Require useWhen. description and prompt are optional.
    - Report an error if the target needs a description and there isn't one.
    - Emit the skill into the configured target's skill directory and format.
    - Derive the skill name from the anchor using the adapter's naming rules.
    - Form the discovery description from description followed by Use when and useWhen.
    - Emit prompt as the primary body of the skill.
    - Serialize additional properties after the primary prompt.
    - Leave skill selection and loading to the consuming platform.
- description: A command provides an explicit entrypoint for invoking a prompt or directing work through skills and other guidance.
  requirements:
    - Treat description and prompt as optional, and leave them out of the output when missing.
    - Report an error if the target needs a description and there isn't one.
    - Emit a native command or the explicit-invocation equivalent defined by the adapter.
    - Prefix the generated command name with x- to distinguish it from a skill.
    - Map description and prompt to the command representation selected by the adapter.
    - Emit allowed-tools and model metadata when supplied and supported by the adapter.
    - Serialize additional properties after the primary prompt.
- description: An agent describes a role and the instructions for performing it, expressed in the format understood by the target platform.
  requirements:
    - Require role. description and prompt are optional.
    - Report an error if the target needs a description and there isn't one.
    - Emit the agent into the configured target's agent directory and format.
    - Use the kebab-case anchor name as agent identity unless the target requires another form.
    - Emit description as discovery metadata.
    - Begin the body with You are a followed by the role.
    - Emit prompt after the role introduction.
    - Map explicit tools and model settings only through supported target configuration fields.
    - Serialize additional properties after the primary prompt.

### Compilation

#### Description

Resolve a Belay project and emit the artifacts selected by its adapters.

#### Requirements

- Obtain the emitted and validated constructs from the Piton compiler.
- Build a target-specific output plan for each configured adapter.
- Resolve instruction placement and reference destinations before rendering.
- Detect shared paths and cross-target discovery conflicts across the entire plan.
- Serialize content using the shared rules and each adapter's native format.
- Validate links, metadata, names, and output ownership before writing.

#### Serialization

##### Description

Belay serializes resolved construct content as readable Markdown while preserving the structure of the source.

##### Requirements

- Render anchor names and prose-bearing property names as word-separated titles.
- Use heading levels to represent the hierarchy of prose-bearing properties.
- Use bold labels when the hierarchy exceeds Markdown's six heading levels.
- Serialize primitive values to their textual representation.
- Render explicit lists as Markdown lists with nested indentation.
- Render pure dictionaries as indentation-based structures inside code fences.
- Preserve the order of prose and structured content within implicit mixed lists.
- Render pure dictionaries embedded in mixed content as fenced structures.
- Apply target-specific frontmatter and body layout around the serialized content.

##### Examples

```
propertyTitle:
  sourceName: myProperty
  renderedTitle: My Property
primitiveText:
  booleanValue: false
  renderedBoolean: false
  numericValue: 42
  renderedNumber: 42
```

#### Interpolation

##### Description

Piton evaluates expressions; the selected output mode determines how their results appear in generated artifacts.

##### Requirements

- Follow the current Piton specification for expression evaluation and casts.
- Preserve the distinction between intrinsic values, strings, and references.
- Render Belay references as links to the applicable compiled target.
- Do not infer reference identity from a display heading alone.

##### String Interpolation

${Anchor} gives you the anchor's name as written in the source. It isn't turned into a title.

### Scope

- description: The shape tree approximately mirrors the application source tree. Instructions attach to the closest existing implementation scope.
  requirements:
    - Determine an instruction's relative directory beneath shapeRoot.
    - Resolve that relative directory beneath codeRoot.
    - Place scoped guidance there when the corresponding directory exists.
    - Otherwise walk upward to the nearest existing corresponding directory.
    - Stop fallback at codeRoot.
    - Combine all instructions resolving to the same destination scope.
    - Preserve the original relative structure in the compiled shape-reference tree.
  examples:
    matchingDirectory:
      source: spec/shape/components/button/Button.pi
      existingScope: src/components/button
      genericOutput: src/components/button/AGENTS.md
    sharedDirectory:
      sources:
        - spec/shape/components/input/Input.pi
        - spec/shape/components/input/InputDesign.pi
      existingScope: src/components/input
      genericOutput: src/components/input/AGENTS.md
    missingDirectory:
      source: spec/shape/components/nonexistent/Component.pi
      existingScope: src/components
      genericOutput: src/components/AGENTS.md
  qualification: AGENTS.md illustrates generic placement. Each adapter selects its supported filename. Fallback changes scoped placement, not the original location preserved in the shape-reference tree.
- description: The special export BELAY_COMPILED_SHAPE points at the compiled shape for the current adapter. Not to be confused with BELAY_SHAPE_ROOT, which is the shape source.
  requirements:
    - Make BELAY_COMPILED_SHAPE available as an explicit import from Belay.
    - Resolve it during compilation rather than at agent runtime.
    - Resolve its path relative to the generated file containing its use.
    - Resolve it separately for each adapter and output location.
  documentedClaudeLocation: .claude/reference/shape
- description: A reference connects generated guidance to another anchor's compiled representation without embedding all of its content at the use site.
  requirements:
    - Serialize reached, referenced anchors into the target's reference directory.
    - Preserve their relative source structure in that directory.
    - Render referential interpolation as a Markdown link to the compiled artifact.
    - Keep referential links distinct from inline value serialization.
    - Do not replace lazy Markdown links with Claude-specific eager import syntax.
  representation: Belay uses the Markdown renderer's references: a relative link from the generated file to wherever the other anchor was compiled.

### Adapters

- description: Compile Belay constructs into project-local Claude Code artifacts.
  targetId: claude-code
  instructionFile: CLAUDE.md
  referenceRoot: .claude/reference
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve BELAY_COMPILED_SHAPE to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
    convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter. How you write them is still an open decision.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: Check the complete output plan before writing. Codex and OpenCode can share AGENTS.md paths, and OpenCode can discover other adapters' skills. File separation alone does not guarantee target isolation.
    requirements:
      - Reject incompatible writes to a shared instruction file.
      - Coalesce identical shared guidance only once.
      - Diagnose duplicate discoverable skills across enabled adapters.
      - Require an explicit deployment choice when cross-discovery changes behavior.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: CLAUDE.md at the scope selected by ShapeMapping
    format: Markdown
    rules:
      - Combine guidance for the same scope in one file.
      - Preserve a separate compiled shape reference beneath .claude/reference/shape.
    loading: Ancestor guidance loads at startup; nested guidance loads when Claude reads within that subtree. Placement does not make every nested instruction part of the initial context.
  skill:
    output: .claude/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
    rules:
      - Use the same normalized name for the directory and metadata.
      - Preserve native allowed-tools and model options only when explicitly configured.
  command:
    support: translated to a manually invoked Claude skill
    output: .claude/skills/x-<name>/SKILL.md
    metadata:
      name: x- followed by the normalized construct name
      description: Command description
      disable-model-invocation: true
    body: Command prompt followed by serialized additional content
    invocation: /x-<name>
    rationale: Claude unifies custom commands with skills. The older commands directory remains supported, but this adapter chooses skills for new output. Do not emit both representations of one command.
  agent:
    output: .claude/agents/<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Agent description
      tools: Explicit native tool list when configured
      model: Explicit native model selector when configured
    body: Role introduction followed by prompt and serialized additional content
    rules:
      - Preserve agent identity independently of display headings.
      - Omit optional settings when absent so native defaults remain effective.
  permissions:
    description: Skill allowed-tools grants and subagent tool selection serve different purposes. Do not interpret either as a portable sandbox definition or copy it into another platform without a mapping.
  checks:
    - A command produces one x-prefixed skill with automatic invocation disabled.
    - Two constructs normalizing to the same skill identity cause a collision diagnostic.
    - Nested instructions retain their mapped scope and target-relative links.
- description: Compile Belay constructs into project-local Codex artifacts.
  targetId: codex
  instructionFile: AGENTS.md
  referenceRoot: .codex/reference
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve BELAY_COMPILED_SHAPE to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
    convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter. How you write them is still an open decision.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: Check the complete output plan before writing. Codex and OpenCode can share AGENTS.md paths, and OpenCode can discover other adapters' skills. File separation alone does not guarantee target isolation.
    requirements:
      - Reject incompatible writes to a shared instruction file.
      - Coalesce identical shared guidance only once.
      - Diagnose duplicate discoverable skills across enabled adapters.
      - Require an explicit deployment choice when cross-discovery changes behavior.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: AGENTS.md at the scope selected by ShapeMapping
    format: Markdown
    rules:
      - Diagnose a same-directory AGENTS.override.md that shadows generated guidance.
      - Account for the configured instruction byte limit.
      - Do not generate override files merely to win precedence.
    loading: Codex builds a root-to-working-directory instruction chain at run start. Descendant placement does not guarantee initial loading from a session started above it.
  skill:
    output: .agents/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
  command:
    support: translated to an explicitly invoked Codex skill
    output: .agents/skills/x-<name>/SKILL.md
    policyOutput: .agents/skills/x-<name>/agents/openai.yaml
    metadata:
      name: x- followed by the normalized construct name
      description: Command description
    policy:
      allow_implicit_invocation: false
    body: Command prompt followed by serialized additional content
    invocation: $x-<name>
    rules:
      - Nest allow_implicit_invocation under policy in the generated YAML.
      - Do not emit deprecated personal custom prompts as repository commands.
      - Do not invent a project-local .codex/commands discovery directory.
  agent:
    output: .codex/agents/<name>.toml
    format: TOML
    fields:
      name: Normalized construct name
      description: Agent description
      developer_instructions: Role introduction, prompt, and serialized additional content
    optionalFields:
      - model
      - model_reasoning_effort
      - sandbox_mode
    rules:
      - Serialize the complete instruction body as a valid TOML string.
      - Omit absent options rather than imposing model or sandbox defaults.
      - Do not translate a generic tool list into an invented tools field.
    qualification: This targets the documented standalone custom-agent format, not older role-registration layouts or every hosted Codex surface.
  permissions:
    description: Native sandbox and approval configuration govern execution. Generated guidance cannot override the active runtime's controls. Unsupported per-command tool or model settings must be diagnosed.
  checks:
    - Commands produce both the skill and its explicit-invocation policy file.
    - Agent output parses as TOML and contains the three required identity and instruction fields.
    - Shadowed or over-budget instruction output is reported before being called usable.
- description: Compile Belay constructs into project-local OpenCode artifacts.
  targetId: opencode
  instructionFile: AGENTS.md
  referenceRoot: .opencode/reference
  documentationChecked: 2026-09-21
  requirements:
    - Consume resolved constructs after Piton imports and inheritance have been evaluated.
    - Map Instruction, Skill, Command, and Agent explicitly for the selected target.
    - Preserve the common Markdown content rules inside target-specific containers.
    - Distinguish native support from translation and unsupported behavior.
    - Fail with an actionable diagnostic when required behavior cannot be represented.
    - Apply only explicitly configured model and permission settings.
    - Keep target settings separate from extra prose properties in each construct.
    - Resolve generated Markdown links from the file containing the link.
    - Resolve BELAY_COMPILED_SHAPE to the selected adapter's compiled shape directory.
    - Record the target version used to validate the generated artifacts.
  paths:
    description: Adapter paths are relative to the project root (where piton.config.pi lives). Scoped instructions go through codeRoot and shapeRoot instead.
    convention: Ordinary references live beneath referenceRoot. Shapes live in its shape subdirectory. These paths are Belay-owned conventions and are not platform discovery directories. Never assume automatic loading.
    collisionPolicy: Normalize names before output planning. Reject collisions unless the artifacts are deliberately shared and identical in content, reference resolution, and activation behavior.
  metadata:
    description: Platform-specific options belong to the selected adapter. How you write them is still an open decision.
    requirements:
      - Validate native options against the selected platform version.
      - Serialize YAML or TOML with a format-aware encoder.
      - Keep role, prompt, and other prose out of duplicated metadata fields.
      - Reject permission translations that would silently widen access.
  unsupportedBehavior:
    description: A prompt that requests restraint is not an enforced restriction. Do not silently turn agents into skills or commands into automatic skills when that changes their intended activation.
  multipleAdapters:
    description: Check the complete output plan before writing. Codex and OpenCode can share AGENTS.md paths, and OpenCode can discover other adapters' skills. File separation alone does not guarantee target isolation.
    requirements:
      - Reject incompatible writes to a shared instruction file.
      - Coalesce identical shared guidance only once.
      - Diagnose duplicate discoverable skills across enabled adapters.
      - Require an explicit deployment choice when cross-discovery changes behavior.
  validation:
    - Verify all generated relative links point to planned outputs.
    - Verify discovery metadata and body content are emitted exactly once.
    - Verify a command retains the x- prefix after target-name normalization.
    - Verify generated filenames and serialized native metadata against the target schema.
    - Verify unsupported requested capabilities produce diagnostics before output is committed.
  instruction:
    output: AGENTS.md at the scope selected by ShapeMapping
    format: Markdown
    loading: OpenCode searches local rule files upward from the working directory and prefers AGENTS.md over its CLAUDE.md fallback. Do not assume Claude's nested-loading behavior or Codex's chain.
    rules:
      - Preserve scoped output placement without claiming identical activation across tools.
      - Use explicit instructions configuration when the selected deployment needs additional files.
      - Do not load all nested instructions globally merely to make them discoverable.
    nestedActivation: Only count on what OpenCode documents for startup. Don't assume nested files get picked up, and report an error if shape scoping is needed but can't be guaranteed.
  skill:
    output: .opencode/skills/<name>/SKILL.md
    format: Markdown with YAML frontmatter
    metadata:
      name: Normalized construct name
      description: Description followed by Use when and useWhen
    body: Prompt followed by serialized additional content
    rules:
      - Require a name of 1 to 64 lowercase alphanumeric characters with single hyphen separators.
      - Match name metadata to the containing directory.
      - Reject descriptions outside the supported 1 to 1024 character range.
      - Do not rely on unrecognized frontmatter to enforce policy.
    discovery: OpenCode also discovers compatible Claude and .agents skill directories. Check cross-adapter identities before installing several generated skill trees into one project.
  command:
    support: native
    output: .opencode/commands/x-<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      description: Command description
      agent: Explicit target agent when configured
      model: Explicit provider-qualified model when configured
    body: Command prompt followed by serialized additional content
    invocation: /x-<name>
    rules:
      - Derive command identity from the filename.
      - Preserve explicitly authored runtime argument placeholders.
      - Do not emit Claude-specific allowed-tools as an enforced command setting.
  agent:
    output: .opencode/agents/<name>.md
    format: Markdown with YAML frontmatter
    metadata:
      description: Agent description
      mode: Explicit primary, subagent, or all selection
      model: Explicit provider-qualified model when configured
      permission: Explicit native permission map when configured
    body: Role introduction followed by prompt and serialized additional content
    defaultMode: subagent
    rules:
      - Derive agent identity from the filename.
      - Prefer native permission entries over the deprecated tools option.
      - Validate native options instead of copying metadata from another adapter.
  permissions:
    description: Map permissions only when their meanings are equivalent. Native allow, ask, and deny entries are not interchangeable with a prose tool list. Reject ambiguous conversions.
  checks:
    - A command keeps its x-prefixed filename and native frontmatter.
    - Agent mode is explicit in generated output.
    - Duplicate skills discovered through other adapters are reported.
    - Required scope behavior that cannot be established for the target version is diagnosed.

### Guarantees

- description: Rules that keep Belay's output safe and repeatable.
  requirements:
    - Produce identical bytes for identical source, configuration, directory state, and toolchain.
    - Define a stable order for instructions combined into a shared output file.
    - Emit metadata and primary body fields once rather than duplicating them in the remainder.
    - Quote and escape frontmatter values according to the target format.
    - Detect collisions caused by normalized names or competing adapter output paths.
    - Report a missing generated reference target instead of emitting a broken link.
    - Report unsupported target metadata instead of silently claiming it was enforced.
    - Track generated files so cleanup cannot delete unrelated user-authored files.
    - Report the originating source anchor and property when generation fails.
    - Keep resolved output paths within their configured output boundaries.
- description: Written instructions express intended behavior. Actual restrictions depend on enforcement by the consuming platform.
  requirements:
    - Distinguish advisory prompt text from platform-enforced permissions.
    - Claim enforcement only for controls the target platform actually applies.
    - Diagnose a requested enforced restriction that the adapter cannot represent.

### Unresolved

#### Description

Things that aren't decided yet. Tooling shouldn't guess here; it should say it's not specified.

#### Instruction Scope

Define placement for instructions outside shapeRoot.

#### Reference Identity

Decide what happens when one anchor turns into more than one output, like a skill that's also referenced somewhere.

#### Metadata Types

Specify tools, allowed-tools, and model types and their mapping for each adapter.

#### Naming

Specify skill filename normalization, command name normalization before the x- prefix, acronym splitting, and name-collision handling.

#### Workflow Model

Decide whether workflows remain compositions of the four constructs. Do not add a workflow keyword solely from examples of agent processes.

## Package Management

### Description

Piton supports package management and dependencies.
Package sources are git repos and can be pinned to a specific commit, branch, or tag. If no specifier is provided, the latest commit on the primary/default branch is used.
The strategy for packages is managed vendored dependencies.  What that means is installed packages are stored as part of the project, committed to version control, but can be added, updated, removed, and inspected by the package management tooling.

### Packages

#### Description

A package declaration is a `piton-package` anchor that can be located anywhere within a specbase, but it must be included in the main piton.config.pi file.
The PitonPackage anchor is exposed by the `@piton/packaging` import/use.
```piton
export abstract anchor PitonPackage as piton-package:
    name:: string:: null: null
    root:: string
    dependencies:: list:: null: null
```
The name is what the package installs under, and it is separate from the anchor's name because it can be kebab-case, like my-package. Left out, the anchor's own name is used. Package names can't have a `/` in them. Names like `@piton/belay` are only for the packages that come with the compiler.
and in piton.config.pi
```piton fragment
packages:
    - {MyPackage}
```
Note here that a project can define multiple packages with different roots and their own different dependencies.
packages is what your repository offers to other projects. A project never installs its own packages. When another project tethers your repository, it gets every package listed here, unless it filters them, each in its own directory under tethers/.

#### Dependencies

If dependencies are specified at a project level, they only apply to the project. A Package must define its own, the same way.
There are no nested dependencies; if MyPackage is required by two different packages at different versions, the one with the newest commit will be chosen and a warning will be displayed.

#### Installation

A package is installed into tethers/ according to its name. So if a repository defines ui-kit and my-package, a project that tethers it gets tethers/ui-kit and tethers/my-package, with the files from each package's root.

### Dependencies

#### Description

Dependencies are listed under the dependencies property of the piton.config.pi file for a project. Packages list theirs the same way.
They're specified as a list of URLs, each with an optional commit, tag, or branch. Only one of those though; more than one is a compiler error. If there isn't one, you get the latest commit on the default branch.
```piton
dependencies:
    - https://github.com/piton-lang/piton-rs
        tag: 1.0
    - https://github.com/piton-lang/other
```
@piton/config types commit, tag, and branch as strings, so `tag: 1.0` stays “1.0” instead of becoming the number 1.
Project dependencies and package dependencies are separate. The project's only apply to the project, and each package has its own.

### Importing And Using

#### Description

Where most `from...import` and `use` typically refer to relative paths, or the project root path via `/` a package can be imported simply by naming it.  So presuming a package is called my-package it can be imported as
```piton fragment
from my-package import MyAnchor
```
If it doesn't exist, it's a compiler error; same as trying to import something that doesn't exist.
Similarly, you can do
```piton fragment
use my-package
```
or
```piton fragment
use my-package/MyAnchor
```
Note that regular path resolution behaves as it does anywhere else, it's just that a named package can serve as a locational placeholder.

### Commands

- description: Remove a package from the project
  commandName: remove
  positionalArguments:
    package: name of the package to remove
  namedArguments: null
- description: Clones and "un-gits" a repository into the tethers directory. Run it on its own and it installs everything in piton.config.pi's dependencies. Give it a source and it adds that to piton.config.pi and installs it.
  commandName: tether
  positionalArguments:
    source: Optional. URL of the git repository
  namedArguments: null
- description: Moves an installed package from tethers/ to untethered/ inside root, and rewrites any imports.
  commandName: untether
  positionalArguments:
    packageName: the name of the package to untether
  namedArguments:
    as: the name to give the untethered package
    no-rewrite: Don't rename any imports, simply move the package
- description: Using the dependencies in the piton.config.pi, update all packages
  commandName: update
  positionalArguments:
    packages: optional list of package names to update; if not provided, all packages will be updated
  namedArguments: null

### Locations

Tethered packages are stored in the same directory as the piton.config.pi file inside a tethers/ directory.
When a package is untethered, it's moved into untethered/ inside root and its imports are rewritten.

### Clone

Cloning with git (tethering) should remove any traces of git; these are just plain files now once they're tethered.

### Lock File

.piton/tether.lock lives next to piton.config.pi. For each tethered package it keeps the URL, the commit, and a hash of the files. tether and update write it, and it gets committed with the project.

### Conflict Resolution

When a package is updated (or tethered or anything else), it should check the current status of the files against the lock file, and only if there are no differences can it update. If it appears that the package has been modified, the user should be informed of the problem and asked to untether.
