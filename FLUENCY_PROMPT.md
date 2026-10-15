# Piton Fluency

A working reference for Piton, its bundled Belay framework, project
configuration, packages and tooling. Read it before reading or changing a Piton
specbase. It is generated from `spec/` by the GenerateFluencyPrompt skill;
section 16 lists where the compiler differs from the spec.

## 1. What Piton is

Piton is a declarative, whitespace-structured "semi-natural programming
language" for agent guidance: skills, agents, commands, scoped instructions and
reference documents. Prose and structure are both first-class; a value can be a
paragraph of English or a typed, inherited, composed structure.

Why (`spec/scope/founding-thesis/`): every language abstracts the layer below,
and agentic development is the next layer, with the agent as compiler. Humans
design coherent systems; agents do the typing. Session-by-session prompting
leaves no traceable artifact, and pure prose (AGENTS.md, CLAUDE.md) decays:
ambiguous, duplicated, not reusable. Piton adds imports, inheritance, types and
references to prose, so recurring patterns are described once. The author
likens it to CAM sitting above G-code. The goal is that pasting the spec into
an agent yields a compiler; this repository is that experiment, and `crates/`
is judged against `spec/`.

Core facts:

- **No runtime.** Piton compiles to data (JSON, YAML, Markdown) through
  **renderers**; frameworks like Belay add **adapters**. No functions, no
  loops, no conditionals except the ternary. Everything is immutable.
- **The `.pi` source is the truth.** Generated Markdown (`.claude/...`,
  `CLAUDE.md`, `AGENTS.md`, build output) is derived. Edit `.pi`, rebuild,
  never hand-edit output.
- A **specbase** is a project: `piton.config.pi`, a source root (usually
  `spec/`), an entry file, frameworks and packages.
- Compiling proves guidance is well-formed, not that an agent will behave.

## 2. Lexical rules

- Files end in `.pi`. A directory with an `index.pi` is a module.
- **Indentation is structure.** Tabs or any spaces, consistent per file;
  canonical is 4 spaces (`piton format`, not configurable).
- **Comments** are `//` on their own line only. After code, `//` is text:
  `url: https://x.com // note` is one string. The formatter adds a space after
  `//` and nothing else. Escape a leading `//` with `\ // \`.
- **Reserved words:** `anchor abstract export from import use as extends pass
  this self super true false null any simple complex string number boolean
  list dictionary`. They can't be keywords (`reserved-keyword`) but are fine as
  keys (`null: text`, `use: text`).
- Keywords are lowercase, may be kebab-case; users define them with `as` (8.6).
- An anchor needs a body: use `pass` when it has no properties
  (`anchor X:` alone is `empty-anchor`).

## 3. Declarations and values

Top-level names are file-scoped variables; `export` makes them importable.

```piton
myVariable: 42
export pi: 3.14
export anchor MyAnchor:
    description: This is my anchor
```

`key: value` needs a space after the colon; the value may sit on the same line
or be indented below (indentation is syntax, not part of the string).
**Every property needs a value**: an empty one is `empty-value`; write `null`
for nothing. Only abstract anchors may declare a constraint with no value
(`title:: string`); elsewhere that is `missing-value`.

### 3.1 Types

| Type | Notes |
| --- | --- |
| `string` | Unicode, never quoted. |
| `number` | One 64-bit float type. |
| `boolean` | `true`, `false`. |
| `null` | The only "nothing"; `{null == null}` is `true`. |
| `list` | Ordered, mixed item types unless constrained. |
| `dictionary` | String keys, mixed values unless constrained. |
| anchor | The one user-defined type. |
| reference | Produced by `@{}` (6.5). |

Simple = string, number, boolean, null. Complex = list, dictionary, anchor.

Unconstrained values are inferred from their look: `true` boolean, `true story`
string; `42`/`-42` numbers, `42 things` string; `A + B` string (no braces);
`"false"` string; `${...}` always string.

Each built-in type is a `type` anchor (`spec/scope/language/types/lib/Type.pi`)
with a `description` and `supportedOperators`. Using an unlisted operator is an
error (after coercion and inference):

| Type | Operators |
| --- | --- |
| number | `+ - * / %`, `== != < <= > >=` |
| string | `+`, `++`, `== != < <= > >=` |
| boolean | `&& \|\| !`, `==`, `!=` |
| null | `==`, `!=` |
| list | `+`, `++`, `==`, `!=` |
| dictionary | `.`, `+`, `++`, `==`, `!=` |
| anchor | `.`, `==`, `!=` |
| reference | `==`, `!=` |

### 3.2 Numbers

`123`, `3.14`, `0.14`, `1_200_000.00`, `-5`. A decimal needs its leading `0`
(`.5` is a string). `_` separates digits. No exponents (`1e3` is silently a
string). `-5` is a number; `- 5` is a list item. Minus is never a prefix
operator: `{-price}` is an error, write `{0 - price}`. Output uses the shortest
form (`1_200_000.00` → `1200000`, `0.50` → `0.5`).

### 3.3 Strings

- Unquoted. **Quotes are ordinary characters**: `greeting: "Hello"` keeps the
  quotes. Only inside an expression are string operands quoted
  (`{"Hello" + name}`), and there the quotes are not part of the value.
- In a multi-line block, lines join with a space; a blank line (or several)
  starts a new paragraph, compiled to `\n`.
- Markdown in a string is plain text: `[x](./x.md)` is never resolved or
  checked. Only `@{}` makes links.

### 3.4 Escaping

Wrap special text in backslashes with a space on each side (the spaces are
removed): `\ {1 + 2} \` → `{1 + 2}`, `\ : \` → `:`. This is the only escape; a
lone backslash (`\:`) is literal. Use more backslashes to escape backslashes;
the closing wrapper must match (`\\ \ {x} \ \\` → `\ {x} \`). A line holding
only backslashes opens a verbatim block closed by the same count; the spec uses
`\\\`.

Code fences protect nothing: expressions, comments, list items and keys inside
them still parse. Put an escape block inside the fence, as every spec example
does (tables need the same, or their rows join):

````piton
description:
    ```piton
    \\\
    anchor Example:
        value: {1 + 2}
    \\\
    ```
````

Escaping changes parsing, not inferred type: `\ false \` is still boolean; use
`${false}` for the string.

**Prose pitfalls** that silently change a block's structure:

- A line starting `- ` is a list item; `+ `/`++ ` is a composition line (5).
  A `:: string` property then fails. Escape: `\ - \ item`.
- A line shaped `word: text` (`Inputs: none`, a wrapped line starting `type:`)
  becomes a key and turns the block into an implicit list. Escape the colon or
  rewrap.
- Backticks don't protect: `` `{x}` `` is an expression.
- An expression in text that yields a list, dictionary or anchor makes an
  implicit list (6.2).

### 3.5 Lists

```piton
a:
    - One
    - Two
