# Piton Fluency

This is a working reference for Piton, its bundled Belay framework, its project
configuration, its package manager and its tooling. Read it before you read or
change a Piton specbase.

## 1. What Piton is

Piton is a declarative, whitespace-structured language for writing agent
guidance: skills, agents, commands, scoped instructions and reference
documents. It treats prose and structure as equals. A value can be a paragraph
of plain English or a typed, inherited, composed data structure, and usually a
file mixes both.

- There is no runtime. Piton compiles to data (JSON, YAML, Markdown) through
  adapters. There are no functions, no loops and no conditionals other than
  the ternary operator. Every value is immutable.
- The `.pi` source is the source of truth. Compiled Markdown under
  `.claude/reference`, `.claude/skills`, `CLAUDE.md` files and so on is derived
  output. Make lasting changes in the `.pi` files and rebuild. Never hand-edit
  generated artifacts.
- A *specbase* is a Piton project: a `piton.config.pi` file, a source root
  (usually `spec/`), an entry file, and the frameworks and packages it uses.
- The goal of the design is traceability. The spec says what the system is,
  and the implementation is judged against it. Successful compilation says the
  guidance is well-formed. It does not say that an agent will behave correctly.

## 2. Files, whitespace, comments

- Source files use the `.pi` extension.
- Indentation defines structure, as in Python. Tabs or any number of spaces
  are accepted, but a file must be consistent. The canonical style, which
  `piton format` enforces and which cannot be configured, is 4 spaces.
- Comments are line-only and start with `//`. There are no block comments.
  A comment can follow a value: `x: 42 // note`. The formatter puts one space
  after `//`.
- A directory that holds an `index.pi` is a module. You can import from the
  directory path instead of from a file.
- Keywords are all lowercase and may be kebab-case (`arithmetic-operator`).
  Built-in keywords include `anchor`, `abstract`, `extends`, `as`, `export`,
  `from`, `import`, `use`, `true`, `false`, `null`, `self`, `this`, `super`,
  `pass`, and the type names `string`, `number`, `boolean`, `null`, `list`,
  `dictionary`, `any`, `simple` and `complex`. Users can define more keywords
  (section 7.5).

## 3. Declarations and values

A file is a sequence of top-level declarations. Top-level names are
file-scoped variables, and `export` makes them visible to other files.

```piton
myVariable: 42
export pi: 3.14

export anchor MyAnchor:
    description: This is my anchor
```

`key: value` assigns. The space after the colon is required. A value can sit on
the same line or be indented on the lines below:

```piton
sameLine: Hello, World!
nextLine:
    Hello, World!
```

### 3.1 Types

Piton is loosely typed, with optional constraints.

| Type | Notes |
| --- | --- |
| `string` | Unicode text. Unquoted by default. |
| `number` | A single numeric type, with no int/float distinction. A leading `0` is required for decimals (`0.14`). `_` may separate digits (`1_200_000.00`). |
| `boolean` | Lowercase `true` and `false`. |
| `null` | Lowercase `null`, and the only "nothing" value (there is no `undefined`). `null == null`. |
| `list` | An ordered collection. |
| `dictionary` | Nested key/value pairs. |
| anchor | The one user-defined type (section 7). |

Simple types are string, number, boolean and null. Complex types are list,
dictionary and anchor. A reference is a type concept produced by `@{}`
(section 5.4), and the adapter decides how it renders.

### 3.2 Inference

Without a constraint, a value's type is inferred from the whole literal:

| Source | Inferred as |
| --- | --- |
| `true` | boolean |
| `true story` | string |
| `42` | number |
| `42 things` | string |
| `null` | null |
| `This costs $5 + tax` | string. Operators outside `{}` are plain text. |
| `\// Just Text` or `"// Also Just Text"` | string |
| `${}` | string (empty) |

### 3.3 Strings

- Strings are unquoted. Leading indentation belongs to the syntax, not the
  string.
- In a multi-line block, consecutive lines join with a single space. A blank
  line produces a line break (`\n`).

  ```piton
  myString:
      This broken string is not considered
      a line break.

      This *is* on a new line.
  ```

  This compiles to `"This broken string is not considered a line break.\nThis *is* on a new line."`.
