# Editor support

Every editor the specification names, and what each one needs.

| Editor | Highlighting | Language server | Directory |
| --- | --- | --- | --- |
| VS Code | TextMate | bundled client | [`vscode`](vscode) |
| Zed | tree-sitter | WebAssembly extension | [`zed`](zed) |
| Helix | tree-sitter | built in | [`helix`](helix) |
| Emacs | tree-sitter (the grammar's own `highlights.scm`) | eglot | [`emacs`](emacs) |
| Neovim | Vim syntax (tree-sitter optional) | built-in `vim.lsp` or lspconfig | [`neovim`](neovim) |
| Vim | Vim syntax | vim-lsp / coc / ALE | [`vim`](vim) |
| Sublime Text | sublime-syntax | LSP package | [`sublime`](sublime) |
| Kate | KSyntaxHighlighting | LSP Client | [`kate`](kate) |
| JetBrains | TextMate bundle (the spec asks for a native one; see its README) | LSP4IJ | [`jetbrains`](jetbrains) |

The grammar shared by Zed, Helix, Emacs and Neovim lives in
[`tree-sitter-piton`](tree-sitter-piton). Zed keeps its own copy of the queries,
because a Zed extension reads them from its own `languages/piton/` directory.
Some of them are the same query — `highlights.scm` and `injections.scm`, which a
test holds byte for byte identical — and the rest are not, because Zed's capture
vocabulary is its own, and it has queries for brackets, the outline and vim's
text objects that the others have no equivalent of. Emacs ships a third copy of
`highlights.scm` in [`emacs/queries`](emacs/queries), also held identical, and
maps each capture to a face rather than keeping rules of its own.

Indentation is the query that differs most. The grammar's `indents.scm` is
written for nvim-treesitter (`@indent.begin`); Helix reads `@indent` and
`@extend`, so [`helix/queries/indents.scm`](helix/queries/indents.scm) is its
own copy; and Zed indents from a line pattern in its `config.toml`, because the
line-oriented grammar has no node spanning a block for its `@indent` to measure.

## Editor behaviour

The specification's rules for every editor (`Editor` in
`spec/scope/tooling/editors/index.pi`):

- **Enter on a colon line** — a declaration, or a dictionary or anchor property
  with nothing after its colon — puts the new line one level in. A list item
  is never a key, even with a colon at the end (`- Settings:` is just a
  string), so a list line or a merge line (`+ ...`, `++ ...`) never opens a
  block; nor does a comment.
- **Enter on a blank line** inside a dictionary or anchor puts the new line one
  level back.
- **Autoformat on save** is an option, off by default. The formatter adds the
  space after `//` and touches nothing else that is commented.

Where each editor gets them:

| Editor | Enter on a colon line | Enter on a blank line | Format on save |
| --- | --- | --- | --- |
| VS Code | `onEnterRules` | `onEnterRules` | `piton.formatOnSave` |
| Zed | `increase_indent_pattern` | server on-type formatting | `format_on_save` |
| Helix | `helix/queries/indents.scm` | not possible (see its README) | `auto-format` |
| Emacs | `piton--indent-line` | `piton--indent-line` | `piton-format-on-save` |
| Neovim | `indent/piton.vim` (+ server on 0.12+) | `indent/piton.vim` | `format_on_save` |
| Vim | `indent/piton.vim` | `indent/piton.vim` | `g:piton_format_on_save` |
| Sublime Text | `Piton.tmPreferences` | key binding + macro | LSP `lsp_format_on_save` |
| Kate | server on-type formatting | server on-type formatting | LSP Client "Format on save" |
| JetBrains | server on-type formatting (LSP4IJ) | server on-type formatting (LSP4IJ) | Actions on Save → Reformat code |

Zed is also the one editor here that needs code: `extension.toml` can declare
that Piton has a language server but has nowhere to name the program, so
[`zed/src/piton.rs`](zed/src/piton.rs) finds `piton` and hands Zed the command.

## Install the binary first

Everything here assumes `piton` is on `PATH`:

```
cargo xtask install
```

The language server is the same binary: `piton lsp` speaks LSP over stdio.

