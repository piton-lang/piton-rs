# Piton Fluency

This is a working reference for Piton, the Belay framework bundled with it, the
project configuration, the package manager and the tooling. Read it before you
read or change a Piton specbase. It is generated from the specification in
`spec/` by the GenerateFluencyPrompt skill. Where the compiler behaves
differently from the spec prose, the text below says so, and section 16 lists
every such gap.

## 1. What Piton is, and why

Piton is a declarative, whitespace-structured language for writing agent
guidance: skills, agents, commands, scoped instructions and reference
documents. Its author calls it a "semi-natural programming language". Prose and
structure are both first-class. A value can be a paragraph of plain English or
a typed, inherited, composed data structure, and most files mix the two.

The founding thesis (`spec/scope/founding-thesis/`) explains the design:

- Every programming language is an abstraction over the layer beneath it. Once
  an abstraction is accepted, nobody cares how its compiled output is produced,
  as long as the output is good. Agentic development is the next step in that
  history. In this model the agent is the compiler.
- Humans are good at designing coherent systems, not at typing out the latest
  accepted abstraction. Piton keeps the describing, constraining and thinking,
  and hands the manual labour to the agent.
- Prompting an agent session by session produces results that live in one
  developer's memory and cannot be traced. AGENTS.md, CLAUDE.md and Markdown
  skills help, but pure prose decays. It is ambiguous, it gets duplicated, and
  it has no tools for reuse. Piton adds imports, inheritance, types and
  references to prose.
- The CNC analogy: G-code replaced the machinist's hand, and CAM then sat on
  top of G-code by combining design intent with the machinist's expertise. The
  CAM engineer never types G-code but is essential. The Piton author plays that
  role for agents.
- The stated goal is that you can paste the specification into an agentic
  coding tool and get a compiler for Piton and Belay back. This repository is
  that experiment. The specification in `spec/` is written in Piton, and the
  compiler in `crates/` is judged against it.

Core facts:

- **There is no runtime.** Piton compiles to data (JSON, YAML, Markdown)
  through adapters. There are no functions, no loops, and no conditionals
  except the ternary operator. Every value is immutable.
- **The `.pi` source is the source of truth.** Compiled Markdown under
  `.claude/reference`, `.claude/skills`, `CLAUDE.md` files and so on is derived
  output. Make lasting changes in the `.pi` files and rebuild. Never hand-edit
  generated artifacts.
- A **specbase** is a Piton project: a `piton.config.pi`, a source root
  (usually `spec/`), an entry file, and the frameworks and packages it uses.
- **Successful compilation shows that the guidance is well-formed. It does not
  show that an agent will behave correctly.** Artifact generation is
  predictable, but agent behaviour is probabilistic.

## 2. Files, whitespace, comments, keywords

- Source files use the `.pi` extension.
- **Indentation defines structure**, as in Python. Tabs or any number of
  spaces are accepted, but one file must be consistent. The canonical style is
  4 spaces. `piton format` enforces it, and it cannot be configured.
- **Comments** are line-only and start with `//`. There are no block comments.
  A comment may follow a value: `myVariable: 42 // note`. The formatter puts one
  space after `//`. To write a literal `//` in text, escape it (`\//`) or quote
  it (`"// text"`).
- **Modules.** A directory that contains an `index.pi` is a module, and you can
  import from the directory path.
- **Keywords** are reserved words. They are always lowercase and may be
  kebab-case (`arithmetic-operator`). The built-in ones are `anchor`,
  `abstract`, `extends`, `as`, `export`, `from`, `import`, `use`, `pass`,
  `true`, `false`, `null`, `self`, `this`, `super`, and the type names
  `string`, `number`, `boolean`, `null`, `list`, `dictionary`, `any`, `simple`
  and `complex`. Users define more keywords with `as` (section 8.6).

## 3. Declarations and values

A file is a sequence of top-level declarations. Top-level names are variables
scoped to their file: they are global within the file and invisible outside it
unless exported.

```piton
myVariable: 42
export pi: 3.14

export anchor MyAnchor:
    description: This is my anchor
```

`key: value` assigns a value, and the space after the colon is required. The
value can sit on the same line or be indented on the lines below:

```piton
sameLine: Hello, World!
nextLine:
    Hello, World!
```

Both produce `"Hello, World!"`. The leading indentation belongs to the syntax,
not to the string.

An anchor whose body is empty is written with `pass`.

### 3.1 Types

Piton is loosely typed, with optional constraints.

| Type | Notes |
| --- | --- |
| `string` | Unicode text, unquoted by default. |
| `number` | One numeric type with no int/float distinction. A decimal needs its leading `0` (`0.14`). `_` may separate digits (`1_200_000.00`, which compiles to `1200000`). |
| `boolean` | Lowercase `true` and `false`. |
| `null` | Lowercase `null`. It is the only "nothing" value (there is no `undefined`), and `null == null`. |
| `list` | An ordered collection. Item types may be mixed unless a constraint says otherwise. |
| `dictionary` | Key/value pairs. Keys are strings, and value types may be mixed unless constrained. |
| anchor | The one user-defined type (section 8). |

- **Simple types** are string, number, boolean and null.
- **Complex types** are list, dictionary and anchor.
- A **reference** is a type concept produced by `@{}` (section 6.5). How it
  renders depends on the adapter.

Every built-in type is described by the abstract `type` anchor
(`spec/scope/language/types/lib/Type.pi`), which lists `supportedOperators`.
Using an operator that is not listed is a compiler error, but the compiler
applies the coercion and inference rules before it reports one.

### 3.2 Inference

Without a constraint, the type of a value is inferred from the whole literal:

| Source | Inferred as |
| --- | --- |
| `true` | boolean |
| `true story` | string |
| `42` | number |
| `42 things` | string |
| `A + B` (inside `{}`) | the shared type if A and B have the same type, otherwise string |
| `This costs $5 + tax` | string, because operators outside `{}` are plain text |
| `null` | null |
| `\// Just Text` | string |
| `"// Also Just Text"` | string |
| `${}` | string (empty) |

### 3.3 Strings

- Strings are unquoted.
- In a multi-line block, consecutive lines join with one space. **A blank line
  produces a line break** (`\n`).

  ```piton
  myString:
      This broken string is not considered
      a line break.

      While this *is* on a new line because there was a blank line above.
  ```

  This compiles to
  `"This broken string is not considered a line break.\nWhile this *is* on a new line because there was a blank line above."`.
