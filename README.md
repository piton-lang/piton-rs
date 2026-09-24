# Piton

A compiler, language server, and build tool for **Piton**, a declarative language
for writing agent guidance, and **Belay**, the framework that compiles that
guidance into the artifacts agentic coding tools read.

Piton has no runtime. It compiles to data — JSON, YAML, Markdown — through
renderers, and frameworks like Belay add adapters on top of them. The
specification in `spec/` is itself written in Piton, so the compiler's
acceptance suite is the language describing itself.

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

Requires Rust 1.75 or newer. The package commands (`tether`, `update`) also
need `git` on `PATH`; nothing else does.

## Installing

On macOS or Linux (or Git Bash on Windows):

```
curl -fsSL https://github.com/piton-lang/piton-rs/releases/latest/download/install.sh | sh
```

On Windows, in PowerShell:

```
irm https://github.com/piton-lang/piton-rs/releases/latest/download/install.ps1 | iex
```

Both install the latest build from `main`, check it against its checksum, and
put `piton` in `~/.local/bin` (`%LOCALAPPDATA%\piton\bin` on Windows). On a
Mac the script also clears the quarantine flag, since the builds aren't signed
yet. Set `PITON_VERSION=0.1.41` to install a particular build, or
`PITON_INSTALL_DIR` to put it somewhere else.

### From source

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
| `piton agent [claude]` | Launches the agent with a primer on the project and the fluency prompt; `--print-fluency` prints the prompt instead |
| `piton build [config]` | Builds the project configured by `piton.config.pi`; `--dry-run` reports without writing |
| `piton check [paths]` | Validates syntax, imports, references, types, inheritance, composition, exports, and circular dependencies; exits 1 on any error |
| `piton compile <path>` | Prints one file's output, or writes a glob's with `--write`; `--renderer json\|yaml\|markdown`, `--dependencies` |
| `piton format [path]` | Applies canonical formatting; `--check` reports without writing, and `piton format -` formats stdin to stdout |
| `piton loc [paths]` | Counts total, code, comment, and blank lines per file |
| `piton lsp` | Runs the language server over stdio |
| `piton reach [targets]` | Reports reachable anchors with depth and path, and what is unreachable; `--no-paths` and `--no-unreachable` trim the report |
| `piton remove <package>` | Deletes an installed package and drops it from `dependencies`, refusing if it was edited or anything still imports it |
| `piton tether [source]` | Installs every dependency, or adds `source` to `dependencies` and installs it; `--as` picks the name |
| `piton untether <package>` | Moves an installed package to `<root>/untethered/` and rewrites the imports that named it; `--as` renames it, `--no-rewrite` only moves it |
| `piton update [packages]` | Reinstalls the dependencies declared in `piton.config.pi` at their pinned versions |

`piton build` writes the renderer's output to the `output` directory in the
config (default `./dist`, as JSON unless `renderer` says otherwise): the entry
file and every file its exports reference, each at its place under `root`. So
`spec/components/Button.pi` becomes `dist/components/Button.json`. Frameworks
add their own output on top.

## Packages

Dependencies are vendored and managed. `piton tether <git-url>` clones a
repository, removes every trace of git from it, and copies the packages it
offers into `tethers/`, where it is committed with the project and read from disk like
any other source. Nothing is fetched at compile time.

A repository says what it offers with `piton-package` anchors from
`@piton/packaging`, listed under `packages` in its `piton.config.pi`. A project
never installs its own packages; `packages` is for other projects that tether
it.

```piton
use @piton/config
use @piton/packaging

export piton-config MyProject:
    root: ./spec

    packages:
        - {MyPackage}

    dependencies:
        - https://github.com/piton-lang/piton-rs
            tag: 1.0

piton-package MyPackage:
    name: my-package
    root: ./spec
```

Each package installs under `tethers/<name>`. Names are flat, with no `/`;
names like `@piton/belay` are kept for the packages bundled with the compiler.
Pins are strings, so `tag: 1.0` means the tag `1.0`, and a dependency gets one
pin at most. A repository that offers no packages is installed whole, under the
name its URL implies. An installed package is imported by name rather than by
path, and the rest of the path resolves as usual:

```piton
from my-package import MyAnchor
from my-package/nested/Thing import Other
use my-package
```

Dependencies are flat: one version of a repository is installed for the whole
project. When two packages want different versions, the newest commit wins and
a warning says so. `.piton/tether.lock` records the URL and commit each package
came from and a hash of its files, which is what lets `update` and `remove`
tell an untouched package from an edited one. An edited package is never
overwritten — `piton untether` is the way to keep the edits, and it moves
the package to `<root>/untethered/` and rewrites every import that named it.

