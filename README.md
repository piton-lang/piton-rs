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
piton tether URL # vendor a dependency into tethers/
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
| `piton build [config]` | Compiles the project configured by `piton.config.pi`; `--dry-run` reports without writing |
| `piton check [paths]` | Validates syntax, imports, references, types, inheritance, composition, exports, and circular dependencies; exits 1 on any error |
| `piton compile <path>` | Renders one file, or a `--write` glob, through `--adapter json\|yaml\|markdown` |
| `piton format [path]` | Applies canonical formatting; `--check` reports without writing |
| `piton loc [paths]` | Counts total, code, comment, and blank lines per file |
| `piton lsp` | Runs the language server over stdio |
| `piton reach [targets]` | Reports reachable and unreachable anchors, with depth and path |
| `piton remove <package>` | Deletes an installed package, refusing while anything still imports it |
| `piton tether <source>` | Clones a git repository, strips it of git, and installs what it publishes under `tethers/` |
| `piton untether <package>` | Moves an installed package into the source root and rewrites the imports that named it |
| `piton update [packages]` | Reinstalls the dependencies declared in `piton.config.pi` at their pinned versions |

## Packages

Dependencies are vendored and managed. `piton tether <git-url>` clones a
repository, removes every trace of git from it, and copies what it publishes
into `tethers/`, where it is committed with the project and read from disk like
any other source. Nothing is fetched at compile time.

A repository says what it publishes with `package` anchors from
`@piton/packaging`, listed in its `piton.config.pi`:

```piton
use @piton/config
use @piton/packaging

export piton-config MyProject:
    root: ./spec

    packages:
        - {MyPackage}

    dependencies:
        - https://github.com/piton-lang/piton-rs
            tag: "1.0"

package MyPackage:
    name: my-package
    root: ./spec
```

A repository with no configuration is installed whole, under the name its URL
implies. An installed package is imported by name rather than by path, and the
rest of the path resolves as usual:

```piton
from my-package import MyAnchor
from my-package/nested/Thing import Other
use my-package
```

Dependencies are flat: one version of a repository is installed for the whole
project, and a disagreement about which version is reported rather than
resolved quietly. `.piton/packages.lock.json` records the commit each package
came from and a digest of every file it installed, which is what lets `update`
and `remove` tell an untouched package from an edited one. An edited package is
never overwritten — `piton untether` is the way to keep the edits, and it moves
the package to `<root>/untethered/` and rewrites every import that named it.

## Layout

```
crates/piton-core      values, text, diagnostics, naming rules
crates/piton-syntax    lexer, parser, lossless CST, formatter
crates/piton-compile   modules, inheritance, evaluation, types, reachability
crates/piton-emit      JSON, YAML, and Markdown adapters
crates/piton-belay     the Belay framework and its three target adapters
crates/piton-lsp       the language server
crates/piton-cli       the `piton` binary
crates/xtask           repository automation, run as `cargo xtask <task>`
editors/               syntax definitions and language-server wiring
packages/              the Vite and Astro plugins, which have to be JavaScript
```

The parser produces a typed AST *and* a lossless rowan tree. The AST is what the
compiler lowers; the rowan tree is what the editor walks, and it reproduces every
byte of the source including comments and trivia. Expression interiors —
everything inside `{...}` — are parsed with chumsky, since that is the only
context-free corner of the grammar; the layout outside them is driven by lines
and indentation.

## Publishing the grammar

The tree-sitter grammar is developed here, in `editors/tree-sitter-piton`,
because this is the only place it can be checked against the language: the
compiler's keyword list is the source of truth, and a test reads the grammar
back to make sure the two agree. Editors that consume tree-sitter grammars
expect one repository per grammar, so it is published as a copy:

```
cargo xtask publish-grammar             # to piton-lang/tree-sitter-piton
cargo xtask publish-grammar --dry-run   # prepare the commit, push nothing
```

The published history is kept rather than restarted. The remote is cloned, its
contents are replaced with the grammar directory, and the result is one commit
naming the revision it came from — so a reader of either repository can line
them up. Replacing rather than adding means a query deleted here is deleted
there.

It refuses to publish when the grammar directory has uncommitted changes, since
the commit it writes would name a revision that does not contain them;
`--allow-dirty` overrides that. `--tag` also pushes a tag, and `--remote` aims
somewhere else, which is how the task is tested against a local bare repository
instead of the real one.