- Double quotes make a value explicitly a string. A quoted value is never
  coerced (`"false"` stays a string), and it can hold text that would otherwise
  parse, such as `"// not a comment"`. A line that is entirely one `"..."`
  loses its quotes in output. A quote pair that crosses a line break is a
  syntax error.
- **Fenced code blocks** inside a string are kept verbatim, newlines included,
  and are not escaped. Markdown tables must be fenced, because otherwise their
  rows join into one line. The spec wraps example code in a fence *and* an
  escape block so that the example is not parsed:

  ````piton
  description:
      ```piton
      \\\
      anchor Example:
          value: {1 + 2}
      \\\
      ```
  ````

### 3.4 Escaping

Escaping in Piton works by wrapping. Put backslashes around the special
characters: `\ {1 + 2 + 3} \` compiles to the literal `{1 + 2 + 3}`. To escape
backslashes themselves, stack them: `\\ \ {x} \ \\` compiles to `\ {x} \`, and
each extra level adds one more backslash. Escape blocks may span several lines,
which is what `\\\ ... \\\` does in the spec's examples.

Short forms that work inside prose: `\{x}`, `$\{x}`, `\:`, `\-`, `\//`.

**Prose pitfalls.** Each of these silently changes the structure of a string
block:

- A line that starts `- ` makes a list item, and a line that starts `+ ` or
  `++ ` makes a merge line. A property declared `:: string` then fails. Write
  `\- item`.
- A line shaped like `word:` (`Similarly:`, `Inputs: none`, or a wrapped line
  that happens to start `type:`) becomes a dictionary key, and the block turns
  into a mixed list. Escape it as `Similarly\:` or rewrap the paragraph.
- Backticks do not protect sigils. `` `{x}` `` is still an expression. Write
  `\{x}` or `$\{x}`. An empty `${}` is fine.
- Backslashes are stripped even inside backticks. A literal `\` is only
  possible inside a fenced code block.
- A reserved word used as a key (`use:`, `from:`) becomes a prose line, not a
  property. Pick another key name.

### 3.5 Lists

```piton
markdownStyle:
    - One
    - List
    - Item

inlineStyle: [This, is, a, list, of, strings]

nestedMarkdownList:
    - Level 1
        - Level 2
            - Level 3
// equivalent to [Level 1, [Level 2, [Level 3]]]
```

**Items cannot be accessed by index.** By design there is no `myList[0]`.
Piton describes things; it does not compute them, and lists exist to be merged
and inherited. Lists support `+` and `++` (section 6.3).

### 3.6 Dictionaries

A dictionary has one syntax, which is indentation:

```piton
firstLevel:
    secondLevel:
        thirdLevel: This is a string
```

- Dot access works: `{firstLevel.secondLevel.thirdLevel}` gives
  `"This is a string"`.
- A key may contain Unicode letters, Unicode digits, `-` and `_`, but **no
  spaces**. Anything that coerces to a string is a valid key, so `thisIsAKey`,
  `123`, `foo-bar`, `false` and `null` are all keys, stored as strings.
  `This is a key` and `1 2 3` are not keys.
- Dictionaries support `+` and `++`.

### 3.7 Implicit mixed lists

A block that mixes prose, list items and keys becomes an **implicit list**
that keeps the source order:

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

This compiles to
`["This is a string", ["This", "Is", "A", "List"], {"nestedDictionary": {"deeplyNestedDictionary": "This is a string"}}]`.
Consecutive prose paragraphs, meaning lines separated by blank lines with no
list or key between them, join into one string element with `\n`.

- **Keys declared directly in the mixed block stay addressable.**
  `combined.nestedDictionary.deeplyNestedDictionary` still resolves.
- The prose and list elements cannot be addressed, because there is no list
  access.
- A dictionary inside an **explicit** list item cannot be addressed at all:

  ```piton
  combined:
      - dictionaryInsideAList:
          nested: value
  ```

## 4. Variables and type constraints

- A variable is declared by naming it at the top level of a file:
  `myVariable: 42`.
- The file is its scope. `export` makes it visible to other files.
- Everything is immutable. There is no compile-time mutability.

### 4.1 Constraint syntax

A constraint is written `name:: type: value`. **The whitespace after `::` and
after `:` is mandatory**, so `myVariable::number:42` is an error.

```piton
count:: number: 42
tags:: string[]: [foo, bar, baz]          // list constraint
config:: dictionary:                       // nested constraints
    a:: number: 1
    b:: string: foo
a:: complex: [1, 2, 3]                     // list, dictionary or anchor
c:: simple: 1                              // string, number, boolean or null
g:: any: 1                                 // anything
```

A constraint without a value (`description:: string`) declares a **required
property**. This is how abstract anchors define their shape.

### 4.2 Multiple constraints and coercion

- `x:: number:: string: 42`. The value takes the first constraint, **read left
  to right**, that it can validly represent, so this gives the number `42`.
  `x:: string:: number: 42` gives the string `"42"`.
- `x:: boolean:: number:: string: "false"` gives the string `"false"`. Quoted
  values are always strings and never coerce.
- Unquoted literals coerce only when their syntax fits the target type. Values
  that are already structurally typed (lists, dictionaries, anchors, booleans
  and null) never coerce into unrelated types.
- If no constraint accepts the value, compilation fails with `type-mismatch`.
- **Nullable default.** Put `null` first: `notes:: null:: string: null`. If you
  write `x:: string:: null: null` instead, the string constraint wins and the
  value becomes the string `"null"`. The bundled packages do use
  `:: string:: null: null`, so treat such a property as "optional" and give it
  a real value.

### 4.3 Anchor-typed constraints

- `items:: A[]` accepts anchors that **directly implement** `A`.
- `items:: extends A[]` accepts any anchor whose **inheritance chain includes**
  `A`, so anchors that implement `B extends A` also qualify. The spec
  introduces `extends` in constraints for abstract anchor definitions, but the
  spec itself also uses it on exported lists:
  `export ARITHMETIC_OPERATORS:: extends ArithmeticOperator[]: ...`.
- A file that names an abstract as a type (`:: extends Foo[]`, `extends Foo`)
  must import it with `from ./Foo import Foo`. `use` alone is not enough
  (section 7.3).

## 5. Composition syntax inside blocks

A list block can spread another list into itself with a merge line:

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
// ["A", "B", "C", "D", "E", "F"]. Without the +, the result is [["A","B","C"], "D", "E", "F"].
```

