# Protocol Testing

## Description

How the language server's rules are tested

## Rules

- Every rule in this scope has a test in `crates/piton-lsp`, named for the behaviour it checks.
- Feature tests build a real project in a temporary directory, so resolution, evaluation, and the file system are all exercised.
- Every request and notification the server handles is also tested through the server itself, as JSON-RPC, so what an editor receives is what is checked.
- An editor's message order is part of the behaviour, so tests send requests back to back and apply edits to buffers before notifying the server, the way editors do.
- What a feature must not do is tested as carefully as what it must, including what completion must not offer and what rename must refuse.

## Reproducing

A problem reported from an editor is reproduced before it is fixed. Run the installed `piton lsp` over stdio against a copy of the project, sending the messages the editor sends, and never against the author's own files. When a report shows mangled text, work out which edit applied to which text produces exactly that result before deciding where the bug is. When it cannot be reproduced, record the real session by wrapping the server so everything it receives and sends is written to a log, and reproduce from the log.
