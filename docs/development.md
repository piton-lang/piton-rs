# Development

```sh
cargo test                    # the whole suite
cargo clippy --all-targets
cargo xtask build             # build the CLI, print where it landed
cargo xtask install           # build it and put it on your PATH
cargo xtask grammar           # regenerate editors/, including the parser
cargo xtask grammar-test      # the Tree-sitter corpus and highlight assertions
cargo xtask zed               # check the Zed extension compiles to WebAssembly
```

`cargo xtask` is an alias defined in `.cargo/config.toml`.

## Tests

The suite is organised by what each part is being held to.

| Suite | Correctness means |
| --- | --- |
| `piton-syntax/tests/syntax.rs` | Parsing is lossless, total, and never panics; the tree has the shape the compiler reads |
| `piton-core/tests/language.rs` | The Piton specification, one test per rule |
| `piton-core/tests/modules.rs` | Imports, exports, `use`, and path resolution against real files |
| `piton-core/tests/formatting.rs` | Formatting preserves what a file compiles to |
| `piton-fmt` | The canonical style, and idempotence |
| `piton-belay/tests/conformance.rs` | The rules Claude Code enforces on the files it loads |
| `piton-belay/tests/project.rs` | The output layout the Belay specification describes |
| `piton-grammar/tests/highlighting.rs` | TextMate patterns compiled and executed against real lines |
| `piton-lsp/src/tests.rs` | Every advertised capability, including what completion must *not* offer |
| `editors/tree-sitter-piton/test` | `tree-sitter test`: a parse corpus and highlight assertions |

Two habits are worth keeping. Assert what is *not* offered as well as what is —
the completion regressions that matter are noise, and noise is invisible to a
test that only checks for presence. And when a new test passes first try, break
the thing it covers and watch it fail before trusting it.

## Regenerating the editor integrations

Everything in `editors/` is generated. Edit `crates/piton-grammar`, never the
output.

```sh
cargo xtask grammar        # regenerate, and refresh src/parser.c
cargo xtask grammar-test   # then check it still parses and highlights
```

The Tree-sitter parser (`src/parser.c` and its headers) is committed on purpose:
Zed and Helix compile it, they do not run the Tree-sitter CLI.

## Publishing the Tree-sitter grammar

Zed, Helix, and nvim-treesitter fetch grammars over git rather than from a
directory, so `editors/tree-sitter-piton` is published as its own repository — a
`git subtree` of this one. This repository stays the source of truth.

Tell git where it lives, once per clone. No URL is stored in the source:

```sh
git remote add grammar <url>
```

Then:

```sh
cargo xtask publish-grammar               # push, and pin the editors to it
cargo xtask publish-grammar --dry-run     # show what would be pushed
cargo xtask publish-grammar --remote URL  # a one-off, somewhere else
```

That regenerates `editors/`, refreshes the parser, commits the grammar prefix,
runs `git subtree push`, asks the remote what commit it now has, and writes that
commit into the Zed, Helix, and Neovim configurations so they fetch exactly what
was pushed. `--no-commit` makes it refuse rather than commit on your behalf.

## Project documents

[`.spec.md`](../.spec.md) restates the language specification as one checkable
claim per line, so it can be diffed against the human-authored spec.
[`.decisions.md`](../.decisions.md) records every choice made where that
specification left room, and why.