b: [This, is, a, list]
nested:
    - Level 1
        - Level 2
```

`nested` is `[Level 1, [Level 2]]`.

- **No index access** (`myList[0]` does not exist). Lists are merged and
  inherited, not computed over.
- A list item is never a key: `- Settings:` is the string `"Settings:"`.
- Anything indented under an item becomes the *next* item:
  `- url` with `tag: v1` indented under it gives `["url", {"tag": "v1"}]`.
  Such a dictionary cannot be addressed.

### 3.6 Dictionaries

Indentation only. `{a.b.c}` accesses nested keys. Keys contain Unicode
letters, digits, `_` and `-` only (no spaces or dots); `123`, `foo-bar`,
`false` are valid keys and always strings. A hyphen belongs to the name
(`{config.foo-bar}`); subtraction needs spaces (`{a - b}`).

A key whose block is only a list has that list as its value. With other content
above it, the value becomes an implicit list with the list as one item.

### 3.7 Implicit lists

A block mixing prose, lists and keys compiles to a list in source order:

```piton
combined:
    This is a string
    - A
    - B
    nested:
        deep: text
```

→ `["This is a string", ["A", "B"], {"nested": {"deep": "text"}}]`. Keys
declared directly in the block stay addressable (`{combined.nested.deep}`);
prose and list elements do not.

## 4. Variables and constraints

- **Inside an expression, a bare name is a file-level variable or import, even
  inside an anchor.** Use `this.x`/`self.x` for the anchor's own properties.
- Constraint syntax `name:: type: value`; whitespace after `::` and `:` is
  mandatory (`constraint-spacing`).

```piton
count:: number: 42
tags:: string[]: [foo, bar]
config:: dictionary:
    a:: number: 1
x:: simple: 1
y:: complex: [1]
z:: any: 1
```

- `simple`, `complex`, `any` as in 3.1. `T[]` is a list of `T` (including
  anchors, `Foo[]`). `extends Foo[]`/`extends Foo` accepts anything inheriting
  from `Foo` (8.4); the spec uses it on exported lists such as
  `export ARITHMETIC_OPERATORS:: extends ArithmeticOperator[]: ...`.
- An anchor-typed constraint (`f:: Foo: {Foo}`) accepts Foo or a copy of it.
- Naming an abstract as a type requires `from ./Foo import Foo`; `use` alone
  isn't enough.

**Multiple constraints** are tried left to right; the first type that can
represent the value wins. `a:: number:: string: 42` → `42`;
`b:: string:: number: 42` → `"42"`. A literal coerced to string keeps its text
(`string: 1.0` → `"1.0"`); an expression is evaluated first
(`string: {1.0}` → `"1"`). **Booleans and null are never coerced**
(`flag:: string: false` is `type-mismatch`; write `${false}`). Collections and
anchors are never coerced. No match → `type-mismatch`.

**Required vs optional** (abstract anchors):

```piton
abstract anchor Card:
    title:: string
    subtitle:: string:: null
    footer:: string:: null: null
```

No value = required (`unimplemented-property` if an implementer omits it), even
when null is allowed (`subtitle` must still be written). A default makes it
optional (`footer`); the LSP doesn't flag missing optional properties.
**A child cannot redeclare an inherited constraint**, only assign a value
(`redeclared-constraint`).

## 5. Composition lines

A block is built top to bottom. **A line starting `+` or `++` combines
everything above it with that line's value**, using that operator (6.4):

```piton
anchor Base:
    items:
        - A
        - B
anchor Child extends Base:
    items:
        + {super.items}
        - C
```

`Child.items` is `[A, B, C]`; with `- {super.items}` it would be
`[[A, B], C]`. Lists: `+` dedupes keeping the last occurrence, `++` keeps all.
Dictionaries: `+` shallow, `++` deep merge. Strings: `+` joins with nothing,
`++` with `\n` (`Child text.` then `++ {super.d}` → `"Child text.\nBase text."`).
A block of only `+ {x}` equals `{x}`. To prepend inherited prose with a space,
interpolate: `${super.description} and more`.

## 6. Expressions

**Nothing is evaluated outside braces**: `a + b` is text, `{a + b}` computes.

- Bare words inside braces are symbols; quote strings, including in list
  literals (`{["A"] + ["B"]}`). Unknown names are `unresolved-symbol`.
- Keep the whole expression in the braces: `{this.flag} && false` is text
  around an expression.
- Invalid or empty braces (`{}`, `${}`) are errors; escape literal braces.
- Forward references resolve. Unresolvable cycles (`A: {B}`, `B: {A}`) are
  `cyclic-value`.
- Collections are copied by value. **Anchors are copied but keep identity**:
  if `x: {Foo}`, then `{x == Foo}` is true, `${x}` is `"Foo"`, and `x` passes a
  `Foo` constraint. Use `@{Foo}` for a link instead of a copy.

### 6.1 Forms and text

| Form | Result |
| --- | --- |
| `{e}` | Intrinsic value, type preserved; an anchor serializes as its content. |
| `${e}` | String. |
| `#{e}` | Number. |
| `@{e}` | Reference: renders as a link/path, never content. |