- A `+` line merges and **removes duplicates**. When a value appears more than
  once it keeps its **last** position:

  ```piton
  items:
      - A
      - B
      - C
      - D
      + {super.items}
  // ["D", "A", "B", "C"]
  ```

- A `++` line merges and keeps every element: `- A` followed by
  `++ {super.items}` gives `["A", "A", "B", "C"]`.
- **Do not use `+ {super.x}` to extend a prose string.** Inside a string block
  it creates a mixed list. To extend inherited prose, interpolate:

  ```piton
  description:
      ${super.description} and more from the child.
  ```

## 6. Expressions

A value is plain text unless it is wrapped in an expression sigil. `a + b`
compiles to the string `"a + b"`. `{a + b}` compiles to `3`. The braces keep
the compiler simple, and they mean an unrecognized word never silently falls
back to being a string.

- Inside the braces, **bare words are symbols**. Quote string operands:
  `{2 + "Hello"}` gives `"2Hello"`. An unknown name is an `unresolved-symbol`
  error.
- **Keep the whole expression inside the braces.** `{this.flag && false}`
  evaluates to `false`. `{this.flag} && false` becomes the string
  `"true && false"`.
- **Forward references resolve.** Unresolved references are errors, and so are
  cyclic values: `A: {B}` together with `B: {A}` fails with `cyclic-value`.
- Collections resolve by value. Anchors resolve by reference and keep their
  identity until serialization.

  ```piton
  myList: [1, 2, 3]
  myDictionary:
      list: {myList}
  newList: {myDictionary.list + [4, 5, 6]}   // [1, 2, 3, 4, 5, 6]
  ```

### 6.1 The four expression forms

| Form | Name | Result |
| --- | --- | --- |
| `{expr}` | Standard | The intrinsic value, with its type preserved. An anchor serializes as its resolved content, which is a **copy**. |
| `${expr}` | String | Converts the value to a string, which can be embedded in prose. |
| `#{expr}` | Numeric | Converts the value to a number. |
| `@{expr}` | Reference | Keeps the anchor's identity. The adapter renders it as a **link**, not as the content. |

Use `@{X}` to point at another document, `{X}` to embed a copy of it, and
`${X}` to mention it by name.

### 6.2 `${}` stringification

- A string is returned unchanged.
- A number becomes its text form. A boolean becomes lowercase `true` or
  `false`. Null becomes `null`.
- An anchor becomes its name: `${Child}` gives `"Child"`, and `${self}` gives
  the name of the current most-derived anchor.
- The spec says lists become "a string representation" and dictionaries become
  the name of their variable or property. **The compiler rejects both with
  `no-string-form`**, so do not stringify collections.
- The result is never re-parsed as source.

### 6.3 `#{}` numeric conversion

- A number is returned unchanged.
- A string converts only if the **entire** string is a valid Piton numeric
  literal: `#{"42"}` gives `42`. The string is never evaluated as an
  expression.
- Booleans, null, collections and anchors are rejected (`not-a-number`).
- In structured output the value stays numeric.

### 6.4 Operators

| Group | Operators | Notes |
| --- | --- | --- |
| Access | `.` | Property access, on dictionaries and anchors. |
| Arithmetic | `+ - * / %` | Numbers only. `* / %` bind tighter than `+ -`. Operators of equal precedence run left to right, and parentheses group. `{2 + 3 * 4 % 5}` gives `4`. Division by zero is an error. |
| Comparison | `== != < <= > >=` | |
| Logical | `&& \|\| !` | |
| Concatenation | `+` | Joins strings. With mixed operand types, the result is a string. |
| Merge | `+` | On lists and dictionaries. Merges and removes duplicates, keeping each value's last position. |
| Duplicate merge | `++` | On lists and dictionaries. Merges and keeps all elements. |
| Conditional | `cond ? a : b` | Chainable in either branch. Only the selected branch is evaluated. `{this.flag ? "yes" : "no"}`. |

Operators each type supports: numbers take the arithmetic operators, strings
and booleans take `+`, lists and dictionaries take `+` and `++`, and null,
anchors and references take none. Anything else is `invalid-operand`.

**Control flow:** the ternary is the only conditional. There are no loops and
no functions.

### 6.5 Reference expressions `@{}`

- The expression must resolve to a referenceable anchor. Otherwise the
  compiler reports `reference-not-an-anchor`.
- The referenced anchor joins the compilation dependency graph, so it gets
  compiled and emitted.
- References are resolved separately for each output target. If a target
  cannot represent a reference, that is an error. A reference must never
  silently become an embedded copy or a plain name.
- **Markdown** renders a relative link from the file that contains the
  reference, with the anchor's display name as the link text:
  `[Button](../../reference/shape/components/button/Button.md)`. The link is
  **lazy**: the content is not inlined, and an agent follows it only when the
  work touches what it describes. When several anchors share one output file,
  the link targets the specific anchor.
- **JSON** renders `{"$ref": "file.pi#Anchor"}`.

## 7. Modules and reuse

### 7.1 Export and import

```piton
// FirstFile.pi
export pi: 3.14
myVariable: 42                    // not exported, so it cannot be imported
export anchor MyAnchor:
    description: This is my anchor
```

```piton
from ./FirstFile import pi, MyAnchor
from ./FirstFile import pi SliceOf, MyAnchor MyAliasedAnchor  // alias: only SliceOf is in scope, not pi
from /subdir/subdir/file import AnAnchor                      // root-relative
from ./path/to/directory import MyAnchor                      // resolves directory/index.pi
from my-package import MyAnchor                               // installed package, by name
from @piton/belay import ClaudeAdapter                        // bundled package
from ./file import
    FirstThing,
    SecondThing,
    ThirdThing
```

- Paths are relative to the current file. A path that starts with `/` is
  relative to the `root` in `piton.config.pi`. The `.pi` extension may be
  omitted.
- Only exported names can be imported. Importing a name that the module does
  not export is `unresolved-export`.
