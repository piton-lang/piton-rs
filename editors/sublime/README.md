# Piton for Sublime Text

Copy `Piton.sublime-syntax` into `Packages/User/`.

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
