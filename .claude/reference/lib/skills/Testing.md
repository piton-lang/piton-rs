# Testing

## Pitch

Every rule in a spec is a claim a test can check, and every rule has a test. Work is not finished until the tests covering the rules it touches pass, and a bug is not fixed until a test that failed because of it passes.
Test what must not happen as carefully as what must. A feature that answers when it should stay silent is as broken as one that stays silent when it should answer.

## Commands

- `cargo test --workspace` runs every test
- `cargo test -p piton-lsp` runs the language server tests alone
- `cargo run -p piton-cli -- build check` checks this spec itself
