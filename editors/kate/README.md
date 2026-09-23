# Piton for Kate

Copy `piton.xml` to `~/.local/share/org.kde.syntax-highlighting/syntax/` and
restart Kate. That is the highlighting (`syntaxHighlighter:
KSyntaxHighlighting`).

## Language server

Add to Kate's **LSP Client** settings (**Settings → Configure Kate → LSP
Client → User Server Settings**):

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

`piton` must be on `PATH`; install it with `cargo xtask install`.

## Editor behaviour

`piton.xml` picks Kate's stock `normal` indenter, which only carries the
previous line's indentation forward. The two Enter rules come from the
language server's on-type formatting, which Kate only asks for when it is
turned on:

**Settings → Configure Kate → LSP Client → Client Settings**, enable
**Format on typing**.

With it on:

- **Enter on a colon line** — a declaration, or a dictionary or anchor
  property with nothing after its colon — lands one level in.
- **Enter on a blank line** inside a dictionary or anchor lands one level
  back.

A list item or a merge line never opens a block, even when it ends in a colon
(`- Settings:` is just a string).

Kate's own `python` indenter is not a substitute: it indents after any line
ending in a colon, list items included.

Autoformat on save is the LSP Client's **Format on save** setting on the same
page — the `autoFormatOnSave` option, off until you enable it. The formatter
adds the space after `//` and touches nothing else that is commented.