`@{X}` points at a document, `{X}` embeds a copy, `${X}` names it.

A value that is exactly one expression keeps its type. With surrounding text:
`${x}` makes the whole value a string; `{x}` with a simple value is spliced in
(`Total: {a + b}` → `"Total: 3"`); `{x}` with a list, dictionary or anchor
**turns the value into an implicit list** (`See {Foo} here` →
`["See", <Foo copy>, "here"]`); `@{x}` inserts a link or path.

### 6.2 Conversions

`${}`: strings unchanged; numbers and booleans to text; null → `null`. An
anchor → its source name (`${Child}` → `"Child"`; `${self}` → current
most-derived anchor's name; never title-cased). A named list/dictionary → its
name (`Anchor.prop`, or the variable name). An unnamed one (`${[1, 2]}`) is
`no-string-form`. **`${}` never inlines contents.** Never re-parsed as source.

With `export anchor Metadata:` (`name: Piton`, `version:: string: 1.0`):
`Info ${Metadata}` → `"Info Metadata"`; `Info {Metadata}` →
`["Info", {"name": "Piton", "version": "1.0"}]`; `Info @{Metadata}` →
`"Info ./Metadata.json:Metadata"` (Markdown: `[Metadata](./Metadata.md#metadata)`).

`#{}`: numbers unchanged; a string converts only if the **whole** string is a
Piton numeric literal (`#{"42"}` → `42`, never evaluated). Booleans, null,
collections and anchors are `not-a-number`. Stays numeric in structured output.

### 6.3 Operators

| Group | Ops | Rules |
| --- | --- | --- |
| Access | `.` | Dictionaries and anchors. |
| Arithmetic | `+ - * / %` | Numbers only; divide/modulo by zero errors; `%` keeps the left sign (`{-7 % 3}` → `-1`). |
| Comparison | `== != < <= > >=` | `==`/`!=` on all types; ordering on numbers or on strings (code point); mixing is an error. |
| Logical | `&& \|\| !` | Booleans only, **no truthiness** (`invalid-operand`), **no short-circuit**: both sides evaluated (`{true \|\| 5}` errors). |
| Conditional | `c ? a : b` | Boolean condition (`invalid-condition`); only the chosen branch is evaluated; chains either way. |
| Merge | `+`, `++` | See below. |

`+` dispatch: number+number adds; string with a simple value concatenates
(stringified by `${}` rules: `{2 + "Hello"}` → `"2Hello"`); string with a
list/dictionary/anchor (either side) makes an implicit list (never
stringified: `{"Tags: " + tags}` → `["Tags: ", [...]]`); list+list merges,
deduped keeping last (`{["A","B","C","D"] + ["A","B","C"]}` → `[D, A, B, C]`);
dict+dict shallow merge, right wins. Anything else errors (`{1 + true}`,
`{null + 1}`, `{1 ++ 2}` are `invalid-operand`).

`++`: lists concatenate keeping duplicates (`[A, B, C, D, A, B, C]`);
dictionaries deep-merge (nested dicts merged, otherwise right wins); strings
join with `\n`. (`+` is set-like, `++` list-like; spec
`ConcatenationOperators.setsAndLists`.)

Equality: collections structurally; anchors only if the same anchor;
references if they point at the same thing.

Precedence, high to low (left-assoc, ternary right-assoc; parentheses
override): `.` › `!` › `* / %` › `+ - ++` › `< <= > >=` › `== !=` › `&&` ›
`||` › `? :`.

`:` and `::` are documented as declaration operators
(`spec/scope/language/operators/declaration/`) but are not expression
operators.

### 6.4 References `@{}`

- Must name an anchor or **a property on one** (`@{Button}`,
  `@{Button.color}`); anything else is `invalid-reference`. A top-level list or
  dictionary can't be referenced; put it in an anchor.
- The target (or the anchor owning the property) joins the dependency graph
  and is emitted.
- Resolved per output target; a target that can't represent it errors. A
  reference never degrades to a copy or a name. Comparable with `==`/`!=`.

| Renderer | `Button` | `Button.color` |
| --- | --- | --- |
| JSON | `"./Button.json:Button"` | `"./Button.json:Button.color"` |
| YAML | `./Button.yaml:Button` | `./Button.yaml:Button.color` |
| Markdown | `[Button](./Button.md#button)` | `[Button.color](./Button.md#color)` |

Markdown links are **lazy**: content is not inlined; an agent follows them only
when relevant.

## 7. Modules

```piton
from ./FirstFile import pi, MyAnchor
from ./FirstFile import pi SliceOf, MyAnchor Alias
from /subdir/file import AnAnchor
from .. import Something
from my-package import MyAnchor
from @piton/belay import ClaudeCodeAdapter
from ./file import
    First,
    Second
```

- Paths are relative to the file; `/` is relative to the config `root`. `.pi`
  is optional (the formatter removes it). A directory or bare `.`/`..` means its
  `index.pi`.
- Only exports import (`unresolved-export`); missing modules are
  `unresolved-module`. Aliases are `Name Alias` (no `as`); only the alias is in
  scope.
- `export Name` re-exports something declared or imported. In an index, the
  short forms are `from ./X export X`, `from ./X export *`,
  `from ./X export A AliasA, B`. **Importing into an index does not re-export.**
- **Circular imports are allowed**; only values that can't settle
  (`A: {B}`/`B: {A}`) and self-inheritance are errors. Two anchors describing
  each other with `${}` is fine.
- **`use` brings only user-defined keywords** into scope
  (`use ./Keywords`, `use .`, `use my-package/X`, `use @piton/belay`);
  `import` never does. Missing `use` → `unknown-keyword`. Declaring with a
  keyword and naming its abstract as a type needs both `use` and `import`.
- Formatter: per run of import lines, `use` first, then `from`, sorted by path,
  names sorted; a blank line separates runs. Lines with more than 2 items or
  over 80 columns are broken across lines.

## 8. Anchors

An anchor is a named declaration taking part in inheritance, typing, exports
and composition: like a class never instantiated. It compiles to an object of
its resolved properties; keys follow dictionary rules.

### 8.1 Inheritance

`anchor C extends A, B:` copies each base's properties, then the child's.
**Right-most base wins; the child beats all bases.** Constraints follow the same
rule. Structural, not polymorphic. Errors: `inheritance-cycle`, `invalid-base`
(not an anchor), `unresolved-base`.

### 8.2 `super`, `self`, `this`

- **`super`** is the merged inheritance (last wins), so it finds a property on
  an earlier base if the last lacks it. Missing everywhere is an error.
- **`self`** is the most-derived anchor and travels down: `${self.name}` in a
  base shows the child's name when compiled for the child.
- **`this`** is the lexically enclosing anchor: `${this.name}` in a base always
  shows the base's value; an override's `this` is the child.

```piton
anchor Base:
    name: Base
    a: ${this.name}
anchor Mid extends Base:
    name: Mid
    a: over ${this.name}
    b: ${self.name}
anchor Final extends Mid:
    name: Final
```

`Final` → `{"name": "Final", "a": "over Mid", "b": "Final"}`.

- `self` travels only through inheritance: reading `${Other.note}` evaluates
  with `self` = `Other`.
- `this`/`self` always mean an anchor, even inside a nested dictionary. Using
  them or `super` outside an anchor: `invalid-this`, `invalid-self`,
  `invalid-super`.

### 8.3 Abstract anchors

```piton
abstract anchor Construct:
    description:: string
abstract anchor Skill extends Construct as skill:
    useWhen:: string
skill Review:
    description: Reviews code
    useWhen: Asked for a review
```

- Never compiled or emitted. Concrete implementers must fill every required
  property across the whole abstract chain.
- An unimplemented, unexported abstract warns (`unimplemented-abstract`).
- An anchor may implement several abstracts and extend concrete anchors.
  Disjoint constraints on one property (string vs number) are
  `conflicting-abstracts`; overlapping ones resolve right-most wins.
- Export abstracts with a keyword; the keyword's anchor goes last and wins.
- Type constraints: `A[]` matches only anchors **directly** implementing `A`
  (for multiple abstracts, the winning one: the keyword's, else right-most);
  `extends A[]` matches any concrete anchor with `A` in its chain.
- Extra properties beyond the abstract's shape are kept (Belay serializes them).

### 8.4 Keywords

```piton
anchor MyAnchor as my-anchor:
    description: Mine
my-anchor Child extends Other:
    description: ${super.description} more
```

`my-anchor Child:` = `anchor Child extends MyAnchor:`;
`my-anchor Child extends Other:` = `extends Other, MyAnchor` — **the keyword's
anchor is last and wins.** One keyword per anchor; keywords can be built on
keywords (`export abstract operator ArithmeticOperator as arithmetic-operator:`).
The file must `use` the exporting module.

## 9. Compilation

- **`piton compile <file>`**: one object, one key per export. Unexported names
  and abstracts are omitted (spec JSON examples omit the wrapper).
- **`piton build`**: compiles everything the entry exports, plus whatever those
  reference; nothing else. Abstracts never compile.
- Renderers: `json` (default), `yaml`, `markdown`. Belay adapters build on the
  Markdown renderer.

**Markdown serialization** (`spec/scope/belay/Compilation.pi`,
`MarkdownSerialization`):

- Anchor and property names become title headings: `myProperty` → `My
  Property`, `t1` → `T 1`, capital runs unsplit (`whatIsAType` →
  `What Is AType`). Depth follows nesting; bold labels past level 6.
- **Every anchor property gets a heading**, even `false` or `42`.
- A **pure dictionary** (all values simple or pure dictionaries — no lists or
  mixed content anywhere) is written verbatim in a code fence, keys not
  headings, even when embedded. **Strings count as simple, even long prose.**
  To get per-key headings, start the block with a prose line or include a
  list. Impure dictionaries and mixed content get a heading per key.
- Lists become nested Markdown lists; mixed lists keep order; primitives are
  their text. `{Other}` is a copy; only `@{Other}` links.
- Intrinsic values, strings and references stay distinct; reference identity
  is never inferred from a heading.

## 10. `piton.config.pi`

Read from the working directory; anchors come from bundled `@piton/config`. It
exports its `piton-config` anchor; the LSP treats it as config.

```piton
use @piton/config
use @piton/belay
use @piton/packaging
from @piton/belay import ClaudeCodeAdapter

export piton-config Config:
    root: ./spec
    entry: ./spec/index.pi
    output: ./dist
    renderer: json
    frameworks:
        - {BelayConfiguration}
    packages:
        - {MyPackage}
    dependencies:
        - https://github.com/org/repo
            tag: 1.0

belay-config BelayConfiguration:
    codeRoot: ./src
    shapeRoot: ./spec/shape
    adapters:
        - {ClaudeCodeAdapter}

piton-package MyPackage:
    name: my-package
    root: ./spec/pkg
```

| Field | Meaning |
| --- | --- |
| `root` | Required source root; `/` imports resolve here. |
| `entry` | Default `index.pi` in `root` (error if absent). |
| `output` | Build output, default `./dist`. With a framework, renderer output is written only if `output` or `renderer` is set. |
| `renderer` | `json` (default), `yaml`, `markdown`. |
| `frameworks` | Framework configs (Belay's `belay-config`). |
| `packages` | Packages this repo **offers** (12). |
| `dependencies` | Packages this project installs (12). |

All optional fields are `:: <type>:: null: null` in `PitonConfig`;
`FrameworkConfig` (`framework-config`) is an empty abstract; `PitonPackage`
(`@piton/packaging`) has `name` (optional), `root` (required),
`dependencies` (optional).

A bundled framework's keywords and modules are usable without registering it
(each file still `use`s/imports what it needs); **registering in `frameworks:`
turns on its output.** Belay is the only framework.

Config diagnostics: `missing-config`, `unknown-config-key`,
`unknown-framework`, `unknown-adapter`, `invalid-adapter`,
`package-without-root`, `invalid-package-name`, `multiple-pins`.

## 11. Belay

Bundled as `@piton/belay`: a vocabulary for agents compiled into what agentic
tools read. Piton does parsing, evaluation, modules, inheritance, reachability
and rendering; Belay adds constructs and per-platform adapters; the platform
executes (compilation ≠ correct behaviour). The compiler loads the constructs
and adapters from the spec's own `.pi` files (`spec/scope/belay/anchors/`,
`spec/scope/belay/adapters/`), so editing `Skill.pi` changes the framework.

### 11.1 `belay-config`

| Field | Meaning |
| --- | --- |
| `codeRoot` | Required. App code location. |
| `shapeRoot` | Optional. Shape instructions; unset means `BELAY_SHAPE_ROOT` is the project root and nothing is placed by shape. |
| `adapters` | Required. `ClaudeCodeAdapter`, `CodexAdapter`, `OpenCodeAdapter`. |
| `crossDiscovery` | Optional. `separate` (each tool deployed apart, each writes a full tree) or `allow` (accept OpenCode seeing other tools' command skills). Unset: one shared project (11.5). |
| `instructionByteLimit` | Optional number, checked for targets with a limit. |

Paths are relative to the project root.

### 11.2 Constructs

All extend abstract `Construct` (`description`, `prompt`, both
`:: string:: null: null`). Other properties are kept and rendered as Markdown
sections after the prompt.

| Keyword | Extra required | Meaning / output |
| --- | --- | --- |
| `instruction` | — | Scoped guidance; **placement is meaning**. Mapped from `shapeRoot` to `codeRoot` (11.3); same-scope instructions combine into one file per target; also kept in the shape-reference tree. |
| `skill` | `useWhen:: string` | Selectively loaded. Discovery description = `<description> Use when <useWhen>`; body = prompt then extras. |
| `command` | — | Explicit entrypoint, named `x-<normalized name>`. Native or translated per adapter. |
| `agent` | `role:: string` | Kebab-case name; body starts `You are a <role>`, then prompt, extras. |

- Required fields are checked on concrete constructs after inheritance.
- Missing description/prompt are omitted, but a target that needs a
  description reports `missing-description` (Claude Code: agents; Codex:
  skills, commands, agents; OpenCode: skills, agents).
- Role and skill description are literal: `role: careful reviewer` →
  `You are a careful reviewer`; `Reviews code.` + `asked for a review` →
  `Reviews code. Use when asked for a review`.

```piton
use @piton/belay

export skill AnswerSpecQuestions:
    description: Reads the spec and answers questions about it.
    useWhen: asked how part of the specification works
    prompt: Read the spec under spec/ before answering.
    reportFormat: Save every report as a standalone HTML file.
```

(`reportFormat` renders as `# Report Format`.)

**Native options** are metadata, never body text: `tools`, `model`,
`allowed-tools`, `mode`, `permission`, `agent`, `model_reasoning_effort`,
`sandbox_mode`. Unsupported on a target → `unsupported-option`; bad value →
`invalid-option`. Their typing is an open decision (11.7).

### 11.3 Shape mapping

An instruction's directory relative to `shapeRoot` is resolved under
`codeRoot`; if missing, walk up to the nearest existing directory, **stopping
at `codeRoot`**. `spec/shape/components/button/Button.pi` →
`src/components/button/AGENTS.md`; two files in `input/` combine into one;
`components/nonexistent/` falls back to `src/components/AGENTS.md`. The file
has a heading per instruction (`# Button Shape`). Fallback changes placement
only; the shape-reference tree keeps the original path. Instructions outside
`shapeRoot` are an open decision, so the compiler errors
(`instruction-placement-unspecified`). Placement never guarantees initial
loading (11.5).

### 11.4 References and special imports

- Belay emits only what the build emits (entry exports plus references).
- Reached anchors are written to the adapter's `referenceRoot`, **one Markdown
  file per source module** at its path under `root`, each anchor a heading:
  `spec/components/Button.pi` → `.claude/reference/components/Button.md`. This
  tree (with its `shape/` subdir) is not a platform discovery directory; it is
  reached only through links.
- `@{X}` becomes a relative link with fragment
  (`[Button](../../reference/components/Button.md#button)`), lazy, never
  Claude's `@import`. A reference to a skill, command or agent links to that
  construct's own output (e.g. its `SKILL.md`) and is not copied.
- **Special imports** from `@piton/belay`, resolved per adapter and per output
  file as paths relative to the generated file; interpolate with `${...}`:

  | Import | Resolves to |
  | --- | --- |
  | `BELAY_COMPILED_SHAPE` | The adapter's compiled shape tree (`.claude/reference/shape`). |
  | `BELAY_AGENT_ROOT` | `.claude`, `.codex` or `.opencode`. |
  | `BELAY_PROJECT_ROOT` | Where `piton.config.pi` lives. |
  | `BELAY_SHAPE_ROOT` | `shapeRoot`, or the project root. |
  | `BELAY_CODE_ROOT` | `codeRoot`. |

  `prompt: Read ${BELAY_COMPILED_SHAPE} first.` in
  `.claude/skills/designer/SKILL.md` becomes `../../reference/shape`. They
  compose: `export FLUENCY_PROMPT_FILE: ${BELAY_PROJECT_ROOT}/FLUENCY_PROMPT.md`
  (see `spec/agent/`, whose skills and `command CheckSpecForProblems` are
  re-exported from `spec/agent/index.pi`).

### 11.5 Adapters

Each extends abstract `belay-agent-adapter` (`description`, `targetId`,
`instructionFile`, `referenceRoot`) and carries `documentationChecked`
(currently `2026-09-21`), recorded in the manifest's `targets`.

| | Claude Code (`claude-code`) | Codex (`codex`) | OpenCode (`opencode`) |
| --- | --- | --- | --- |
| Instructions | `CLAUDE.md` | `AGENTS.md` | `AGENTS.md` |
| Reference root | `.claude/reference` | `.codex/reference` | `.opencode/reference` |
| Skill | `.claude/skills/<n>/SKILL.md`, frontmatter `name`, `description` (dir = name) | `.agents/skills/<n>/SKILL.md`, same | `.opencode/skills/<n>/SKILL.md`; name 1–64 lowercase alnum with single hyphens, matching dir; description 1–1024 chars |
| Command | `.claude/skills/x-<n>/SKILL.md` with `disable-model-invocation: true`, `/x-<n>` (never also `.claude/commands/`) | `.agents/skills/x-<n>/SKILL.md` + `agents/openai.yaml` with `policy: allow_implicit_invocation: false`, `$x-<n>` | Native `.opencode/commands/x-<n>.md`, `/x-<n>` |
| Agent | `.claude/agents/<n>.md`, `name`, `description`, opt. `tools`, `model` | `.codex/agents/<n>.toml`: `name`, `description`, `developer_instructions` (one TOML string: role, prompt, extras), opt. `model`, `model_reasoning_effort`, `sandbox_mode` | `.opencode/agents/<n>.md`: `description`, `mode` (`primary`/`subagent`/`all`, always explicit, default `subagent`), opt. `model`, `permission` |
| Native options | agent `tools`, `model`; skill/command `allowed-tools`, `model` | agent only: `model`, `model_reasoning_effort`, `sandbox_mode` | agent `mode`, `model`, `permission`; command `agent`, `model` |
| Loading | Ancestors at startup; nested when Claude reads that subtree | Root-to-cwd chain at start; same-dir `AGENTS.override.md` shadows (`shadowed-instruction`); byte limit (`instruction-over-budget`) | Searches up from cwd, prefers `AGENTS.md` over `CLAUDE.md`; nested files not assumed loaded (`unguaranteed-instruction-scope`); also discovers `.claude/skills` and `.agents/skills` |

Rules for all adapters:

- Pipeline: take validated constructs (after imports and inheritance) → one
  plan per adapter → resolve placement and reference destinations → check
  shared paths and cross-discovery over the whole plan → serialize → validate
  links, metadata, names and ownership → write.
- Distinguish native, translated and unsupported; fail with an actionable
  diagnostic when required behaviour can't be represented. Never silently turn
  an agent into a skill, or a command into an auto-invoked skill.
- Model/permission settings only when explicitly configured; absent ones are
  omitted. Options are validated against the platform; prose never goes into
  metadata.
- **A prompt requesting restraint is not enforcement.** Claim enforcement only
  for platform controls; never widen access in translation. Claude's
  `allowed-tools` and subagent `tools` differ and neither is a portable
  sandbox. An unrepresentable enforced restriction is a diagnostic.
- Names normalized before planning; commands keep `x-`. Collisions are
  `name-collision`/`output-collision` unless identical in content, references
  and activation.
- **Multiple adapters share one project**: the first adapter listed owns the
  single reference tree and all link into it; a shared instruction path
  (`AGENTS.md`) is written once, by the first listed; OpenCode writes no copy
  of a skill Claude Code or Codex already writes where it looks, linking to
  theirs; a command's `x-` skill OpenCode would discover is
  `cross-discovery-activation` unless `opencode.json` denies it
  (`"permission": {"skill": {"x-*": "deny"}}`; last matching pattern wins) or
  `crossDiscovery` is set; a skill OpenCode finds twice (all three enabled) is
  a `cross-target-discovery` warning. With `crossDiscovery: separate`, each
  writes a full tree and incompatible writes to one path are
  `output-collision`.
- Generated links are relative and must hit planned outputs
  (`broken-reference-link`); hand-written Markdown links aren't checked.
- Metadata and body emitted once; frontmatter via a real YAML/TOML encoder;
  filenames and metadata checked against the target schema before writing.

Specifics:

- **Claude Code**: agent identity from the normalized name, never a heading;
  `tools`/`allowed-tools` never copied elsewhere without a mapping.
- **Codex**: never write `AGENTS.override.md`; guidance below the start
  directory isn't guaranteed; limit is `instructionByteLimit`; no deprecated
  custom prompts or invented `.codex/commands/`; generic `tools` never mapped;
  standalone custom-agent format only; sandbox/approval can't be overridden by
  guidance; tool/model settings on a command are diagnosed; a command must
  produce both files; agent TOML must parse with its three fields.
- **OpenCode**: no assumed nested loading or chains; extra guidance only via
  explicit `instructions` config, never global; required but unguaranteeable
  shape scoping is an error; unrecognized frontmatter enforces nothing;
  `model` is provider-qualified; command argument placeholders are kept;
  `allowed-tools` never enforced on commands; agents use `permission` (not
  deprecated `tools`), mapped only when equivalent; ambiguous conversions
  rejected.

Other diagnostics: `invalid-artifact-name`, `missing-command-prefix`,
`description-too-long`, `output-outside-project`, `unowned-output`,
`duplicated-content`, `unrepresentable-reference`.

### 11.6 Guarantees

Identical inputs and toolchain give identical bytes; combined instructions have
stable order. Frontmatter is quoted/escaped per format. Missing reference
targets and unsupported metadata are reported, not papered over. Generated files
are tracked in `.piton/manifest.json` (with per-target documentation dates), so
cleanup never deletes user files. Failures name the source anchor and property.
Output stays inside its boundaries.

### 11.7 Open decisions (`spec/scope/belay/Decisions.pi`)

Say "unspecified" rather than guess, and raise with the user if your work
depends on one: instructions outside `shapeRoot`; types and per-adapter mapping
of `tools`, `allowed-tools`, `model`; naming (skill filename and command name
normalization, acronym splitting, collisions); whether workflows stay
compositions of the four constructs (don't add a `workflow` keyword); how
platform-specific adapter options are written.

## 12. Packages

Git repositories, **vendored** into the project and committed; nothing is
fetched at compile time.

- **Offering**: `piton-package` anchors listed under `packages:` in the config
  (declared anywhere). `root` is relative to the config. A repo may offer
  several. **A project never installs its own packages.**
- **Names**: `name` (may be kebab-case) or else the anchor name. Flat, no `/`;
  `@piton/...` is reserved for bundled packages.
- **Dependencies**: git URLs, each optionally with one indented `commit:`,
  `tag:` or `branch:` (more is `multiple-pins`; also `dangling-pin`,
  `unknown-pin`). No pin = default branch head. Pins are strings (`tag: 1.0` →
  `"1.0"`). Choose some of a repo's packages with `packages: [ui-kit]` under
  the URL, else all.
- Project and package dependencies are separate; **no nesting**. Version
  conflicts: newest commit wins, with a warning.
- **Install** into `tethers/<name>` next to the config, containing the package
  `root`'s files, with all git traces removed. A repo offering no packages
  installs whole under its URL-implied name.
- **Import** by name: `from my-package import X`, `use my-package/X`; missing
  package is an error.
- **Lock**: `.piton/tether.lock` (URL, commit, file hash per package), written
  by `tether` and `update`, committed.
- **Edits**: every operation first compares files with the lock; if edited, it
  stops and suggests `untether` (or `update --force`).

Commands:

- `piton tether [url] [--as name]`: no URL installs all dependencies; a URL is
  added to `dependencies` and installed.
- `piton update [packages...] [--force] [--diff]`: reinstall (all by default).
  `--force` discards edits, warning per package with what changed. `--diff`
  prints each changed file's regions with 3 context lines and old/new line
  numbers (unchanged lines both, removed old only, added new only), a separator
  line between regions, and per package how many files changed (or that none
  did). When refused over edits, it shows those edits: the package re-fetched
  at the locked commit against disk. Computed in-process (no `diff` needed);
  red/green on a terminal.
- `piton untether <pkg> [--as name] [--no-rewrite]`: move to
  `<root>/untethered/` and rewrite imports, making it project source.
- `piton remove <pkg>`: delete and drop from `dependencies`; refuses if edited
  or still imported.

## 13. CLI

Every command prints results, then a ✓/✗ outcome line, then diagnostics with
help and notes and an error/warning count. Coloured with `+`/`-` path marks on a
terminal; piped or `NO_COLOR` gives plain text with bare results on stdout.
`piton --version` (and the version the LSP reports) is major.minor from
`Cargo.toml` with the commit count as patch (e.g. `0.1.57`); `PITON_VERSION`
overrides; without git, the `Cargo.toml` version.

| Command | Behaviour |
| --- | --- |
| `check [paths...]` | Validates syntax, imports, references, types, inheritance, composition, exports, cycles. Writes nothing; exit 1 on errors. Paths are files, dirs or globs; default is the project (or all `.pi` under cwd with no config). |
| `build [config] [--dry-run]` | Writes `<output>/<path under root>.<ext>` for the entry and every file holding something its exports reference. A registered framework's output replaces it unless `output` or `renderer` is set (then both). Validates the whole plan first; records files in `.piton/manifest.json`. |
| `compile <path> [--renderer json\|yaml\|markdown] [--write] [--dependencies]` | One file to stdout, a key per export. `--write` writes next to each input (required for globs). `--dependencies` wraps as `{"value": "<output string>", "dependencies": [abs paths]}`, bundled files excluded. |
| `format [path] [--check]` | Canonical formatting (4 spaces, constraint spacing, sorted/wrapped imports, no `.pi`, space after `//`). `format -` filters stdin. **Only splits long lines, never rejoins paragraphs**: rewrap edited prose by hand, and don't reflow `key: value` blocks as prose. |
| `reach [targets...] [--no-unreachable] [--no-paths]` | From files, globs or anchor names (default entry), follows imports, references, inheritance and composition; reports reachable items with depth and path, and unreachable ones. Finds dead spec. Always exits 0. |
| `slice <target> [--adapter <id>]` | Prints the spec one thing depends on as one Markdown document. Target: `file.pi#Anchor`, `#Anchor.property`, `#variable`, or a bare project-unique name. Follows bases (and every base declaring the property), constraint types, values read, references and embeds, transitively; imports alone add nothing. A property slice omits the anchor's other properties. Adds the **chain** from the entry to the target (shortest build path, one property per link; none for entry exports, variables, unreached files). Order: target, dependencies nearest first (source order ties), chain outermost first. Each declaration once, with name, file, bases, type and reads; references written as names. Paths relative to cwd; project found upward. `--adapter` cites compiled outputs instead of `.pi` files (the target must be built). Exit 1 on errors. |
| `init [dir] [--template group/name] [--adapter id...] [--list] [--no-tether]` | New project from built-in templates: `crux/project` (preferred, listed first and marked recommended: Belay with https://github.com/piton-lang/crux.git declared, tethered by init and re-exported from the entry), `piton/minimal`, `piton/library`, `belay/application`, `belay/minimal`. A template declaring `dependencies` has them tethered after its files are written (needs the network; on failure the files stay and it exits 1); `--no-tether` skips that. A group with a `.preferred` file comes first. Prompts for name, group, template and adapters (Claude Code checked by default) when interactive; non-interactive needs a full template name. Chosen adapters rewrite the config's import and `adapters`; `.adapters/<id>/` files written only for chosen ones. Templates live in `create-templates/<group>/<name>` with a `.template` description and must check, format and build cleanly. Never overwrites (names conflicts, writes nothing). Paths on stdout, summary on stderr. |
| `loc [paths...]` | Total/code/comment/blank lines per file plus summary. Exit 0. |
| `lsp` | Language server over stdio (14). |
| `agent [claude] [--print-fluency] [args...]` | Launches `claude` with a project primer plus this prompt (project `FLUENCY_PROMPT.md` or the built-in copy) as `--append-system-prompt`; extra args pass through. `--print-fluency` only prints it. |
| `tether`, `update`, `untether`, `remove` | Section 12. |

Each command is a `cli-command` anchor (`spec/scope/tooling/cli/index.pi`) with
`commandName`, `positionalArguments`, `namedArguments`.

## 14. Language server, editors, JavaScript

`piton lsp` provides diagnostics (Piton and Belay; not for missing optional
properties), completion, hover (type, source, docs, inheritance chain, export
status, compiled value), go-to-definition, find-references, workspace rename,
auto-import and import organization (unused, duplicate, invalid, overly broad),
code actions (import symbol, create anchor, add export, qualify reference,
resolve simple inheritance conflict), document/workspace symbols, semantic
highlighting, inlay hints (types, inherited origins, composition sources),
formatting, folding, selection ranges, hierarchy and provenance views,
compiled-output preview, source-to-output mapping, unused/redundant detection,
cycle detection (self-inheritance shown; circular imports not reported), and
incremental indexing. No signature help.

Completion: never suggest things that don't exist; nothing on an empty value
(`property: `) or after `.` in a string; **each thing once** — a re-exported
name is offered once, imported from the shortest path, while distinct
same-named anchors (e.g. a project copy and the bundled one) are each offered,
naming their module.

Editors must: indent one level after Enter on a colon opening a block; dedent
one level on Enter from a blank line in a block; optionally format on save.
Supported (under `editors/`): VS Code (TextMate), Emacs, Helix, Zed
(tree-sitter), Neovim/Vim (vim syntax), JetBrains, Sublime, Kate; all with full
LSP.

JavaScript: `packages/vite-plugin-piton` imports `.pi` as modules (HMR,
dependency tracking via `compile --dependencies`, virtual modules); each export
is a named export and the default export is the whole file. Default renderer is
configurable (`'json' | 'yaml' | 'markdown'`); renderers are also importable
from `vite-plugin-piton/renderers`. `packages/astro-piton` wraps it for Astro.

```ts
import { SaveButton } from '../spec/app.pi';
import spec from '../spec/app.pi';
```

## 15. Working on a specbase

1. **Orient**: read `piton.config.pi` (root, entry, output, frameworks,
   adapters, codeRoot, shapeRoot), then the entry (usually `spec/index.pi`;
   here it exports `Specification` — FoundingThesis, Language, Tooling, Belay,
   PackageManagement — plus `spec/agent/`), then down through `index.pi` files.
   Compiled reference is for overview; source is truth. `piton slice` gives a
   focused view.
2. **Edit `.pi`** following local conventions: one exported anchor per file,
   named after it; indexes use `from ./X export *`; abstracts exported with a
   keyword, shared ones in `lib/`; lowerCamelCase properties; SCREAMING_CASE
   exported constant lists; prose in string blocks, structure in keys/lists;
   `@{X}` to link; fenced examples wrapped in `\\\`.
3. **Imports**: `use` for keywords, `import` for names (often both). A new file
   compiles only if the entry reaches it.
4. **Validate**: `piton check`; `piton compile` each edited spec example
   (escape blocks aren't checked) against its ` ```json ` block, remembering
   only exports appear; `piton build` and read the generated diff;
   `piton reach`; `piton format --check`.
5. **Never hand-edit generated files**: `CLAUDE.md`/`AGENTS.md` under
   `codeRoot`, `.claude/`, `.codex/`, `.agents/`, `.opencode/`, build output
   listed in `.piton/manifest.json`, and `.piton/` itself.
6. **Spec and implementation must agree.** When they don't, report it rather
   than bending the spec; trust `piton compile` for current behaviour.

## 16. Compiler vs spec

- Extra CLI features the spec doesn't name: `build --dry-run`, `tether --as`,
  agent argument passthrough, `reach` flags, `format` with no path or `-`.
- Paragraph breaks compile to a single `\n`, which Markdown renders as a soft
  break within one paragraph.

Limitations where the spec is silent:

- A `/`-rooted import inside an installed package resolves against the
  consumer's root; use relative imports in packages.
- A required abstract property is not satisfied by a value from a concrete base
  (the keyword's abstract comes last and its value-less declaration wins); set
  it on the anchor itself.
- `piton check` with no paths only checks what the entry reaches; name
  unreached files explicitly.
- **Prose directly in an anchor body (no key) is silently dropped.** Put it
  under a key such as `description:`.
- Mixed-type `==` is just false (`{1 == "1"}`), not an error.
- Which constructs need a `description` per adapter is hardcoded in
  `crates/piton-belay/src/adapter.rs`, not declared in the adapter `.pi` files.