- Aliasing is `Name Alias`, with no `as`. After aliasing, only the alias is in
  scope.
- **Circular imports are allowed.** Mutually referencing values are also fine
  as long as every value settles. `anchor A` containing `... ${B}` and
  `anchor B` containing `... ${A}` work because both settle to strings.
  `A: {B}` with `B: {A}` cannot resolve, so it fails.

### 7.2 Index files and re-export

An `index.pi` file exposes a directory as a module:

```piton
from ./MyAnchor import MyAnchor
export MyAnchor                                     // verbose form

from ./MyAnchor export MyAnchor                     // concise form
from ./MyAnchor export *                            // everything MyAnchor.pi exports
from ./MyAnchor export MyAnchor Alias, Other OtherAlias   // renaming re-export
```

**Importing a name into an index does not re-export it.** When an index uses
`import`, consumers must import leaf anchors from their own files.

### 7.3 `use`

`use` brings the **keywords** a module exports into scope, and nothing else.
`from ... import` never brings keywords.

```piton
use ./CustomKeywords

my-custom-keyword Wow:
    description: amazing
```

- All of these forms are valid: `use ./CustomKeywords`, `use .`, `use ./`,
  `use ../cli`, `use my-package`, `use my-package/MyAnchor`,
  `use @piton/belay`.
- A file that declares with a keyword *and* names its abstract as a type needs
  both lines: `use ./Foo` and `from ./Foo import Foo`.
- Using a keyword without `use` gives `unknown-keyword`.

### 7.4 Formatting of imports

`piton format` sorts imports, and it breaks an import or export across lines
when it has more than 2 items or runs past 80 columns.

## 8. Anchors

An anchor is a named structural declaration. It is the core building block of
Piton, something like a class that is never instantiated. An anchor holds
properties, can be exported and inherited, acts as a type, and can be
referenced. An *object* is a value. An *anchor* is a declaration.

```piton
export anchor MyFirstAnchor:
    whatIsAnAnchor:
        An anchor is a kind of object or document that is structured via
        properties and values.
    listTypes:
        - List types
        - are supported.
    numberTypes: {3.14 - 3.14}
    booleanTypes: true
    nullType: null
    booleanOperatorsAnd: {this.booleanTypes && false}
    booleanOperatorsOr: {this.booleanTypes || false}
```

Compiled to JSON, an anchor becomes an object of its resolved properties.
`piton compile` wraps a file's top-level names in one object, as in
`{"MyFirstAnchor": {...}}`.

### 8.1 Structural inheritance

`anchor Child extends A, B:` copies the properties of every base and then
applies the child's own. On a collision, **the right-most base wins, and the
child beats every base.** Type constraints collide by the same rule. This is
structural inheritance, not polymorphism.

```piton
anchor FirstBaseAnchor:
    description: Description from FirstBaseAnchor
anchor SecondBaseAnchor:
    description: Description from SecondBaseAnchor
anchor ChildAnchor extends FirstBaseAnchor, SecondBaseAnchor:
    childAnchorProperty: Hello
// description is "Description from SecondBaseAnchor"
```

Inheritance cycles are errors (`inheritance-cycle`), and so is extending
something that is not an anchor (`invalid-base`, `unresolved-base`).

### 8.2 `super`

`super.prop` reads the inherited value. With several bases it reads from the
right-most base, following the same authority as property inheritance. Use
`${super.description}` to extend prose, and a `+ {super.items}` line to extend
a list (section 5).

### 8.3 `self` and `this`

- **`self`** means the most-derived anchor, and it travels down the hierarchy.
  When a base writes `${self.name}`, a child compiled from it shows the child's
  name.
- **`this`** is pinned to the anchor that lexically contains the expression.
  When a base writes `${this.name}`, it always shows the base's own value,
  including in anchors that inherit it. When a child overrides the property,
  the override's own `this` refers to the child.

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
// FinalChild: name "FinalChild Anchor",
//   baseDescription "Override on baseDescription MidChild Anchor",
//   childDescription "This is FinalChild Anchor"
```

Using `self`, `this` or `super` outside an anchor is an error (`invalid-self`,
`invalid-this`, `invalid-super`).

### 8.4 Abstract anchors

```piton
export abstract anchor Skill as skill:
    description:: string            // required: implementers must define it
    notes:: null:: string: null     // has a default, so it's optional

anchor ConcreteSkill extends Skill:
    description: This must be a string as defined by the abstract
```

- An abstract anchor defines a shape without providing values, and it **never
  compiles on its own**. A concrete anchor that extends it *implements* it and
  must define every property that lacks a value. Otherwise the compiler
  reports `unimplemented-property`.
- **A concrete anchor implements at most one abstract**
  (`multiple-abstract-bases`), because Piton does not union abstracts. It can
  still extend any number of concrete anchors. When type constraints conflict
  across abstracts, compilation fails.
- An abstract may extend another abstract: `abstract anchor B extends A:`.
- All the special constraints (`simple`, `complex`, `any`, `[]` and
  `extends X[]`) are available inside abstracts.
- Good practice is to export an abstract with a keyword, which makes the
  single-abstract rule ergonomic.

### 8.5 Properties on a keyword-declared anchor

After inheritance resolves, the compiler validates required properties on the
concrete anchor. Properties that the abstract does not declare are kept.
Belay relies on this: extra properties on a construct are serialized into the
generated guidance.

### 8.6 User-defined keywords

`as` gives an anchor a keyword alias. Declaring with the keyword is syntactic
sugar for `extends`:

```piton
anchor MyAnchor as my-anchor:
    description: This is a description of my anchor

my-anchor ChildAnchor:                     // = anchor ChildAnchor extends MyAnchor
    description:
        ${super.description} My additional description

my-anchor Mixed extends OtherBase:         // = anchor Mixed extends MyAnchor, OtherBase
    pass
