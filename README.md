# Piton

A compiler, language server, and build tool for **Piton**, a declarative language
for writing agent guidance, and **Belay**, the framework that compiles that
guidance into the artifacts agentic coding tools read.

Piton has no runtime. It compiles to data — JSON, YAML, Markdown — through
adapters. The specification in `spec/` is itself written in Piton, so the
compiler's acceptance suite is the language describing itself.

```
piton check      # validate the specbase
piton build      # compile every configured artifact
piton reach      # see what the entrypoints can and cannot see
```

## Building

```
cargo build --release
./target/release/piton --help
```

Requires Rust 1.75 or newer. There are no system dependencies.

## Commands

| Command | What it does |
| --- | --- |
| `piton agent <agent>` | Builds the project, then launches the agent with a primer on how to read it |
| `piton build [config]` | Compiles the project configured by `piton.config.pi`; `--dry-run` reports without writing |
| `piton check [paths]` | Validates syntax, imports, references, types, inheritance, composition, exports, and circular dependencies; exits 1 on any error |
| `piton compile <path>` | Renders one file, or a `--write` glob, through `--adapter json\|yaml\|markdown` |
| `piton format [path]` | Applies canonical formatting; `--check` reports without writing |
| `piton loc [paths]` | Counts total, code, comment, and blank lines per file |
| `piton lsp` | Runs the language server over stdio |
| `piton reach [targets]` | Reports reachable and unreachable anchors, with depth and path |

## Layout

```
crates/piton-core      values, text, diagnostics, naming rules
crates/piton-syntax    lexer, parser, lossless CST, formatter
crates/piton-compile   modules, inheritance, evaluation, types, reachability
crates/piton-emit      JSON, YAML, and Markdown adapters
crates/piton-belay     the Belay framework and its three target adapters
crates/piton-lsp       the language server
crates/piton-cli       the `piton` binary
```

The parser produces a typed AST *and* a lossless rowan tree. The AST is what the
compiler lowers; the rowan tree is what the editor walks, and it reproduces every
byte of the source including comments and trivia. Expression interiors —
everything inside `{...}` — are parsed with chumsky, since that is the only
context-free corner of the grammar; the layout outside them is driven by lines
and indentation.

## The language server

`piton lsp` implements diagnostics, completion, hover, go-to-definition,
find-references, rename, document and workspace symbols, semantic highlighting,
inlay hints, signature help, code actions, formatting, folding, and selection
ranges.

Every one of them answers from the *resolved* program rather than from raw
syntax. Hovering a user keyword finds the anchor it aliases and says so.
Go-to-definition on an inherited property lands on the base that actually
supplies the value. Find-references on an anchor includes the places that reach
it through keyword sugar. Inlay hints show the inferred type and name the base a
value was inherited from or the declaration it overrides.

## Decisions

The specification leaves several questions open, and a few of its statements
disagree with its own examples. Each choice below is implemented as described and
is covered by a test.

**String interpolation of an anchor yields its source name.** `${Adapter}`
renders `Adapter`. The reference output establishes this: `${self}` inside
`ArithmeticOperators` renders `ArithmeticOperators`.

**Only `@{...}` produces a reference document.** A `${...}` is a string, so it
does not create a file and nothing links to one. The reference tree in
`.claude/reference` contains five documents that nothing links to, all of them
anchors named only in `${...}`; this compiler does not generate them.

**A reference to something that is not an anchor warns and falls back to the
value.** The specification says this should be an error, but it also lists
reference identity as unresolved, and the specbase contains three such uses.
Warning keeps the build honest without failing it over an unfinished rule.

**Braces that do not contain a valid expression are text.** Prose can discuss
`{...}` and `${}` without escaping them, and a warning says so. This does not
reintroduce string fallback for unknown symbols: a bare name still parses as an
expression and still fails during resolution.

**Reserved words cannot be property names.** A line such as
`null: The absence of a value` is prose, not a property, which is why such lines
do not appear as headings in the reference output. An anchor body that contains
anything other than properties gets a warning naming what will not be emitted.

**Only fully braced expressions evaluate.** `{a} && false` is the string
`true && false`, following the normative text — "an expression must be wrapped in
curly braces, otherwise it'll be interpreted as a string" — rather than the
worked example in `Anchors.md`, which shows it evaluating to `false`.

**The merge operator keeps the last occurrence.** Merging `[A, B, C]` into
`[A, B, C, D]` yields `[D, A, B, C]`, which is the result the specification
states.

**Word splitting does not break up runs of capitals.** `whatIsAType` renders as
`What Is AType`, matching the reference output. Acronym-aware splitting would
produce a different document.

**Continuation lines are indented.** A multi-line value inside a list item or map
entry is indented so it stays within its item. The reference output leaves such
lines at column zero, which breaks the list it belongs to.

**Instructions outside `shapeRoot` attach to the code root**, with a warning,
since the specification lists their placement as undefined.

## Determinism and safety

A build plans every file for every adapter, validates the whole plan, and only
then writes. The plan checks that generated links point at files the plan will
write, that names normalize to valid identities, that a command keeps its `x-`
prefix, that descriptions fit the target's limits, and that no output escapes the
project root. Identical shared guidance is written once; conflicting writes to
one path are rejected.

Output order is stable and content is compared before writing, so identical
source produces identical bytes. Each build records what it wrote in
`.piton/manifest.json`, and cleanup only removes paths from that manifest — a
file Belay did not write is never deleted.

## Testing

```
cargo test --workspace
```

The suite covers the language semantics against the examples in the
specification, parses the whole specbase and checks the lossless tree reproduces
it byte for byte, compiles it, compares generated artifacts against the
checked-in `.claude` tree, drives the language server's features over the real
sources, and runs the CLI end to end against a fixture project that exercises all
four constructs and all three adapters.