## What belongs to which half

Highlighting is the part that can be done from the text alone, so each editor
gets a syntax definition in its own format. Everything else needs the resolved
program — what a name refers to, which base supplied a value, what a project
reaches — so it comes from the language server: diagnostics, completion, hover,
go-to-definition, find-references, rename, document and workspace symbols,
semantic highlighting, inlay hints, code actions, code lenses, formatting,
folding and selection ranges.

That split is what the specification asks for. Zed, Helix and Emacs are listed
as `["tree-sitter", "lsp"]` and VS Code as `["textmate", "lsp"]`: a syntax
definition for tokens, a server for meaning.

## Keeping the definitions honest

Six syntax definitions describing one language is six chances to disagree with
it, so none of them is taken on trust. The parser decides what every byte of a
file is, and the definitions are tested against that.

- **The classes.** [`fixtures/scopes.toml`](fixtures/scopes.toml) lists the
  highlighting classes — comment, keyword, declared type, type reference,
  variable, key, module path, built-in type, number, constant, string,
  `this`/`self`/`super`, operator, list marker, sigil, escape, verbatim — and
  the one name each format uses for each: a TextMate and Sublime scope, a
  tree-sitter capture, a Kate item, a Vim group, an Emacs face. Anything else is
  plain text, and that includes prose: it is shown in the editor's plain text
  style everywhere, so the structure around it stands out. A code fence is
  text too; only an escape block directly inside a fence that names a language
  may be highlighted as that language.
- **The answer.** `piton_syntax::highlight::classify` reads the class of every
  token off the parser's syntax tree, and writes
  [`fixtures/*.expected`](fixtures) for the fixtures beside them: valid Piton
  covering declarations, imports, properties, literal values and their prose
  look-alikes, prose pitfalls, expressions, escapes and fences, composition and
  comments. `PITON_BLESS=1 cargo test -p piton-syntax --test highlight_corpus`
  rewrites them, and the review is the diff.
- **The comparison.** `crates/piton-syntax/tests/highlight_corpus.rs` runs each
  definition over the fixtures with the tool that runs it — the tree-sitter CLI
  on the grammar and `highlights.scm`; VS Code's own `vscode-textmate` engine
  (`vscode/scripts/tokenize.mjs`, after `npm ci` in `vscode/`); syntect for
  the Sublime syntax; `vim`; `ksyntaxhighlighter6` — and compares the class of
  every non-blank byte with the parser's. A runner whose tool is not installed
  says so and passes, unless `PITON_REQUIRE_EDITOR_TOOLS=1`.
- **The exceptions.** Where a format cannot, or by design does not, do what the
  parser does, [`fixtures/gaps.toml`](fixtures/gaps.toml) names the line and
  says why. An entry whose line starts matching fails the test, so the list
  only holds deviations that are real.

`crates/piton-syntax/tests/editors.rs` adds the cheap static checks: the keyword
lists in `crates/piton-syntax/src/language.rs`, that the copies of the queries
are identical, that the highlights query has no predicates (so every editor
reads it the same way), and that every name in `scopes.toml` is used by its
format. `crates/piton-lsp/tests/editors.rs` checks that VS Code maps each
semantic token type to the very scope the grammar uses for the same class, so
the colour only changes when the server's resolution adds something.

## On the tree-sitter grammar

It is a line-oriented grammar with no external scanner: it recognises lines and
the tokens in them, and leaves indentation alone. Structure comes from the
language server, which has the resolved program. Capitalised and lowercase
names are different tokens, so the highlights query needs no predicates and
captures no node twice.

That is a deliberate trade. An indentation-sensitive grammar needs a C scanner
tracking INDENT and DEDENT, and it would duplicate — imperfectly — what the
compiler already does. Since every editor using this grammar also runs the
server, the grammar only has to be right about tokens.

`src/parser.c` is generated, not committed. Run `tree-sitter generate` in
`tree-sitter-piton` to build it. Zed fetches the grammar from the published
`tree-sitter-piton` repository at the commit in `zed/extension.toml`, so a
grammar change reaches Zed after `cargo xtask publish-grammar`.