```

- The keyword's anchor comes **first** in the inheritance chain, so bases to
  its right override it on collision.
- A keyword can be defined on top of another keyword:
  `export abstract operator ArithmeticOperator as arithmetic-operator:` builds
  on the `operator` keyword.
- To declare with a keyword, a file must `use` the module that exports it.

## 9. Compilation model

- **Entry exports compile.** For a project, everything exported from the entry
  file is compiled, whether or not anything uses it. Beyond that, only source
  reachable from the entry points is compiled, through imports, references,
  inheritance and composition (see `piton reach`). An anchor in another file
  that is not exported and not referenced produces no output.
- **Adapters** decide the format: `json`, `yaml` or `markdown` for
  `piton compile`, and the Belay agent adapters for `piton build`.
- **Markdown serialization**, which Belay uses for reference output:
  - Anchor names and prose-bearing property names become word-separated title
    headings. `myProperty` becomes `My Property`, and `t1` becomes `T 1`.
    Heading depth follows nesting, and bold labels take over past level 6.
  - Primitives become their text form (`false`, `42`, `null`).
  - Explicit lists become Markdown lists with nested indentation.
  - Pure dictionaries become indentation-structured text inside code fences,
    including when they are embedded in mixed content.
  - Mixed lists keep the order of their prose and structure.
  - A `{Other}` value is serialized as a copy of the anchor's content. Only
    `@{Other}` keeps identity, as a link.
- **Belay interpolation boundary:** Piton evaluates expressions, and the output
  mode decides how the results appear. The distinction between intrinsic
  values, strings and references is preserved. References render as links to
  the compiled target of the applicable adapter. Reference identity is never
  inferred from a display heading.

## 10. Project configuration (`piton.config.pi`)

The compiler reads `piton.config.pi` from the working directory. The
configuration anchors come from the bundled `@piton/config`. The language
server treats this file as a project config, not as an ordinary source file.

```piton
use @piton/config
use @piton/belay
use @piton/packaging

from @piton/belay import ClaudeAdapter

export piton-config Config:
    root: ./spec                 // source root, required; used by /-imports
    entry: ./spec/index.pi       // optional; defaults to the root
    frameworks:
        - {BelayConfiguration}
    packages:                    // packages this repo publishes
        - {MyPackage}
    dependencies:                // packages this project installs
        - https://github.com/org/repo
            tag: "1.0"           // or commit: abc / branch: main

belay-config BelayConfiguration:
    codeRoot: ./src              // application source root, required
    shapeRoot: ./spec/shape      // optional architectural shape tree
    adapters:
        - {ClaudeAdapter}

package MyPackage:
    name: my-scope/pkg           // optional; defaults to the anchor name; may contain /
    root: ./spec/pkg
```

What the bundled packages actually define:

```piton
// @piton/config
export abstract anchor PitonConfig as piton-config:
    root:: string
    entry:: string:: null: null
    frameworks:: list:: null: null
    packages:: list:: null: null
    dependencies:: list:: null: null
export abstract anchor FrameworkConfig as framework-config:
    pass

// @piton/packaging
export abstract anchor Package as package:
    name:: string:: null: null
    root:: string
    dependencies:: string[]:: null: null

// @piton/belay (config part)
export abstract anchor BelayConfig extends FrameworkConfig as belay-config:
    codeRoot:: string
    shapeRoot:: string:: null: null
    adapters:: list
