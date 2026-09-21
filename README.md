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

## Installing

```
cargo xtask install
```

Builds the release binary and copies it into the local cargo bin directory,
saying where it went and whether that directory is on `PATH`. It builds in the
workspace's own target directory rather than shelling out to
`cargo install --path`, which would rebuild every dependency from scratch
somewhere else. The copy goes through a rename, so reinstalling over a binary
that is currently running works.

Pass `--root <dir>` to install into `<dir>/bin` instead, or `--dry-run` to see
what would happen. `cargo xtask` on its own lists the tasks.

## Commands

| Command | What it does |
| --- | --- |
| `piton agent <agent>` | Builds the project, then launches the agent with a primer on how to read it |
| `piton analyze [targets]` | Reports statements that contradict each other; `--explain` shows every piece of evidence |
| `piton build [config]` | Compiles the project configured by `piton.config.pi`; `--dry-run` reports without writing |
| `piton check [paths]` | Validates syntax, imports, references, types, inheritance, composition, exports, and circular dependencies; exits 1 on any error |
| `piton compile <path>` | Renders one file, or a `--write` glob, through `--adapter json\|yaml\|markdown` |
| `piton format [path]` | Applies canonical formatting; `--check` reports without writing |
| `piton loc [paths]` | Counts total, code, comment, and blank lines per file |
| `piton lsp` | Runs the language server over stdio |
| `piton reach [targets]` | Reports reachable and unreachable anchors, with depth and path |

## Layout

```
crates/piton-analyze   semantic analysis of prose against Piton structure
crates/piton-core      values, text, diagnostics, naming rules
crates/piton-syntax    lexer, parser, lossless CST, formatter
crates/piton-compile   modules, inheritance, evaluation, types, reachability
crates/piton-emit      JSON, YAML, and Markdown adapters
crates/piton-belay     the Belay framework and its three target adapters
crates/piton-lsp       the language server
crates/piton-cli       the `piton` binary
crates/xtask           repository automation, run as `cargo xtask <task>`
```

The parser produces a typed AST *and* a lossless rowan tree. The AST is what the
compiler lowers; the rowan tree is what the editor walks, and it reproduces every
byte of the source including comments and trivia. Expression interiors —
everything inside `{...}` — are parsed with chumsky, since that is the only
context-free corner of the grammar; the layout outside them is driven by lines
and indentation.

## Analysis

`piton analyze` reads what the prose says and combines it with what the
structure knows. Each sentence that makes a claim is reduced to a subject, a
property, a value, a polarity, and a modality; pairs of claims are then compared
nearest structural neighbour first, and three separate kinds of evidence —
semantic, structural, and contradiction — combine into a severity. An error
takes all three to be strong; a weak leg degrades the finding rather than
suppressing it.

Extraction is rule-based over a curated lexicon. There is no statistical
part-of-speech tagger and no dependency parser, so claims come only from copular
statements, and the lexicon's relations are the only ones the analysis acts on.
That is the point rather than a gap: a sentence it cannot read produces no
claim, and a value pair it knows nothing about produces no contradiction.
`The parser is fast` and `The parser is careful` are not a conflict, because
nothing establishes that they are.

Three rules keep it from inventing conflicts. Quoted text is being *mentioned*,
so a document that writes `we say "the button is blue"` has described a
statement rather than made one. Fenced blocks are data, not prose. And two
phrases only count as one subject when their modifiers are identical or one set
contains the other, so `the save button` and `the cancel button` stay separate.

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

**Any text without spaces is a property key.** `thisIsAKey`, `123`, `foo-bar`,
`false`, and `null` are all keys, exactly as the Dictionaries specification says.
A word that is a keyword elsewhere is still a key here: what separates
`anchor MyAnchor:` from `anchor: a description` is the colon, not the word. An
anchor body that contains something other than properties — a stray code fence,
say — gets a warning naming what will not be emitted.

**Only fully braced expressions evaluate.** `{a} && false` is the string
`true && false`, following the normative text — "an expression must be wrapped in
curly braces, otherwise it'll be interpreted as a string" — rather than the
worked example in `Anchors.md`, which shows it evaluating to `false`.

**The merge operator keeps the last occurrence.** Merging `[A, B, C]` into
`[A, B, C, D]` yields `[D, A, B, C]`, which is the result the specification
states.

**A line that is nothing but backslashes delimits a multi-line escape block.**
Its contents are literal and its delimiters are consumed, so a code example can
quote syntax the compiler would otherwise read. The closing run must be the same
length as the opening one, which lets a block quote another block's delimiters.
A backslash run *inside* a line keeps its inline meaning, so the two forms do not
collide — the same distinction Markdown draws between an inline code span and a
fenced block.

**Word splitting does not break up runs of capitals.** `whatIsAType` renders as
`What Is AType`, matching the reference output. Acronym-aware splitting would
produce a different document.

**Continuation lines are indented.** A multi-line value inside a list item or map
entry is indented so it stays within its item. The reference output leaves such
lines at column zero, which breaks the list it belongs to.

**Instructions outside `shapeRoot` attach to the code root**, with a warning,
since the specification lists their placement as undefined.

## The specification is the framework

`@piton/belay` is not written in Rust. The four constructs are the `.pi` files in
`spec/scope/belay/anchors`, embedded into the compiler at build time and served
from a virtual filesystem, so the specification's description of a skill *is* the
skill a project gets. Add a property to `Skill.pi` and every skill in every
project is required to define it, immediately. There is no second copy.

Those files also hold the prose that documents each construct, so the package
index re-exports the constructs by name rather than re-exporting the files
wholesale: `from @piton/belay import Skill` works, `import SkillBehavior` does
not.

Target configuration stays in the package index. The specification describes each
adapter as a prose contract — what the target supports, what must be diagnosed —
while the compiler needs machine-readable fields the contract does not carry, and
names them differently. Those are two artifacts about one subject rather than one
artifact written twice, so they are written separately.

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
