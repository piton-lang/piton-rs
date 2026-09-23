# Piton for Sublime Text

Copy this directory to `Packages/Piton/` (**Preferences → Browse Packages…**).
The package name matters: the Enter key binding runs a macro by its
`Packages/Piton/` path.

| File | |
| --- | --- |
| `Piton.sublime-syntax` | highlighting (`syntaxHighlighter: sublime-syntax`) |
| `Piton.tmPreferences` | the colon indent rule, and `// ` for Toggle Comment |
| `Default.sublime-keymap`, `Enter on Blank Line.sublime-macro` | the blank-line dedent |
| `Piton.sublime-settings` | four spaces, not tabs |

## Editor behaviour

- **Enter on a colon line** — a declaration, or a dictionary or anchor
  property with nothing after its colon — lands one level in. That is
  `increaseIndentPattern` in `Piton.tmPreferences`. A list item or a merge line
  never opens a block, even when it ends in a colon (`- Settings:` is just a
  string), and neither does a comment.
- **Enter on a blank line** inside a dictionary or anchor lands one level
  back. Sublime has no indentation setting for this (its
  `decreaseIndentPattern` is tested against the new, empty line, so it would
  outdent every line), so it is a key binding scoped to `source.piton`: Enter
  on a whitespace-only line inserts the newline and unindents it once. A blank
  line with no whitespace on it has no depth to measure and gets a plain
  Enter.

Autoformat on save is the LSP package's `lsp_format_on_save` setting — the
`autoFormatOnSave` option — and it is `false` unless you turn it on. The
formatter adds the space after `//` and touches nothing else that is
commented.

## Language server

Install LSP from Package Control and add to `LSP.sublime-settings`:

```json
{
  "clients": {
    "piton": {
      "enabled": true,
      "command": ["piton", "lsp"],
      "selector": "source.piton"
    }
  }
}
```

`piton` must be on `PATH`; install it with `cargo xtask install`.
