# Editor support

Every editor the specification names, and what each one needs.

| Editor | Highlighting | Language server | Directory |
| --- | --- | --- | --- |
| VS Code | TextMate | bundled client | [`vscode`](vscode) |
| Zed | tree-sitter | WebAssembly extension | [`zed`](zed) |
| Helix | tree-sitter | built in | [`helix`](helix) |
| Emacs | tree-sitter or regex | eglot | [`emacs`](emacs) |
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
text objects that the others have no equivalent of.

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

Nine syntax definitions describing one language is nine chances to disagree with
it. `crates/piton-syntax/src/language.rs` holds the keyword lists, and
`crates/piton-syntax/tests/editors.rs` checks every definition in this directory
against them, so a keyword added to the compiler cannot be quietly missing from
an editor.

## On the tree-sitter grammar

It is a line-oriented grammar with no external scanner: it recognises lines and
the tokens in them, and leaves indentation alone. Structure comes from the
language server, which has the resolved program.

That is a deliberate trade. An indentation-sensitive grammar needs a C scanner
tracking INDENT and DEDENT, and it would duplicate — imperfectly — what the
compiler already does. Since every editor using this grammar also runs the
server, the grammar only has to be right about tokens.

`src/parser.c` is generated, not committed. Run `tree-sitter generate` in
`tree-sitter-piton` to build it.
