# Piton for Kate

Copy `piton.xml` to `~/.local/share/org.kde.syntax-highlighting/syntax/` and
restart Kate.

`piton.xml` picks Kate's stock `normal` indenter, which has no rules for this
language, so both behaviors come from the language server: enter after a line
that opens a block lands one level in (on-type formatting), and enter on a
blank line inside a dictionary or anchor lands one level back, also through
on-type formatting.

Autoformat on save is the LSP Client's **Format on save** setting — the
`autoFormatOnSave` option, off until you enable it — and the formatter leaves
commented content alone.

For the language server, add to Kate's **LSP Client** settings:

```json
{
  "servers": {
    "piton": {
      "command": ["piton", "lsp"],
      "url": "https://github.com/mctavishdynamics/piton",
      "highlightingModeRegex": "^Piton$",
      "rootIndicationFileNames": ["piton.config.pi"]
    }
  }
}
```
