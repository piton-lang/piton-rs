# Piton for Sublime Text

Copy `Piton.sublime-syntax` into `Packages/User/`.

The syntax file carries highlighting only — Sublime syntaxes have no indent
rules — so both editor behaviors come from the language server's on-type
formatting: enter after a line that opens a block lands one level in, and
enter on a blank line inside a dictionary or anchor lands one level back.

Autoformat on save is the LSP package's `lsp_format_on_save` setting, the
`autoFormatOnSave` option, and it is `false` unless you turn it on. When it
runs, the formatter leaves commented content alone.

For the language server, install LSP from Package Control and add to
`LSP.sublime-settings`:

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