- Double quotes make a value explicitly a string. A quoted value is never
  coerced (`"false"` stays a string) and can hold text that would otherwise
  parse, such as `"// not a comment"`. A line that is entirely one `"..."`
  loses its quotes in output. A quote pair that crosses a line break is a
  syntax error.
- Fenced code blocks (```` ``` ````) inside a string are kept verbatim,
  newlines included, and are not escaped. Markdown tables need a fence,
  because otherwise their rows join into one line. The Piton spec itself wraps
  example code in a fence and an escape block, `\\\ ... \\\`, so the example
  is not parsed.

### 3.4 Escaping

Wrap special characters in backslashes: `\ {1 + 2} \` compiles to the literal
`{1 + 2}`. Stack backslashes to escape backslashes: `\\ \ {x} \ \\` compiles to
`\ {x} \`. Escape blocks may span several lines.

Prose pitfalls. Each one silently changes structure.

- A line that begins `- ` or `+ ` inside a string block starts a list or a
  merge. Write `\- item`.
- A line shaped like `word:` (`Similarly:`, `Inputs: none`, or a wrapped line
  that happens to begin `type:`) becomes a dictionary key, and the block turns
  into a mixed list. Escape it as `Similarly\:` or rewrap the paragraph.
- Backticks do not protect sigils. `` `{x}` `` is still an expression. Write
  `\{x}` or `$\{x}`.
- A reserved word used as a key (`use:`, `from:`) becomes a prose line, not a
  property. Choose another key name.

### 3.5 Lists

```piton
markdownStyle:
    - One
    - Item
    - Per line

inlineStyle: [This, is, a, list]

nested:
    - Level 1
        - Level 2
            - Level 3
// equivalent to [Level 1, [Level 2, [Level 3]]]
```

Items cannot be accessed by index. By design, `myList[0]` is not syntax, and
`{myList[0]}` is left as literal text. Lists exist to be merged and inherited.

### 3.6 Dictionaries

```piton
firstLevel:
    secondLevel:
        thirdLevel: This is a string
```

- Dot access works: `{firstLevel.secondLevel.thirdLevel}`.
- A key may contain Unicode letters and digits, `-` and `_`, but no spaces.
  Anything that coerces to a string is a valid key, so `123`, `foo-bar`,
  `false` and `null` are all keys, stored as strings.
- A block has only one way to define a dictionary: indentation.

### 3.7 Implicit mixed lists

When one block mixes prose, list items and keys, it becomes an implicit list
that preserves order:

```piton
combined:
    This is a string

    - This
    - Is

    nested:
        deep: value
```

This compiles to `["This is a string", ["This", "Is"], {"nested": {"deep": "value"}}]`.
Keys declared directly in the mixed block stay addressable
(`combined.nested.deep` works). The prose and the list items are not
addressable. A dictionary inside an explicit list item (`- key:` followed by
nested lines) cannot be addressed at all.

## 4. Type constraints

Declare a constraint with `name:: type: value`. The whitespace after `::` and
after `:` is mandatory, so `x::number:42` is an error.

```piton
count:: number: 42
tags:: string[]: [a, b]
config:: dictionary:
    a:: number: 1
    b:: string: foo
anything:: any: ...
scalar:: simple: 1          // string, number, boolean or null
structured:: complex: [1]   // list, dictionary or anchor
```

A constraint with no value (`description:: string`) declares a required
property. This is how abstract anchors define their shape.

- **Multiple constraints.** `x:: number:: string: 42`. The value takes the
  first constraint, reading left to right, that it can validly represent.
  `x:: string:: number: 42` gives the string `"42"`.
- **Coercion is limited.** Unquoted literals coerce when their syntax fits the
  target type. Quoted values never coerce. Lists, dictionaries, anchors,
  booleans and null never coerce into unrelated types. If no constraint
  accepts the value, compilation fails with `type-mismatch`.
- **Nullable default.** Put `null` first: `name:: null:: string: null`. If you
  write `string:: null: null`, the string constraint wins and the value
  becomes the string `"null"`. The bundled packages do write
  `:: string:: null: null`. Treat such a property as "optional" and set a real
  value.
- **Anchor-typed lists.** `items:: A[]` accepts anchors that directly
  implement `A`. `items:: extends A[]` accepts any anchor whose inheritance
  chain includes `A`. The `extends` form is meant for abstract definitions,
  but it appears on exported lists too, as in
  `export OPS:: extends Operator[]: ...`.

## 5. Expressions

A value is plain text unless it is wrapped in an expression sigil. `a + b`
compiles to the string `"a + b"`, while `{a + b}` compiles to `3`. Inside the
braces, bare words are symbols, so a string operand must be quoted
(`{2 + "Hello"}` gives `"2Hello"`). An unknown name is an `unresolved-symbol`
error, never a string fallback.

| Form | Name | Result |
| --- | --- | --- |
| `{expr}` | Standard | The intrinsic value, keeping its type. An anchor serializes as its resolved content, a copy. |
| `${expr}` | String | Converts to a string and can be embedded in prose. |
| `#{expr}` | Numeric | Converts to a number. The whole string must be a valid numeric literal, and booleans, null, collections and anchors are rejected. |
| `@{expr}` | Reference | Keeps anchor identity. The adapter renders it as a link, not as the content. |

Expressions resolve forward references. Unresolved references are errors, and
so are cyclic values (`A: {B}` with `B: {A}` gives `cyclic-value`). Circular
imports are allowed, and so are mutually referencing anchors, as long as every
value settles. `anchor A: talks about ${B}` together with `anchor B: talks about ${A}`
is fine, because both settle to strings.

### 5.1 Stringification rules for `${}`

- Numbers become their text form. Booleans become `true` or `false`. Null
  becomes `null`.
- Lists become a string representation.
- A dictionary becomes its variable or property name. An anchor becomes its
  name (`${Base}` gives `"Base"`).
- The result is never re-parsed as source.

### 5.2 Operators

| Group | Operators |
| --- | --- |
| Access | `.` for property access |
| Arithmetic (numbers only) | `+ - * / %`. `* / %` bind before `+ -`, operators of equal precedence run left to right, and parentheses group. Division by zero is an error. |
| Comparison | `== != < <= > >=` |
| Logical | `&& \|\| !` |
| Concatenation | `+` joins strings. Mixed types become a string, so `{2 + "Hello"}` gives `"2Hello"`. |
| Merge (lists and dictionaries) | `+` merges and removes duplicates. A value that appears more than once keeps its *last* position: `[A, D] + [A, B, C]` gives `[D, A, B, C]`. `++` merges and keeps duplicates. |
| Conditional | `cond ? a : b`, which can be chained. Only the selected branch is evaluated. |

Each type supports only its listed operators, after the coercion and inference
rules apply. Numbers support arithmetic. Strings and booleans support `+`.
Lists and dictionaries support `+` and `++`. Null and anchors support none.
Anything else is a compiler error (`invalid-operand`).

### 5.3 Merge lines inside blocks

A list block can spread another list into itself with a `+` or `++` line:

```piton
anchor Base:
    items:
        - A
        - B
        - C

anchor Child extends Base:
    items:
        + {super.items}
        - D
// Child.items gives [A, B, C, D]. Without the `+`, it would give [[A, B, C], D].
```

A `+` line merges and removes duplicates, and order follows the operands. A
`++` line keeps duplicates. **Do not use `+ {super.x}` to extend a prose
string.** Inside a string block it creates a mixed list. To extend inherited
prose, interpolate instead:

```piton
description:
    ${super.description} and more from the child.
```

### 5.4 Reference expressions `@{}`

- The expression must resolve to a referenceable anchor. Otherwise the
  compiler reports `reference-not-an-anchor`.
- The referenced anchor joins the compilation dependency graph, so it gets
  compiled and emitted.
- Markdown renders a relative link to the anchor's compiled file, using the
  anchor's display name as the link text: `[Button](../../reference/shape/components/button/Button.md)`.
  The link is lazy, which means the referenced content is not inlined. An
  agent follows the link only when the work touches what it describes.
- JSON renders `{"$ref": "file.pi#Anchor"}`.
- Use `@{}` to point at another document. Use `{}` to embed a copy. Use `${}`
  to mention something by name.

## 6. Modules and reuse

```piton
from ./FirstFile import pi, MyAnchor
from ./FirstFile import pi SliceOf, MyAnchor Alias    // alias: only SliceOf is in scope
from /subdir/file import AnAnchor                     // root-relative, via piton.config.pi root
from ./dir import Thing                                // dir/index.pi
from my-package import MyAnchor                        // installed package, by name
from @piton/belay import ClaudeAdapter                 // bundled package
from ./file import
    FirstThing,
    SecondThing
```

- Paths are relative to the current file. A path that starts with `/` is
  relative to the configured `root`. The `.pi` extension is optional.
- Only exported names can be imported. Importing a name that the module does
  not export is `unresolved-export`.
- An `index.pi` re-exports with `from ./X export Name`,
  `from ./X export Name Alias, Other OtherAlias`, or `from ./X export *`. You
  can also import and then write `export Name`. **Importing a name into an
  index does not re-export it.** When the index uses `import`, import leaf
  anchors from their own files.
- **`use`** brings the *keywords* that a module exports into scope, and
  nothing else. `from ... import` never brings keywords. When a file uses a
  keyword and also names the abstract as a type (`:: extends Foo[]` or
  `extends Foo`), it needs both lines: `use ./Foo` and
  `from ./Foo import Foo`.
  - `use ./CustomKeywords`, `use .`, `use ../cli`, `use my-package`,
    `use my-package/MyAnchor` and `use @piton/belay` are all valid.
- Circular imports are allowed.
- `piton format` sorts imports and breaks an import across lines when it has
  more than 2 items or runs past 80 columns.

## 7. Anchors

An anchor is a named structural declaration. It is Piton's building block,
something like a class without instantiation. An anchor can hold properties,
be exported, be inherited, act as a type, and be referenced.

```piton
export anchor MyFirstAnchor:
    stringValue: String types are supported.
    listTypes:
        - List types
    numberTypes: {3.14 - 3.14}
    nullType: null
    both: {this.flag && false}
    flag: true
```

Compiled to JSON, an anchor becomes an object of its resolved properties. A
body with nothing in it is written `pass`.

### 7.1 Structural inheritance

`anchor Child extends A, B:` copies properties from every base, then applies
the child's own. On collision, the **right-most base wins**, and the child
beats all bases. Type constraints collide by the same rule. This is structural
inheritance, not polymorphism.

### 7.2 `super`

`super.prop` reads the inherited value, from the right-most base that defines
it. Use `${super.description}` for prose, and a `+ {super.items}` line for
lists (section 5.3).

### 7.3 `self` and `this`

- `self` means the most-derived anchor, and it travels down the hierarchy. A
  base that writes `${self.name}` shows the child's name when the child is
  compiled.
- `this` is pinned to the anchor that lexically contains the expression. A
  base that writes `${this.name}` always shows the base's own value, including
  when inherited. When a child overrides that property, the override's own
  `this` refers to the child.

```piton
anchor Base:
    name: Base Anchor
    baseDescription: This is ${this.name}   // always "Base Anchor"
    anyDescription: This is ${self.name}    // "Child Anchor" when compiled as Child

anchor Child extends Base:
    name: Child Anchor
```

Using them outside an anchor is an error (`invalid-self`, `invalid-this`,
`invalid-super`). Keep the whole expression inside the braces. The compiler
evaluates `{this.flag && false}`, but `{this.flag} && false` becomes the
string `"true && false"`.

### 7.4 Abstract anchors

```piton
export abstract anchor Skill:
    description:: string       // required: implementers must define it
    useWhen:: string
    notes:: string:: null: null  // has a default, so it's optional

anchor ConcreteSkill extends Skill:
    description: ...
    useWhen: ...
```

- An abstract anchor never compiles by itself. A concrete anchor that extends
  it *implements* it and must define every property without a value.
  Otherwise the compiler reports `unimplemented-property`.
- A concrete anchor implements **at most one** abstract anchor
  (`multiple-abstract-bases`), because Piton does not union abstracts. It may
  extend any number of concrete anchors as well. Conflicting type constraints
  across abstracts are errors.
- Abstracts may extend other abstracts (`abstract anchor B extends A:`).
- Special constraints (`simple`, `complex`, `any`, `[]`, `extends X[]`) are
  all available inside abstract anchors.

### 7.5 User-defined keywords

`as` gives an anchor a keyword alias. Declaring with the keyword is sugar for
`extends`:

```piton
export abstract anchor Skill as skill:
    ...

skill MySkill:                          // = anchor MySkill extends Skill
    ...

skill Mixed extends OtherBase:          // = anchor Mixed extends Skill, OtherBase
    ...
```

The keyword's anchor comes *first* in the chain, so later bases override it on
collision. By convention, abstract anchors are exported with a keyword. A
keyword can also be based on another keyword:
`export abstract operator ArithmeticOperator as arithmetic-operator:`. To
declare with a keyword, a file must `use` the module that exports it.
Otherwise the compiler reports `unknown-keyword`.

## 8. Compilation model

- `piton compile file.pi` compiles one file and prints JSON, including its
  unexported top-level names. `--adapter json|yaml|markdown` picks the format.
  `--write` writes `<file>.<ext>` next to the input, which is required for
  globs. `--dependencies` wraps the result with the list of source files it
  used, for build tools that watch files.
- For a project, **everything exported from the entry file gets compiled**,
  whether or not anything uses it. Only source reachable from the entrypoints
  is compiled (see `piton reach`). An unexported, unreferenced anchor in some
  other file produces no output.
- Markdown serialization, which is used by the Belay reference output:
  - Anchor names and prose-bearing property names become word-separated title
    headings (`myProperty` becomes `## My Property`). Nesting deepens the
    heading level, and bold labels take over past level 6.
  - Primitives become their text form (`false`, `42`, `null`). Explicit lists
    become Markdown lists. Pure dictionaries become indentation-structured
    blocks in code fences. Mixed lists keep the order of prose and structure.
  - An anchor-typed value (`{Other}`) is serialized as a copy of the content.
    Only `@{Other}` keeps identity, as a link.

## 9. Project configuration (`piton.config.pi`)

The compiler looks for `piton.config.pi` in the working directory. The
configuration anchors come from the bundled `@piton/config` package.

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
            tag: 1.0             // or commit: abc / branch: main

belay-config BelayConfiguration:
    codeRoot: ./src              // application source root, required
    shapeRoot: ./spec/shape      // optional architectural shape tree
    adapters:
        - {ClaudeAdapter}

package MyPackage:
    name: my-scope/pkg           // optional; defaults to the anchor name, may contain /
    root: ./spec/pkg
```

- `@piton/config` exports `piton-config` (`root`, `entry`, `frameworks`,
  `packages`, `dependencies`) and `framework-config`.
- A framework in `frameworks` becomes available to the project. Each file
  still has to `use` or `import` what it needs, because registering a
  framework is not an implicit import.
- The language server treats `piton.config.pi` as a project config.

## 10. Belay: the agentic framework

Belay is bundled as `@piton/belay`. It gives Piton an agentic vocabulary and
compiles it into artifacts for agentic coding tools. Piton handles parsing,
evaluation, imports, inheritance and reachability. Belay handles the
constructs and the output. The consuming platform executes the result.

### 10.1 The four constructs

Every construct extends the abstract `Construct`, which requires
`description:: string` and `prompt:: string`. Any other property you add is
kept and serialized, as Markdown, after the prompt.

| Keyword | Extra required fields | Meaning | Output behaviour |
| --- | --- | --- | --- |
| `instruction` | none | Persistent guidance tied to a code scope. Where it is placed is part of what it means. | Mapped from `shapeRoot` to the matching `codeRoot` directory (section 10.2). All instructions for one scope combine into one guidance file. They are also preserved in the shape-reference tree. |
| `skill` | `useWhen:: string` | Instructions the agent loads selectively for a kind of work. | Name derived from the anchor. The discovery description is `description` followed by `Use when` and `useWhen`. The body is `prompt` followed by the extra properties. The platform decides when to load it. |
| `command` | none | An explicit entrypoint. | Name is `x-` plus the normalized name. The Claude and Codex adapters emit a manually invoked skill, and OpenCode emits a native command. |
| `agent` | `role:: string` | A role, and instructions for performing it. | Body starts `You are <role>`, then the prompt and the extra properties. Name is the kebab-case anchor name. |

```piton
use @piton/belay

export skill InspectSpec:
    description: Reads the spec and answers questions.
    useWhen: Explicitly invoked
    prompt:
        Read the spec under spec/ and summarize it.
    reportFormat:
        Save reports as standalone HTML.   // extra property, becomes "# Report Format"
```

### 10.2 Shape mapping (instructions)

The tree under `shapeRoot` roughly mirrors the tree under `codeRoot`.

- An instruction at `spec/shape/components/button/Button.pi` targets
  `src/components/button/`. If that directory does not exist, the compiler
  walks upward to the nearest directory that does, stopping at `codeRoot`, and
  writes the adapter's instruction file there, such as
  `src/components/CLAUDE.md`.
- Instructions that resolve to the same scope combine into one file.
- The original relative structure is kept under
  `<referenceRoot>/shape/...`, even when placement fell back to a parent
  directory.
- Diagnostics: `instruction-outside-shape`, `unplaceable-instruction`.

### 10.3 References and special exports

- Exported non-construct anchors that are reached from the entry (plain
  `anchor`s such as a `Specification` tree) are serialized as Markdown under
  `referenceRoot`, and the source's relative structure is preserved. `@{X}`
  in any generated file becomes a relative link to X's compiled file. Links
  stay lazy, and they are never replaced with eager import syntax.
- Special imports from `@piton/belay`. Each one is resolved at compile time,
  for each adapter, as a path relative to the generated file that uses it.
  - `__BELAY_SHAPE__` is the compiled shape root (`.claude/reference/shape`
    for Claude).
  - `BELAY_AGENT_ROOT` is the agent directory (`.claude`, `.opencode`, and so
    on).
  - `BELAY_PROJECT_ROOT` is the project root.
  - `BELAY_SHAPE_ROOT` is the configured `shapeRoot`, or the project root.
  - `BELAY_CODE_ROOT` is the configured `codeRoot`, or the project root.

  Import them explicitly (`from @piton/belay import __BELAY_SHAPE__`) and
  interpolate them with `${...}`.

### 10.4 Adapters

The adapters exported by `@piton/belay` are `ClaudeAdapter`, `CodexAdapter`
and `OpenCodeAdapter`, all built on the abstract `belay-agent-adapter`
(`targetId`, `instructionFile`, `referenceRoot`, `skillRoot`, `agentRoot`,
`commandSupport`).

| | Claude Code (`claude-code`) | Codex (`codex`) | OpenCode (`opencode`) |
| --- | --- | --- | --- |
| Instruction file | `CLAUDE.md` | `AGENTS.md` | `AGENTS.md` |
| Reference root | `.claude/reference` | `.codex/reference` | `.opencode/reference` |
| Skill | `.claude/skills/<name>/SKILL.md` | `.agents/skills/<name>/SKILL.md` | `.opencode/skills/<name>/SKILL.md` |
| Command | `.claude/skills/x-<name>/SKILL.md` with `disable-model-invocation: true`, invoked as `/x-<name>` | `.agents/skills/x-<name>/SKILL.md` plus `agents/openai.yaml` with `policy.allow_implicit_invocation: false`, invoked as `$x-<name>` | native `.opencode/commands/x-<name>.md`, invoked as `/x-<name>` |
| Agent | `.claude/agents/<name>.md` (YAML frontmatter with `name`, `description`, and optionally `tools`/`model`) | `.codex/agents/<name>.toml` (`name`, `description`, `developer_instructions`, optionally `model`, `model_reasoning_effort`, `sandbox_mode`) | `.opencode/agents/<name>.md` (`description`, `mode`, optionally `model`/`permission`) |

Adapter rules that apply everywhere:

- An adapter emits a platform option (model, tools, permissions) only when you
  configure it explicitly. When an option is absent, the native defaults
  apply.
- Prompt text that asks for restraint is advisory. An adapter claims
  enforcement only where the platform enforces it, and it must never silently
  widen permissions.
- An adapter must not turn an agent into a skill, or a command into an
  automatically invoked skill.
- Names are normalized before planning. Collisions are errors unless the
  outputs are identical (`output-collision`, `invalid-artifact-name`,
  `missing-command-prefix`, `description-too-long`).
- With several adapters configured, the whole plan is checked before anything
  is written. Codex and OpenCode share `AGENTS.md`. OpenCode also discovers
  Claude and `.agents` skills (`cross-target-discovery`).
- Generated links must point at planned outputs (`broken-reference-link`), and
  no output may land outside the project (`output-outside-project`).
- Loading differs by platform. Claude loads ancestor `CLAUDE.md` files at
  startup and nested ones when it reads inside that subtree. Codex builds a
  chain from the root to the working directory when a run starts. OpenCode
  searches upward and prefers `AGENTS.md`. Placing a file does not guarantee
  that it is in the initial context.

### 10.5 Build output

`piton build` writes every planned artifact and records them in
`.piton/manifest.json` (`{"generated": [...]}`), next to the config file. The
manifest lets later builds clean up only the files that Piton generated.

### 10.6 Proposed and open areas

The spec marks some behaviour as *proposed* or *unresolved*:

- Byte-identical, reproducible builds.
- Stable ordering of instructions in shared files.
- Where adapter metadata such as tools or model lives in author-facing source.
- Name-normalization edge cases.
- The final string form of an anchor.
- Whether "workflows" become a construct. The spec says not to add a workflow
  keyword.

Do not present these as settled. If the work depends on one of them, raise it.

## 11. Package management

Packages are git repositories, managed as vendored dependencies.

- A repo publishes packages with `package` anchors from `@piton/packaging`
  (`name:: string:: null` which may contain `/`, `root:: string` which is
  required, and `dependencies:: string[]:: null`). The anchors are listed
  under `packages:` in `piton.config.pi`. One repo can publish several
  packages with different roots.
- A project declares its `dependencies:` as git URLs, each optionally pinned
  with `commit:`, `tag:` or `branch:`. With no pin, the latest commit on the
  default branch is used. Project-level pins are inherited by packages that do
  not set their own. Dependencies do not nest: when two packages need the same
  package, the newest version wins, with a warning.
- `piton tether <git-url>` clones the repo, strips all git metadata, and
  copies what it publishes into `tethers/<name>`, next to `piton.config.pi`.
  The files are committed with the project and read from disk. Nothing is
  fetched at compile time.
- `piton update [packages]` reinstalls at the pinned versions. It refuses when
  the vendored files differ from the locked version, and asks you to untether
  instead.
- `piton untether <pkg> [--as name] [--no-rewrite]` moves a package into
  `<root>/untethered` and rewrites the imports that named it, so that it
  becomes your own source.
- `piton remove <pkg>` deletes a package. It refuses while anything still
  imports the package.
- Import an installed package by name: `from my-package import X`,
  `use my-package`, `use my-package/Sub`. A missing package is a compiler
  error (`unknown-package`).

## 12. CLI reference

| Command | Purpose |
| --- | --- |
| `piton check [paths]` | Validates syntax, imports, references, types, inheritance, composition, exports and circular dependencies. Writes nothing. Exits 1 on errors. |
| `piton build [config]` | Compiles the project and writes the Belay artifacts and the manifest. `--dry-run` reports what would be written. |
| `piton compile <path>` | Compiles one file to stdout (`--adapter json\|yaml\|markdown`, `--write`, `--dependencies`). |
| `piton format [path]` | Applies canonical formatting. `--check` reports without writing. The formatter only splits overflowing lines and never rejoins a paragraph, so rewrap edited prose by hand. Do not reflow `key: value` blocks as prose. |
| `piton reach [targets]` | Reports what is reachable and unreachable from files, anchors or the project, through imports, references, inheritance and composition, with depth and paths. Use it to find dead spec. |
| `piton loc [paths]` | Counts total, code, comment and blank lines per file. |
| `piton lsp` | Language server over stdio: diagnostics, completion, hover, go-to-definition, references, rename, symbols, semantic tokens, inlay hints, code actions and auto-import, formatting, inheritance and provenance views, compiled-output preview, folding. |
| `piton agent <agent>` | Builds the project, refuses to launch on errors, then launches the agent (currently `claude`) with this fluency prompt. `--print-fluency` prints the prompt without building anything. |
| `piton tether`, `update`, `untether`, `remove` | Package management (section 11). |

Editor support: VS Code (TextMate grammar plus the LSP), Neovim, Vim, Emacs,
Helix, Zed, JetBrains, Sublime and Kate. Every editor follows the same rules.
Enter after a trailing `:` indents one level. Enter on a blank line inside a
block dedents one level. Format-on-save is optional and never formats
commented-out lines. Completion never offers anything on an empty value or
inside prose. A Vite plugin imports `.pi` files as modules, with HMR,
dependency tracking, virtual modules and configurable renderers. An Astro
integration wraps the Vite plugin.

## 13. Common diagnostics

`unresolved-symbol`, `unresolved-import`, `unresolved-export`,
`unresolved-module`, `unresolved-base`, `unknown-keyword`, `unknown-property`,
`type-mismatch`, `unimplemented-property`, `multiple-abstract-bases`,
`invalid-base`, `inheritance-cycle`, `cyclic-value`, `duplicate-declaration`,
`invalid-operand`, `invalid-access`, `list-access`, `division-by-zero`,
`not-a-number`, `no-string-form`, `reference-not-an-anchor`,
`invalid-self`, `invalid-this`, `invalid-super`, `non-property-in-anchor`,
`missing-config`, `unknown-framework`, `unknown-adapter`, `unknown-package`,
plus the Belay output diagnostics listed in section 10.4.

## 14. Working on a specbase

1. **Orient.** Open `piton.config.pi` to find `root`, `entry`, the frameworks,
   the adapters, `codeRoot` and `shapeRoot`. Read the entry file, which is
   usually `spec/index.pi`. Its exports are the compilation roots. Follow
   `index.pi` files down through the modules.
2. **Read the compiled reference for overview, and the source for truth.**
   `<referenceRoot>/*.md` is easy to browse, and its links are lazy. Follow
   them only when they are relevant.
3. **Edit the `.pi` source.** Follow the conventions around it: one exported
   anchor per file named after the file, `index.pi` files that re-export with
   `from ./X export *`, abstract anchors exported with a keyword, lowercase
   camelCase property names, prose in string blocks, and structure in keys and
   lists.
4. **Keep imports honest.** Use `use` for keywords and `from ... import` for
   names, and a file often needs both. A new file is compiled only if it is
   reachable from the entry, so export it through an index or reference it.
5. **Validate.** Run `piton check`. `piton check` does **not** compile the
   code inside fenced blocks, so when you edit spec examples, extract each
   ` ```piton ` block and run `piton compile` on it, then compare it with any
   ` ```json ` block that follows. Then run `piton build` and inspect the
   diff of the generated files. Run `piton reach` to confirm that new anchors
   are reached, and `piton format --check`.
6. **Never hand-edit generated files.** That includes `CLAUDE.md` and
   `AGENTS.md` under `codeRoot`, everything under `.claude/`, `.codex/`,
   `.agents/` and `.opencode/` that is listed in `.piton/manifest.json`, and
   the manifest itself.
7. **Keep implementation and spec in agreement.** Implementation is judged
   against the spec. If the two disagree, report it. Do not quietly bend the
   spec to fit the implementation. Where this document notes that the compiler
   differs from the spec prose, trust `piton compile`, and flag the gap.

### Where the compiler differs from the spec prose

`piton compile` output confirms each of these:

- The spec shows `{this.booleanTypes} && false` evaluating to `false`. Only
  an expression that sits entirely inside the braces is evaluated.
- The spec shows `+ {super.description}` followed by prose extending a
  string. In practice it produces a mixed list. Use `${super.description}`.
- The spec shows `2 + Hello` inside an expression. Bare words inside `{}` are
  symbols, so write `"Hello"`.
- The LSP spec lists a `!{...}` expression form. It is not implemented, and
  the only sigils are `{}`, `${}`, `#{}` and `@{}`.
- A duplicate key in one block is accepted without an error. Avoid duplicate
  keys.
