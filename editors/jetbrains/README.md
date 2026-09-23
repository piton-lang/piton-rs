# Piton for JetBrains IDEs

The specification lists JetBrains' highlighter as `jetbrains` — a native
highlighter. There is none yet; see [Limitations](#limitations). What works
today needs no compiled plugin: highlighting from a TextMate bundle and
everything else from `piton lsp` through the LSP4IJ plugin.

`piton` must be on `PATH`; install it with `cargo xtask install`.

## Highlighting

The TextMate bundle is `editors/vscode`. An extension directory with a
`package.json` and a `syntaxes/` folder is a valid bundle.

**Settings → Editor → TextMate Bundles → +** and select `editors/vscode`.

## Language server

Install the **LSP4IJ** plugin (Red Hat) from the Marketplace. It works in every
JetBrains IDE, Community editions included. The IDE's own LSP API is not an
option here: it can only be used from a compiled plugin.

Import the template in this directory:

**Settings → Languages & Frameworks → Language Servers → +** →
**Template: Import from custom template…** → select
`editors/jetbrains/lsp4ij-template`.

Or fill the fields in by hand:

| Field | Value |
| --- | --- |
| Name | Piton |
| Command | `piton lsp` |
| Mappings → File name patterns | `*.pi`, language id `piton` |

## Editor behaviour

- **Enter on a colon line** (a new dictionary or anchor property) should land
  one level in, and **Enter on a blank line** inside a dictionary or anchor
  one level back. The IDE has no Piton indent rules of its own; the language
  server answers both through `textDocument/onTypeFormatting`, which LSP4IJ
  forwards on Enter in the versions that support it (check **Language
  Servers → Piton → Debug** traces if nothing happens). A list item or a
  merge line never opens a block (`- Settings:` is just a string).
- **Autoformat on save** is an option. LSP4IJ plugs the server's
  `textDocument/formatting` into the IDE's **Code → Reformat Code** action, so
  turning on **Settings → Tools → Actions on Save → Reformat code** formats
  Piton files on save. Leave it off to keep formatting manual. The formatter
  adds the space after `//` and touches nothing else that is commented.

## Limitations

- Highlighting is TextMate, not a native JetBrains highlighter
  (`syntaxHighlighter: jetbrains`). A native one is a Gradle plugin with its
  own lexer (and, for anything structural, a parser and PSI): a second
  implementation of the language's lexical rules to keep in step with the
  compiler. Until that exists, semantic tokens from the server refine the
  TextMate colours.
- The Enter rules depend on LSP4IJ's on-type formatting support; without it,
  Enter keeps the previous line's indentation.