If the tree-sitter CLI is installed the parser is generated into the publish,
so consumers need no toolchain. If it is not, the task says so and publishes
the grammar alone.

## Editor support

`editors/` covers every editor the specification names — VS Code, Zed, Helix,
Emacs, Neovim, Vim, Sublime Text, Kate and JetBrains — each with a syntax
definition in its own format and the wiring to run `piton lsp`.

Highlighting is the part that can be done from the text alone. Everything else
needs the resolved program, so it comes from the server. That split is what the
specification asks for: Zed, Helix and Emacs are listed as
`["tree-sitter", "lsp"]`, VS Code as `["textmate", "lsp"]`.

Nine syntax definitions describing one language is nine chances to disagree with
it, so `crates/piton-syntax/src/language.rs` holds the keyword lists and
`crates/piton-syntax/tests/editors.rs` checks every definition against them.
That test is not decoration: it caught `pass` missing from five of the nine.

## Consuming a specification from a build

`packages/vite-plugin-piton` makes `import spec from './app.pi'` work in Vite,
and `packages/astro-piton` adds the same to Astro. They are the only parts of
this repository that are not Rust, because a Vite plugin has to be JavaScript
to be loaded by Vite.

Neither reimplements the language. Both spawn the `piton` binary, so there is
one compiler and one set of diagnostics, and a build that succeeds is one
`piton check` agrees with. A second implementation in TypeScript could disagree
with the first, and the disagreement would show up as a build that passes while
the checker fails.

Hot reload needs to know more than Vite can see. Vite tracks the imports it
finds in JavaScript; it cannot see one `.pi` file importing another, and a
package can keep a file somewhere the importing text never names. So
`piton compile --dependencies` wraps a result with every source file the
compilation read, and the plugin watches all of them. That flag exists for
these packages and is the whole of what they need from the compiler beyond
rendering.

Those packages cannot be exercised from a Rust test, so what is tested here is
the seam: `crates/piton-cli/tests/packages.rs` reads the command line the
plugin writes and checks the compiler still accepts it, and checks that the
adapters named in the plugin are the adapters the compiler has. A renamed flag
fails in this suite rather than in someone else's project.

## The language server

`piton lsp` implements diagnostics, completion, hover, go-to-definition,
go-to-implementation, type hierarchy, find-references, rename, document and
workspace symbols, semantic highlighting, inlay hints, signature help, code
actions, formatting, folding, and selection ranges.

Every one of them answers from the *resolved* program rather than from raw
syntax. Hovering a user keyword finds the anchor it aliases and says so.
Go-to-definition on an inherited property lands on the base that actually
supplies the value. Go-to-implementation runs the other way, from an abstract
anchor to everything that extends it, and the type hierarchy keeps going in both
directions. Find-references on an anchor includes the places that reach it
through keyword sugar. Inlay hints show the inferred type and name the base a
value was inherited from or the declaration it overrides. Completing a module
path offers installed packages by name, not by the relative path to
`tethers/`.

Type hierarchy is registered dynamically rather than declared in the initialize
response, because the `lsp-types` version in use has no field for it. A client
that does not support dynamic registration simply never asks.

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

### Location exports

`@piton/belay` also exports five paths a generated artifact may need to point
at:

| Export | Resolves to |
| --- | --- |
| `BELAY_AGENT_ROOT` | The directory of the target being compiled — `.claude`, `.codex`, `.opencode` |
| `BELAY_PROJECT_ROOT` | The project root |
| `BELAY_SHAPE_ROOT` | The configured `shapeRoot`, or the project root |
| `BELAY_CODE_ROOT` | The configured `codeRoot`, or the project root |
| `__BELAY_SHAPE__` | The target's *compiled* shape directory, under its reference root |

None of them can be decided while evaluating: the agent root depends on which
adapter is being compiled, and all five are written relative to the file that
carries the use. So evaluation leaves a marker and Belay substitutes a path per
file, per target — which is why the same source produces `../..` in a skill and
something else in a reference document.

```piton
from @piton/belay import BELAY_CODE_ROOT

export skill Example:
    description: Explains where the code lives.
    useWhen: Explicitly invoked
    prompt: The implementation is under ${BELAY_CODE_ROOT}.
```

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

The package tests build real git repositories in a temporary directory and
tether them into real projects, because cloning, un-gitting, the lock file and
the refusal to overwrite an edited package are exactly the parts that cannot be
faked. They are skipped when git is not installed.
