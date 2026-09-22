# Piton for Zed

Highlighting from the tree-sitter grammar, everything structural from
`piton lsp`.

## Install

Install `piton` first, so the language server exists:

```
cargo xtask install
```

Then add this directory as a dev extension: run **zed: install dev extension**
from the command palette and choose `editors/zed`. Zed builds the extension for
`wasm32-wasip2`, so the target has to be installed:

```
rustup target add wasm32-wasip2
```

Zed also fetches the tree-sitter grammar at the revision `extension.toml` pins
and builds it into `grammars/`. Neither that directory nor `target/` belongs in
the repository, and `.gitignore` keeps them out.

## Finding the server

The extension looks for `piton` in this order:

1. `lsp.piton.binary.path` from your Zed settings, used exactly as written.
2. `piton` on the `$PATH` Zed sees for the project.
3. `$CARGO_HOME/bin/piton`, then `~/.cargo/bin/piton`.

The third step exists because `cargo xtask install` installs into the cargo bin
directory, which is on the `$PATH` of a shell but not always on the `$PATH` of
an editor started from a desktop launcher. A candidate there is run once, with
`--version`, before it is used: an extension cannot look at the filesystem
outside its own directory, so running it is the only way to tell a real path
from a guess.

If none of them finds anything, the extension says so and repeats the settings
that would fix it:

```json
{
  "lsp": {
    "piton": {
      "binary": {
        "path": "/path/to/piton",
        "arguments": ["lsp"]
      },
      "initialization_options": {},
      "settings": {}
    }
  }
}
```

`initialization_options` and `settings` are passed through to the server
untouched, as `initialize` options and as the answer to
`workspace/configuration`.

## What comes from where

| | |
| --- | --- |
| `languages/piton/highlights.scm` | tokens: keywords, keys, sigils, prose, fences |
| `languages/piton/injections.scm` | a fenced block is highlighted as its own language |
| `languages/piton/brackets.scm` | matching `[ ]`, an interpolation and its brace, a fence and its close |
| `languages/piton/outline.scm` | the outline panel and the breadcrumbs |
| `languages/piton/overrides.scm` | scopes where auto-closing is turned off |
| `languages/piton/textobjects.scm` | `gc` in vim mode |
| `languages/piton/semantic_token_rules.json` | how the server's semantic tokens are styled |
| `languages/piton/config.toml` | comments, indentation, brackets, word characters |
| `piton lsp` | everything that needs the resolved program |

That last row is the long one: diagnostics, completion, hover, go-to-definition,
find-references, rename, document and workspace symbols, semantic highlighting,
inlay hints, signature help, code actions, formatting, folding and selection
ranges.

`src/piton.rs` adds two things on top of the protocol. It labels completions and
symbols so the name is highlighted for what it is — an anchor as a type, a key
as a property, a built-in type as a type — with the server's detail beside it in
the colour of a comment, and it keeps the detail out of the range Zed filters
on, so searching for `agent` does not match every property that says `inherited
from Agent`.

Completion itself is contextual, and the server decides what that means: module
paths after `use`, exported names after `import`, bases after `extends`, types
inside a `::` constraint, an anchor's members after a `.`, and for a value, only
what its constraint accepts. A name from a file this one does not import comes
with the `from … import …` line attached. The description of an item arrives
through `completionItem/resolve` when the cursor settles on it, because building
one renders the anchor's whole compiled value.

## Indentation

There is no `indents.scm`. Zed's `@indent` measures a node that spans the lines
it indents, and the grammar has no such node: indentation is deliberately left
out of it, so an anchor's header is a line and its body is not part of it.

The rule is therefore a line pattern in `config.toml`:

```toml
increase_indent_pattern = ":\\s*$"
```

A declaration or a key that ends in a colon opens a block. A key with a value on
the same line does not.

## Checking it

```
cargo test -p piton-syntax --test editors     # queries, config, the server command
cargo build --release --target wasm32-wasip2  # from this directory
```

The first of those reads the files in this directory and checks them against the
language: that every query names a node and a token the grammar actually writes,
that the queries shared with the grammar have not drifted from it, and that this
extension starts the server the same way every other editor does.
