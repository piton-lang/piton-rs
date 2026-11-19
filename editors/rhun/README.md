# Piton for rhun

[rhun](https://github.com/vshvedov/rhun/) support: syntax highlighting from a
rhun grammar, and the language server through a rhun extension.

| File | What it is |
| --- | --- |
| `syntax/piton.syn` | The grammar: highlighting, `//` for Toggle Comment, indent after a colon. |
| `extension.ini` | The extension manifest. It brings the grammar and starts `piton lsp` for `.pi` files. |
| `lsp.patch` | A patch to [rhun-editor/lsp](https://github.com/rhun-editor/lsp), the first-party bridge from rhun's extension protocol to LSP. |
| `build.sh` | Builds the patched bridge into `bin/`. |

## Highlighting only

Copy the grammar into rhun's grammar folder:

```
mkdir -p ~/.config/rhun/syntax
cp syntax/piton.syn ~/.config/rhun/syntax/
```

## Highlighting and the language server

rhun extensions speak rhun's own protocol, not LSP. The first-party `lsp`
extension is the bridge, but its sandbox starts only the servers that its
manifest names, and `piton` is not one of them. Thus this directory is an
extension of its own: the same bridge, at a pinned commit, with `lsp.patch`. The
patch makes it serve Piton only and start `piton lsp`, and gives its commands
the `piton.` prefix.

1. Put `piton` on `PATH` (`cargo xtask install`). The server is `piton lsp` on
   stdio.
2. Run `./build.sh`. It needs git and Go, and writes
   `bin/piton-lsp-<target>`.
3. In rhun, install this folder as an extension: **Install Extension from
   Folder** in the command palette, or `ext-install-folder PATH link` in a rhun
   script.

The extension starts when a Piton file opens. It gives what the server gives:
problems, completion, hover, go to definition, references, symbols, code
actions, rename, formatting, folding, semantic colors and inlay hints. Open
the folder that holds `piton.config.pi`, so the server finds the project.

## Editor behaviour

| Rule | How |
| --- | --- |
| Enter on a colon line | `colon_indent = yes` in the grammar |
| Enter on a blank line | not possible: a rhun grammar has no rule for it |
| Format on save | Format Document (shift+alt+f) through the server; rhun's save actions are an option |

## What the grammar cannot do

A rhun grammar has no context. It colours a word by its spelling, a number
where it starts, and a region between two tokens. Only three rules can see
where they are: a line comment at the first token of a line, a key
(`labels = yes`, a word at the first token of a line before a `:`), and a
verbatim block between two `\\\` lines. Thus:

- A declared name, a type name, a module path and the names in an expression
  are plain. The semantic colors of the server paint them.
- `operators = no`, because rhun would colour the colon of each key. Thus the
  sigils and operators of an expression and the `-` of a list item are plain.
- The grammar lists no keywords, because a word list would also colour
  `from` in a string body. The semantic colors of the server paint each
  keyword where the parser reads one. Without the server, keywords are plain.
- `true`, `false`, `null` or a number in prose gets its colour.

`editors/fixtures/gaps.toml` lists each fixture line where rhun differs from
the parser, and says why. `crates/piton-syntax/tests/highlight_corpus.rs` runs
the grammar in a headless rhun (`print-syntax`) and holds it to that list.
