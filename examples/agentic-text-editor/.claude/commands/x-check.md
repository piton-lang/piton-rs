---
description: Runs the Rust checks and the Piton checks, then reports what is left
allowed-tools: Bash, Read, Grep
model: sonnet
---

Run every check below, in order, and report what failed with enough of the output to act on. Stop there. Do not fix anything without being asked.

# Checks

- cargo fmt --all --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test
- piton build check
- piton format . --check

# Notes

- A failing `piton build check` means the spec is wrong, not the code; say which document
- A `CLAUDE.md` or `AGENTS.md` that differs from a fresh `piton build` means someone edited generated output
