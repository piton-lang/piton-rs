---
name: debug-lsp
description: Reproduces and fixes language server behaviour reported as wrong from an editor Use when someone reports that hover, go to definition, references, rename, completion, moving files, diagnostics, or any other editor feature does the wrong thing in a Piton file
---

Find the rule in [LspScope](../../reference/scope/lsp/LspScope.md) that covers the behaviour before reading any code. If no rule covers it, the spec is incomplete: write the rule first, because a fix without one has nothing to be checked against.
Then reproduce the report, following [ProtocolTesting](../../reference/scope/lsp/testing/ProtocolTesting.md). Drive the installed `piton lsp` over stdio against a copy of the author's project, sending the messages their editor sends. Never run a reproduction against the author's own files.

# Order

- Find or write the rule the report breaks
- Reproduce it against the installed binary, on a copy of the project
- When text came out mangled, work out which edit applied to which text produces exactly that result
- Remember that editors apply edits to their buffers and ask the next question before telling the server, and send one request per file for a multi-file move
- Write a test that fails for the same reason, at the level the problem happens
- Fix it, run the language server tests, and check the installed binary again

# Cannot Reproduce

Ask for a recording of the real session. Point the editor at a small script that runs `piton lsp` with `tee` on both its input and its output, so every message is written to a log, and reproduce from the log rather than from a guess about what the editor sent.

Links in this document point at reference files. Read one when the work touches what it describes.
