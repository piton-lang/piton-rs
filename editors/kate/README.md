# Piton for Kate

Copy `piton.xml` to `~/.local/share/org.kde.syntax-highlighting/syntax/` and
restart Kate.

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
