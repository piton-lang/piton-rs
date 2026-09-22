# tree-sitter-piton

A [tree-sitter](https://tree-sitter.github.io) grammar for
[Piton](https://github.com/piton-lang/piton).

```bash
npm install tree-sitter-piton
```

## Queries

`queries/` holds highlights, injections, locals, folds and indents. Editors
that read tree-sitter queries — Helix, Neovim, Zed — can point at them
directly.

Injections matter more here than in most languages: a Piton value can hold a
fenced code block in another language, and the injection query is what makes
that block highlight as what it is.

## Generating the parser

The repository holds the grammar, not the parser it produces. Generate it with
the tree-sitter CLI:

```bash
npm install
npx tree-sitter generate
npx tree-sitter test
```

## This repository is generated

The grammar is developed in the [Piton repository](https://github.com/piton-lang/piton),
under `editors/tree-sitter-piton`, where it is checked against the compiler's
own keyword list so the two cannot drift. It is copied here by
`cargo xtask publish-grammar`.

Send changes there rather than here; a pull request against this repository
would be overwritten by the next publish.