## Layout

```
crates/piton-core      values, text, diagnostics, naming rules
crates/piton-syntax    lexer, parser, lossless CST, formatter
crates/piton-compile   modules, inheritance, evaluation, types, reachability
crates/piton-emit      JSON, YAML, and Markdown renderers
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

Some editors can't do everything natively. Helix has no way to express the
"dedent on a blank line" Enter rule. Zed, Kate and JetBrains get that rule from
the server's on-type formatting instead. JetBrains has no native highlighter
yet: it uses the VS Code TextMate bundle plus the LSP4IJ plugin.

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
workspace symbols, semantic highlighting, inlay hints, code actions, code
lenses, formatting (including on-type formatting for the Enter rules), folding,
selection ranges, and a compiled-output preview. There is no signature help.

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

The specification leaves some questions open. Each choice below is implemented
as described and is covered by a test.

**Only `@{...}` produces a reference document.** A `${...}` is a string, so it
does not create a file and nothing links to one.

**Braces that do not contain a valid expression are an error.** To write braces
in prose, escape them: `\ {...} \`.

**An anchor body that holds something other than properties** — a stray code
fence, say — gets a warning naming what will not be emitted.

**Word splitting does not break up runs of capitals.** `whatIsAType` renders as
`What Is AType`. Acronym splitting is an open decision in the specification.

**Continuation lines are indented.** A multi-line value inside a list item or map
entry is indented so it stays within its item.

**Instructions outside `shapeRoot` are an error.** The specification lists
their placement as undecided, so the build says so
(`instruction-placement-unspecified`) rather than guessing.

**A reference to a construct links to its own output.** `@{...}` on a skill,
command or agent links to its native file, like its `SKILL.md`, rather than to
a copy in the reference tree.

## The specification is the framework

`@piton/belay` is not written in Rust. The four constructs are the `.pi` files in
`spec/scope/belay/anchors`, and the three adapters are the files in
`spec/scope/belay/adapters`. They are embedded into the compiler at build time
and served from a virtual filesystem, so the specification's description of a
skill *is* the skill a project gets. Add a property to `Skill.pi` and every
skill in every project is required to define it, immediately. There is no
second copy.

Those files also hold the prose that documents each construct, so the package
index re-exports the constructs and adapters by name rather than re-exporting
the files wholesale: `from @piton/belay import Skill` works,
`import SkillBehavior` does not. The index adds what has no spec file of its
own: the `belay-config` anchor and the special imports. `ClaudeAdapter` is
still exported as a deprecated alias of `ClaudeCodeAdapter`.

A project picks targets by listing adapter anchors in its `belay-config`:

```piton
use @piton/config
use @piton/belay

from @piton/belay import ClaudeCodeAdapter, CodexAdapter

export piton-config MyProject:
    root: ./spec

    frameworks:
        - {Belay}

belay-config Belay:
    codeRoot: ./src
    shapeRoot: ./spec/shape

    adapters:
        - {ClaudeCodeAdapter}
        - {CodexAdapter}
```

`crossDiscovery` (`allow` or `separate`) and `instructionByteLimit` are the
other two fields.

### Location exports

`@piton/belay` also exports five paths a generated artifact may need to point
at:

| Export | Resolves to |
| --- | --- |
| `BELAY_AGENT_ROOT` | The directory of the target being compiled — `.claude`, `.codex`, `.opencode` |
| `BELAY_PROJECT_ROOT` | The project root |
| `BELAY_SHAPE_ROOT` | The configured `shapeRoot`, or the project root |
| `BELAY_CODE_ROOT` | The configured `codeRoot` |
| `BELAY_COMPILED_SHAPE` | The target's *compiled* shape directory, under its reference root |

None of them can be decided while evaluating: the agent root depends on which
adapter is being compiled, and all five are written relative to the file that
carries the use. So evaluation leaves a marker and Belay substitutes a path per
file, per target — which is why the same source produces `../..` in a skill and
something else in a reference document.

```piton
use @piton/belay

from @piton/belay import BELAY_CODE_ROOT

export skill Example:
    description: Explains where the code lives.
    useWhen: Explicitly invoked
    prompt: The implementation is under ${BELAY_CODE_ROOT}.
```

### Reference documents

Everything the build reaches that is not a construct goes into the target's
reference root, one Markdown file per source module, with each anchor as a
heading. `@{Button}` becomes a relative link with a fragment, like
`[Button](../../reference/components/Button.md#button)`, and
`@{Button.color}` links to the property's heading.

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
file Belay did not write is never deleted. The manifest's `targets` records the
date each target's platform documentation was last checked.

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
