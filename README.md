# Piton

Piton is a language for writing the instructions that agentic coding tools read.
You describe your system once — in prose where prose is right, in structure
where structure is right — and the compiler produces the agents, skills,
commands, and per-directory `AGENTS.md` files that Claude Code and its cousins
actually load.

```piton
use @piton/belay

from ./ButtonDesign import ButtonDesign

export skill BuildButton:
    description: Implements the button component
    useWhen: the user asks to build or change the button

    prompt:
        The button should be clickable and should have a hover state.

        For details about the design, read @{ButtonDesign}.
```

Piton has no runtime. It compiles to data — JSON, YAML — or, through a
framework, to documents. **Belay** is the framework bundled here, and it is what
turns the file above into `.claude/skills/build-button/SKILL.md`.

Why bother, instead of writing the Markdown by hand? Because Markdown has no way
to say "this is the same thing I described over there". Piton has inheritance,
imports, and references, so a design decision lives in one place and every
document that needs it points at it.

---

## Install

```sh
git clone git@github.com:piton-lang/piton-rs.git piton
cd piton
cargo xtask install
```

That builds the release binary, copies it into Cargo's binary directory
(`~/.cargo/bin` by default), and runs it to check that it works. You need a
recent stable Rust toolchain and nothing else.

```sh
piton --version
```

If that fails, the binary is not on your `PATH`. `cargo xtask install --dest DIR`
puts it somewhere else, and `cargo xtask uninstall` removes it.

## Your first project

Make a directory with somewhere to put your descriptions and somewhere for your
code to live:

```sh
mkdir -p my-project/spec my-project/src
cd my-project
```

**1. Tell Piton what this project is.** Every project has a `piton.config.pi`:

```piton
use @piton/config
use @piton/belay

from @piton/belay import ClaudeAdapter

export piton-config Config:
    // Where your Piton sources live. Absolute imports like `/shapes/Button`
    // resolve from here.
    root: ./spec

    frameworks:
        - {Belay}

// Belay needs to know where your code is, and where to write.
belay-config Belay:
    codeRoot: ./src
    shapeRoot: ./spec/shape

    adapters:
        - {ClaudeAdapter}
```

**2. Write something for the agent to read.** `spec/index.pi` is the entry
point — everything reachable from it gets compiled:

```piton
use @piton/belay

export skill ReviewChange:
    description: Reviews a change against the recorded design intent
    useWhen: the user asks for a review

    prompt:
        Read the diff. For anything you touch, read its shape document first.
```

**3. Build it.**

```sh
piton build
```

```
.claude/skills/review-change/SKILL.md
wrote 1 file
```

Open that file and you will find front matter and prose that Claude Code loads
as a skill. Change the Piton, run `piton build` again, and the Markdown follows.

`examples/agentic-text-editor` in this repository is a complete project of this
shape: the specification for a Rust and egui text editor, split into an `agent`
tree of agents, commands and skills; a `concept` tree that says how the editor
fits together; and a `shape` tree mirrored onto `src/`.

## Set up your editor

Every editor integration lives in `editors/` and is generated from the compiler
itself. They all start the same language server, `piton lsp`, which gives you
diagnostics, completion, hover, go to definition, find references, rename,
formatting, and inlay hints. The `piton` binary must be on your `PATH` first —
step 1 above did that.

Run these from the repository you cloned.

**VS Code** (also Cursor and Windsurf):

```sh
cd editors/vscode
npm install
npx vsce package
code --install-extension piton-0.1.0.vsix
```

**Zed** — Zed compiles the extension itself, so add the WebAssembly target
first, then install it from the command palette:

```sh
rustup target add wasm32-wasip1
```

Command palette → `zed: install dev extension` → choose `editors/zed`.

Highlighting comes from the Tree-sitter grammar, which Zed fetches over git
from the commit `editors/zed/extension.toml` pins, so there is nothing more to
do. Everything the language server provides works even without it.

**Neovim** — add `editors/vim` to your `runtimepath`, then:

```lua
require('piton').setup()
```

**Vim** — copy the syntax files in, or point a plugin manager at `editors/vim`:

```sh
cp -r editors/vim/{syntax,ftdetect,ftplugin} ~/.vim/
```

