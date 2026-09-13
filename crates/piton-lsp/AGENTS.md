# Language Server Crate

The Piton language server, run as `piton lsp`

This crate implements [LspScope](../../.claude/reference/scope/lsp/LspScope.md), and every behaviour it has is a rule in that spec. When a behaviour has to change, change the Piton source under `spec/scope/lsp` first, run `piton build`, and then change the code to match.
What a name, a module specifier, or a problem means is decided in `crates/piton-core`. When a feature here needs an answer the compiler does not give yet, add it to the compiler instead of working it out here.

## Layout

- `src/lib.rs` connects protocol requests to features and decides nothing itself
- `src/world.rs` owns the workspace, its projects, and the analysis every request reads
- `src/index.rs` is the symbol model that definition, references, highlighting, hover, rename, and unused imports share
- `src/model.rs` answers the compiler's questions about scopes, inheritance, and members that the symbol model is built from
- `src/navigation.rs`, `src/hover.rs`, `src/completion.rs`, `src/actions.rs`, `src/imports.rs`, `src/refactor.rs`, and `src/tokens.rs` implement the features their names say
- `src/diff.rs` turns a formatted document into edits that touch only the lines that changed
- `src/tests.rs` and `src/model_tests.rs` test features against real projects on disk, and `src/protocol_tests.rs` tests the server through JSON-RPC

Links in this document point at reference files. Read one when the work touches what it describes.
