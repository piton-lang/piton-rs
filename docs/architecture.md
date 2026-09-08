# Architecture

```
xtask/            development tasks, run as `cargo xtask <command>`
crates/
  piton-syntax    lexer, chumsky grammar, rowan CST, typed AST
  piton-core      resolution, evaluation, diagnostics, framework interface
  piton-fmt       the canonical formatter, over the same CST
  piton-belay     the Belay framework — a plugin, not part of the compiler
  piton-grammar   editor grammars, generated from the compiler's kind tables
  piton-docs      language reference, generated from the same tables
  piton-lsp       the language server, over the same compilation
  piton-cli       the `piton` binary; the only crate that names a framework
editors/          generated editor integrations, excluded from the workspace
examples/         a complete Belay project
```

Two properties are worth stating because the crate graph enforces them.

**Frameworks are plugins.** `piton-core` defines a `Framework` trait and never
names a concrete framework. `piton-cli` is the only crate that links
`piton-belay`, and it registers it in one function. Nothing about Belay's output
format is known to the compiler.

**The grammars, the docs, and the language server are the compiler.** Editor
word lists come from `piton_syntax::kind`. Framework keywords and sigils are
read out of each framework's own module source, by compiling it. The language
server answers from the same `Compilation` that `piton build` produces, so the
editor and the compiler cannot disagree.

## Parsing

Parsing works in two stages.

A hand-written, line-oriented lexer turns indentation into zero-width
`INDENT`/`DEDENT` markers. That is what makes the grammar context free: a
whitespace-sensitive language is only tractable for a parser combinator library
once indentation has become tokens.

A `chumsky` parser over that token stream produces a lightweight tree of node
kinds and token indices, which is then replayed over the *full* token stream to
build a lossless `rowan` green node. Trivia handling lives in one place, and
`parse(src).syntax().text() == src` holds for every input, valid or not.

Parse errors are recovered by an explicit "error line" alternative rather than
by chumsky's recovery combinators, so parsing always yields a complete tree and
diagnostics can be phrased with more context during validation.

## Evaluation

Anchors are compiled property by property, lazily. `self.name` inside an anchor
looks up a single property rather than compiling the whole anchor, which is what
stops a self-reference from being an inheritance cycle.

Two contexts travel with every expression: `self`, the most-derived anchor being
compiled, and `this`, the anchor the expression was written in. That distinction
is the reason a base can say "my own name" and a child can say "the name of
whoever ends up being compiled".

## The framework interface

A framework contributes:

- **modules** — Piton source resolvable as `@name`, which is where its anchors
  and keywords are declared;
- **sigils** — `@{}` and friends, and how to render them;
- **builtin values** — such as Belay's `__BELAY_SHAPE__`;
- **configuration** — it is asked whether a given config anchor is its own;
- **output** — given a `Compilation`, it returns files to write.

A framework describing itself in Piton is what lets `piton docs` and
`piton grammar` learn about it without either of them knowing it exists.