**Emacs**:

```elisp
(add-to-list 'load-path "/path/to/piton/editors/emacs")
(require 'piton-mode)
```

`piton-mode` registers itself with both `eglot` and `lsp-mode`.

**JetBrains IDEs** — there are two ways in, because the LSP API only exists in
the paid IDEs.

For highlighting in *any* of them, free or paid, with no build:
Settings → Editor → TextMate Bundles → `+` → select
`editors/jetbrains/bundles/piton` → Apply.

For the language server as well, in IntelliJ IDEA Ultimate, WebStorm, PyCharm
Professional and friends, build the plugin. You need a JDK 17 or newer and
Gradle; the wrapper script is not checked in because it ships as a binary:

```sh
cd editors/jetbrains
gradle wrapper        # once — or use `gradle` in place of `./gradlew` below
./gradlew buildPlugin
```

Then Settings → Plugins → the gear icon → **Install Plugin from Disk…** and
choose `build/distributions/piton-0.1.0.zip`.

**Sublime Text**:

```sh
cp -r editors/sublime "$HOME/.config/sublime-text/Packages/Piton"
```

Install the `LSP` package and the bundled settings start the server.

**Helix** — merge `editors/helix/languages.toml` into
`~/.config/helix/languages.toml`, then `hx --grammar fetch && hx --grammar build`.

**Kate**:

```sh
mkdir -p ~/.local/share/org.kde.syntax-highlighting/syntax
cp editors/kate/piton.xml ~/.local/share/org.kde.syntax-highlighting/syntax/
```

Enable Kate's LSP Client plugin and point it at `piton lsp`.

More detail, and what to do when the binary is not on your `PATH`, is in
[editor setup](docs/editors.md).

## What to reach for next

- **Give the agent guidance per directory.** An `instruction` compiles into an
  `AGENTS.md` beside the code it applies to, and a `self-instruction` into one
  beside the specification itself, each with a `CLAUDE.md` that imports it for
  Claude Code. See [the Belay guide](docs/belay.md).
- **Stop repeating yourself.** An `anchor` is a named, inheritable block; `@{}`
  points one document at another instead of copying it.
- **Read the language reference**, which is generated from the compiler, so it
  cannot drift: [`docs/language.md`](docs/language.md).

## Commands

| Command | What it does |
| --- | --- |
| `piton build` | Build the project described by `piton.config.pi` |
| `piton build check` | Build and report problems without writing |
| `piton check <path>` | Report problems in specific files |
| `piton reach [file]` | Show what the entry point reaches, or how one file got reached |
| `piton loc [path]` | Count lines, separating prose from structure |
| `piton compile <path>` | Compile to JSON, or YAML with `--format yaml` |
| `piton format <path>` | Apply the canonical style; `--check` to only report |
| `piton lsp` | Run the language server |
| `piton docs` | Print the language reference |
| `piton claude` | Launch Claude Code already fluent in Piton |
| `piton grammar [dir]` | Regenerate the editor integrations in `editors/` |
| `piton ast <path>` | Print a file's concrete syntax tree |

Full detail in [the CLI reference](docs/cli.md).

## Documentation

| | |
| --- | --- |
| [Language reference](docs/language.md) | Every rule, generated from the compiler itself |
| [The Belay framework](docs/belay.md) | Agents, skills, commands, instructions, and what they compile to |
| [CLI reference](docs/cli.md) | Every command and flag |
| [Editor setup](docs/editors.md) | VS Code, Zed, JetBrains, Vim, Emacs, Sublime, Helix, Kate |
| [Architecture](docs/architecture.md) | How the compiler is put together, and why |
| [Development](docs/development.md) | Building, testing, and publishing the grammar |

[`REFERENCE.md`](REFERENCE.md) is the human-authored language specification —
the essay the implementation is written against, and the place to look for why
Piton is shaped the way it is rather than for how to use it.

Two further documents live at the repository root because they are about this
implementation rather than about using it: [`.spec.md`](.spec.md) restates
`REFERENCE.md` as one checkable claim per line, so the two can be diffed, and
[`.decisions.md`](.decisions.md) records every choice made where the
specification left room.

## Licence

MIT — see [`LICENSE.md`](LICENSE.md).