```

**Frameworks.** A framework adds project-wide modules and extends what the
compiler outputs through adapters. Belay is the only framework today, and the
concept is expected to grow. Registering a framework in `frameworks:` makes its
keywords and modules *available*. **It is not an implicit import**, so every
file still has to `use` or `import` what it needs.

Configuration diagnostics include `missing-config`, `unknown-framework`,
`unknown-adapter`, `invalid-adapter` and `package-without-root`.

## 11. Belay: the agentic framework

Belay is bundled as `@piton/belay`. It gives Piton a vocabulary for agents and
compiles it into the artifacts that agentic coding tools read. The division of
labour:

- **Piton** handles parsing, evaluation, imports, exports, inheritance and
  reachability.
- **Belay** handles the constructs and how they are output.
- **Adapters** translate resolved constructs into each target's format.
- The **consuming platform** executes the result.
- **Suspense**, a separate authoring and orchestration environment, is not
  required. Piton and Belay stay open source independently of Suspense
  licensing.

Source authority: lasting changes go into the Piton source. Generated guidance
is derived from the resolved source and configuration, and implementation is
assessed against the specification.

### 11.1 The four constructs

Every construct extends the abstract `Construct`, which requires
`description:: string` and `prompt:: string`. Any other property is preserved
and serialized as Markdown after the prompt. The definitions the compiler
ships are the files in `spec/scope/belay/anchors/*.pi`, embedded at build time,
so changing `Skill.pi` changes the framework.

| Keyword | Extra required field | Meaning | Output |
| --- | --- | --- | --- |
| `instruction` | none | Persistent guidance tied to a code scope. **Its placement is part of its meaning.** | Mapped from `shapeRoot` to the matching `codeRoot` directory (11.2). Instructions for the same scope combine into one guidance file per target. Each is also preserved in the compiled shape-reference tree. |
| `skill` | `useWhen:: string` | Instructions the agent loads selectively for one kind of work. | The name comes from the anchor through the adapter's naming rules. The discovery description is `description`, then "Use when", then `useWhen`. The body is `prompt` followed by the extra properties. The platform decides when to load it. |
| `command` | none | An explicit entrypoint that invokes a prompt or directs work through skills. | The name is `x-` plus the normalized name. The output is a native command, or the explicit-invocation equivalent the adapter defines. |
| `agent` | `role:: string` | A role and the instructions for performing it. | Identity is the kebab-case anchor name. The body starts with "You are a" followed by the role, then the prompt, then the extra properties. |

```piton
use @piton/belay

export skill InspectSpec:
    description: Reads the spec for the Piton language and answers questions.
    useWhen: Explicitly Invoked
    prompt:
        Read the spec for the Piton language under spec/ and understand the
        language and features.
    reportFormat:
        All reports should be saved as a standalone HTML file.
```

`reportFormat` is an extra property. It renders as a "Report Format" section
after the prompt.

**Native options.** Some property names are treated as target metadata rather
than prose, and are never serialized into the body: `tools`, `model`,
`allowed-tools` (or `allowedTools`), `mode`, `permission`, `agent`,
`model_reasoning_effort` and `sandbox_mode`. Each adapter supports a subset
(11.4). An option the target cannot enforce produces an `unsupported-option`
warning, and an option that cannot be represented produces an `invalid-option`
error.

### 11.2 Shape mapping (instructions)

The tree under `shapeRoot` roughly mirrors the tree under `codeRoot`:

| Source | Existing scope | Generic output |
| --- | --- | --- |
| `spec/shape/components/button/Button.pi` | `src/components/button` | `src/components/button/AGENTS.md` |
| `spec/shape/components/input/Input.pi` and `InputDesign.pi` | `src/components/input` | one combined `src/components/input/AGENTS.md` |
| `spec/shape/components/nonexistent/Component.pi` | `src/components` (walked up) | `src/components/AGENTS.md` |

- The compiler takes the instruction's directory relative to `shapeRoot` and
  resolves it under `codeRoot`. If that directory is missing, it walks upward
  to the nearest directory that exists, **stopping at `codeRoot`**.
- Each adapter uses its own filename: `CLAUDE.md` for Claude, `AGENTS.md` for
  Codex and OpenCode.
- Falling back to a parent changes where the guidance is placed. The shape
  reference tree (`<referenceRoot>/shape/...`) keeps the original relative
  location.
- Diagnostics: `instruction-outside-shape`, `unplaceable-instruction`. What
  happens to an instruction outside `shapeRoot` is still an open decision.

### 11.3 References and special exports

- Exported, reached anchors that are not constructs, such as a
  `Specification` tree of plain `anchor`s, are serialized as Markdown under
  `referenceRoot`, keeping their relative source structure. For example,
  `spec/scope/belay/Belay.pi` becomes `.claude/reference/scope/belay/Belay.md`.
- `@{X}` in any generated file becomes a relative link to X's compiled file.
  Links stay lazy, and they are never replaced with Claude's eager `@import`
  syntax.
- **Special exports from `@piton/belay`.** Each is imported explicitly and
  resolved **at compile time, separately for each adapter and each output
  file**, as a path relative to the generated file that uses it. It is never
  resolved when the agent runs. Interpolate them with `${...}`.

  | Export | Resolves to |
  | --- | --- |
  | `__BELAY_SHAPE__` | the compiled shape root of the target, such as `.claude/reference/shape` |
  | `BELAY_AGENT_ROOT` | the agent directory: `.claude`, `.codex` or `.opencode` |
  | `BELAY_PROJECT_ROOT` | the project root |
  | `BELAY_SHAPE_ROOT` | the configured `shapeRoot`, or the project root if unset |
  | `BELAY_CODE_ROOT` | the configured `codeRoot`, or the project root if unset |

  ```piton
  use @piton/belay
  from @piton/belay import __BELAY_SHAPE__

  export skill Designer:
      description: ...
      useWhen: ...
      prompt: Read the shape documents under ${__BELAY_SHAPE__} first.
  ```

### 11.4 Adapters

`@piton/belay` exports `ClaudeAdapter`, `CodexAdapter` and `OpenCodeAdapter`.
Each is built on the abstract `belay-agent-adapter`, which has the fields
`targetId`, `instructionFile`, `referenceRoot`, `skillRoot`, `agentRoot` and
`commandSupport`. The spec prose calls the first one `ClaudeCodeAdapter`, but
import `ClaudeAdapter`.

| | Claude Code (`claude-code`) | Codex (`codex`) | OpenCode (`opencode`) |
| --- | --- | --- | --- |
| Instruction file | `CLAUDE.md` | `AGENTS.md` | `AGENTS.md` |
| Reference root | `.claude/reference` | `.codex/reference` | `.opencode/reference` |
| Skill | `.claude/skills/<name>/SKILL.md` (YAML frontmatter: `name`, `description`) | `.agents/skills/<name>/SKILL.md` | `.opencode/skills/<name>/SKILL.md`. The name must be 1–64 lowercase alphanumeric characters with single hyphens and must match its directory. The description must be 1–1024 characters. |
| Command | Translated: `.claude/skills/x-<name>/SKILL.md` with `disable-model-invocation: true`, invoked as `/x-<name>` | Translated: `.agents/skills/x-<name>/SKILL.md` plus `agents/openai.yaml` containing `policy: allow_implicit_invocation: false`, invoked as `$x-<name>` | Native: `.opencode/commands/x-<name>.md`. Identity comes from the filename. Invoked as `/x-<name>`. |
| Agent | `.claude/agents/<name>.md`, Markdown with frontmatter `name`, `description` and optional `tools`/`model` | `.codex/agents/<name>.toml` with `name`, `description`, `developer_instructions` (role, prompt and extras) and optional `model`, `model_reasoning_effort`, `sandbox_mode` | `.opencode/agents/<name>.md` with `description`, `mode` (always explicit, defaulting to `subagent`) and optional `model`, `permission`. Identity comes from the filename. |
| Native options | agent: `tools`, `model`. skill and command: `allowed-tools`, `model`. | agent: `model`, `model_reasoning_effort`, `sandbox_mode`. None for skills or commands. | agent: `mode`, `model`, `permission`. command: `agent`, `model`. None for skills. |
| Loading | Ancestor `CLAUDE.md` files load at startup. Nested ones load when Claude reads inside that subtree. | Builds a chain of files from the root to the working directory when a run starts. An `AGENTS.override.md` in the same directory shadows generated guidance. There is a byte limit for instructions. | Searches upward from the working directory and prefers `AGENTS.md` over its `CLAUDE.md` fallback. It also discovers Claude and `.agents` skills. |

Rules that apply to every adapter:

- Constructs are consumed **after** imports and inheritance resolve. Each of
  the four constructs is mapped explicitly for each target, and every target
  follows the shared Markdown rules.
- An adapter distinguishes native support, translation, and unsupported
  behaviour. When required behaviour cannot be represented, it fails with an
  actionable diagnostic instead of approximating.
- Model and permission settings are applied **only when configured
  explicitly**. When they are absent, the native defaults apply, and the
  adapter never imposes its own.
- **A prompt that requests restraint is not an enforced restriction.** An
  adapter claims enforcement only for controls the platform actually applies,
  and it never translates permissions in a way that silently widens access.
  Claude's `allowed-tools` and subagent `tools` serve different purposes, and
  neither is a portable sandbox.
- An adapter never turns an agent into a skill, or a command into an
  automatically invoked skill.
- Names are normalized before the output is planned, and commands keep the
  `x-` prefix after normalization. Collisions are errors unless the colliding
  artifacts are identical in content, reference resolution and activation.
- **Multiple adapters:** the whole output plan is checked before anything is
  written. Codex and OpenCode share `AGENTS.md` paths. Incompatible writes to a
  shared file are rejected, and identical shared guidance is written once.
  OpenCode's discovery of other adapters' skills is diagnosed
  (`cross-target-discovery`).
- Generated links must point at planned outputs, and discovery metadata and
  the body are each emitted exactly once. Frontmatter is serialized with a
  YAML or TOML encoder that understands the format.

Belay output diagnostics include `output-collision`, `invalid-artifact-name`,
`missing-command-prefix`, `description-too-long`, `broken-reference-link`,
`output-outside-project`, `cross-target-discovery`, `unsupported-option`,
`invalid-option`, `instruction-outside-shape` and `unplaceable-instruction`.

### 11.5 Build output and the manifest

`piton build` writes every planned artifact. It records each one in
`.piton/manifest.json`, next to `piton.config.pi`:

```json
{ "generated": [".claude/reference/Specification.md", "..."] }
```

The manifest lets later builds remove only files that Piton generated, so
files written by users are never deleted.

### 11.6 Proposed and unresolved areas

The spec marks the following as **proposed**, meaning not yet adopted:

- Byte-identical builds for identical inputs.
- A stable order for instructions combined into one file.
- Quoting frontmatter correctly.
- Reporting the source anchor and property when generation fails.
- Keeping output inside its configured boundaries.
- A permission boundary that separates advisory text from enforced controls.

The spec lists these as **open decisions** in `Decisions.pi`:

- Whether the base declaration stays `anchor` or becomes `spec`, and the exact
  syntax for abstract exports and keyword aliases.
- The layout of the reference directory.
- The complete configuration schema: path bases, behaviour without
  `shapeRoot`, and behaviour when `codeRoot` is missing.
- Placement of instructions outside `shapeRoot`.
- The reference value type and cross-target identity.
- Whether stringifying an anchor yields its source name or its compiled name.
- The types of `tools`, `allowed-tools` and `model`.
- Name normalization: acronym splitting and collision handling.
- Whether workflows remain compositions of the four constructs. The spec says
  not to add a `workflow` keyword just because examples describe agent
  processes.
- Pinning tested tool versions for each adapter. The documentation for each
  adapter was checked on 2026-09-21.

Do not present any of these as settled. If your work depends on one, raise it
with the user.

## 12. Package management

Packages are git repositories, managed as **vendored dependencies**. Installed
packages are stored inside the project and committed to version control, and
the tooling adds, updates, removes and inspects them.

- **Publishing.** A repository declares packages with `package` anchors from
  `@piton/packaging`, which have `name`, `root` and `dependencies`. A package
  anchor can live anywhere in the specbase, but it must be listed under
  `packages:` in `piton.config.pi`. One repository can publish several
  packages with different roots. `name` exists because it may contain `/`,
  which an anchor name cannot, and when omitted it defaults to the anchor's
  name. The spec prose shows the keyword as `piton-package`, but the compiler
  ships `package`.
- **Depending.** `dependencies:` lists git URLs. Each one can be pinned with
  `commit:`, `tag:` or `branch:` on an indented line beneath it. Without a
  pin, the latest commit on the default branch is used. Quote version pins
  (`tag: "1.0"`), because an unquoted `1.0` is read as a number
  (`unquoted-pin`). Other pin diagnostics are `dangling-pin` (a pin with no URL
  above it) and `unknown-pin`.
- Dependencies declared at project level apply only to the project, and each
  package must declare its own. A project-level pin is inherited by any
  package that does not set its own pin. **There are no nested dependencies**:
  when two packages need the same package, the newest version is chosen and a
  warning is shown.
- **Installation.** Packages go into `tethers/<name>`, next to
  `piton.config.pi`, so a package named `MyScope/package` lands in
  `tethers/MyScope/package`. Tethering removes every trace of git, leaving
  plain files. Nothing is fetched at compile time.
- **Importing.** An installed package is a location placeholder:
  `from my-package import X`, `use my-package`, `use my-package/MyAnchor`.
  Normal path resolution applies below it. A missing package is a compiler
  error (`unknown-package`).
- **Conflicts.** Before any update or tether, the tooling compares the
  vendored files against the locked version. It proceeds only if they are
  unchanged. If someone has edited them, it tells the user and asks them to
  untether.
- **Untethering** moves a package into `<root>/untethered/` so it becomes your
  own source, and rewrites the imports that pointed at it.

## 13. CLI reference

| Command | Behaviour |
| --- | --- |
| `piton check [paths...]` | Validates syntax, imports, references, types, inheritance, composition, exports and circular dependencies, reporting errors and warnings. Writes nothing. Exits 0 when clean and 1 on errors. With no paths it checks the project. |
| `piton build [config] [--dry-run]` | Compiles the project configured by `piton.config.pi` and writes every Belay artifact and `.piton/manifest.json`. `--dry-run` reports without writing. |
| `piton compile <path> [--adapter json\|yaml\|markdown] [--write] [--dependencies]` | Compiles one file to stdout, in JSON by default, including its unexported top-level names. `--write` writes `<file>.<ext>` next to each input and is required for globs. `--dependencies` wraps the result with the source files it was compiled from, so build tools know what to watch. Bundled package files are left out of that list. |
| `piton format [path] [--check]` | Applies canonical formatting. `--check` reports without writing. It **only splits overflowing lines and never rejoins a paragraph**, so rewrap edited prose by hand, and do not reflow `key: value` blocks as prose. |
| `piton reach [targets...] [--unreachable] [--paths]` | Starting from files or anchor names (or the entry by default), follows outgoing imports, references, inheritance and composition, both direct and transitive. It reports what is reachable with its depth, and optionally what is unreachable and the paths taken. Use it to find dead spec. |
| `piton loc [paths...]` | Counts total, code, comment and blank lines for each file, with a summary. |
| `piton lsp` | Runs the language server over stdio (section 14). |
| `piton agent [claude] [--print-fluency] [args...]` | Builds the project and refuses to launch if there are errors. Then it launches the agent with a project primer plus this fluency prompt, passed as `--append-system-prompt`. Extra arguments go to the agent. `--print-fluency` prints the prompt and does nothing else. |
| `piton tether <git-source> [--as name]` | Clones the repository, removes git, and installs its packages into `tethers/`. |
| `piton update [packages...]` | Reinstalls packages at the versions pinned in `piton.config.pi`, all of them when none are named. |
| `piton untether <package> [--as name] [--no-rewrite]` | Moves a package to `<root>/untethered` and rewrites its imports, or skips the rewrite with `--no-rewrite`. |
| `piton remove <package>` | Removes a package. |

Each CLI command in the spec is a `cli-command` anchor
(`spec/scope/tooling/cli/index.pi`) with `commandName`, `positionalArguments`
and `namedArguments`.

## 14. Language server and editors

`piton lsp` is meant to provide:

- Diagnostics, completion, hover (type, source, documentation, inheritance
  chain, export status and compiled interpretation), go-to-definition,
  find-references, and rename that works across the whole specbase.
- Document and workspace symbols, semantic highlighting, inlay hints
  (resolved types and inherited origins), and signature help.
- Code actions: import a missing symbol, create an unresolved anchor, add an
  export, qualify an ambiguous reference, fix a simple inheritance conflict.
  Also auto-import and import organization.
- Formatting.
- Resolution of inheritance, composition (`+`, `++`), overrides and conflicts.
- Reference resolution, and validation and type information for expressions
  inside `{}`, `${}`, `#{}` and `@{}`.
- Export validation, module resolution, and detection of circular
  dependencies.
- Navigation to related symbols, a hierarchy view, inspection of resolved
  values and their provenance, compiled-output preview and source-to-output
  mapping.
- Detection of unused and redundant definitions, and documentation
  integration.
- Incremental analysis, workspace indexing, selection ranges and folding.

Editing rules every editor must follow, through the LSP or the editor
extension:

- Enter after a trailing `:` on a new property opens a new line indented one
  level deeper.
- Enter on a blank line inside a dictionary or anchor inserts a new line one
  level shallower.
- Format-on-save is optional, and it never formats commented-out code.
- Completion never offers anything on an empty value (`property: `), because
  prose probably follows, and never offers anything after a `.` inside a
  string. It never suggests things that don't exist.

Supported editors, with their highlighters: VS Code (TextMate plus LSP),
Emacs, Helix and Zed (tree-sitter plus LSP), Neovim and Vim (vim syntax),
JetBrains, Sublime (sublime-syntax) and Kate (KSyntaxHighlighting). The
integrations live under `editors/`.

**Consuming Piton from JavaScript.** `packages/vite-plugin-piton` imports
`.pi` files as modules (`import spec from '../spec/app.pi'`, then
`spec.anchors.SaveButton`), with HMR, dependency tracking (through
`piton compile --dependencies`), virtual modules, a configurable default
renderer, and renderers you can import as functions. `packages/astro-piton` is
an Astro integration that wraps the Vite plugin.

## 15. Working on a specbase

1. **Orient.** Open `piton.config.pi` to find `root`, `entry`, the frameworks,
   the adapters, `codeRoot` and `shapeRoot`. Read the entry file, usually
   `spec/index.pi`, because its exports are the compilation roots. Follow the
   `index.pi` files down through the modules. In this repository,
   `spec/index.pi` exports `Specification`, whose sections are
   FoundingThesis, Language, Compiling, Tooling, Belay and PackageManagement,
   plus the skills in `spec/agent/`.
2. **Read compiled reference for overview, and source for truth.**
   `<referenceRoot>/**/*.md` is easy to browse, and its links are lazy, so
   follow them only when they are relevant.
3. **Edit the `.pi` source**, following the conventions around it:
   - one exported anchor per file, named after the file;
   - `index.pi` files that re-export with `from ./X export *`;
   - abstract anchors exported with a keyword, and shared abstracts in a
     `lib/` folder;
   - lowerCamelCase property names;
   - SCREAMING_CASE for exported constant lists;
   - prose in string blocks, and structure in keys and lists;
   - `@{X}` to link related documents.
4. **Keep imports honest.** Use `use` for keywords and `from ... import` for
   names, and a file often needs both. A new file only compiles if it is
   reachable from the entry, so export it through an index or reference it.
5. **Validate.**
   - Run `piton check`.
   - `piton check` does **not** compile code inside fenced blocks. When you
     edit spec examples, extract each ` ```piton ` block, run `piton compile`
     on it, and compare the output with any ` ```json ` block that follows.
   - Run `piton build`, then look at the diff of the generated files.
   - Run `piton reach --unreachable` to confirm new anchors are reached.
   - Run `piton format --check`.
6. **Never hand-edit generated files.** That includes `CLAUDE.md` and
   `AGENTS.md` under `codeRoot`, everything under `.claude/`, `.codex/`,
   `.agents/` and `.opencode/` that is listed in `.piton/manifest.json`, and
   the manifest itself.
7. **Keep implementation and spec in agreement.** The implementation is judged
   against the spec. When they disagree, report it instead of quietly bending
   the spec to fit. Where this document notes that the compiler differs from
   the spec prose, trust `piton compile` for how things behave today, and flag
   the gap.

## 16. Where the compiler differs from the spec prose

`piton compile` output confirms each of these:

- The spec shows `{this.booleanTypes} && false` evaluating to `false`. Only an
  expression that sits entirely inside the braces is evaluated, so write
  `{this.booleanTypes && false}`.
- The spec shows `+ {super.description}` followed by prose extending a string.
  In practice this produces a mixed list. Use `${super.description}`.
- The spec's inference note shows `2 + Hello`. Bare words inside `{}` are
  symbols, so write `{2 + "Hello"}`.
- `${list}` and `${dictionary}` are specified as a string representation and
  as the property name. The compiler rejects both with `no-string-form`.
- The LSP spec lists a `!{...}` expression form. It is not implemented, and
  `!{1}` compiles to the text `"!1"`. The only sigils are `{}`, `${}`, `#{}`
  and `@{}`.
- The spec names the Claude adapter `ClaudeCodeAdapter`, but the package
  exports `ClaudeAdapter`. The spec shows the package keyword as
  `piton-package`, but the compiler ships `package`.
- A duplicate key in one block is accepted without an error. Avoid duplicate
  keys.
- The `@piton/config` and `@piton/packaging` defaults use
  `:: string:: null: null`, which coerces a `null` default to the string
  `"null"`. Set these properties explicitly.
