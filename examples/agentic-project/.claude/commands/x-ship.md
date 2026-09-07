---
description: Runs the tests and the Piton checks, then reports what is left
allowed-tools: Bash, Read, Grep
model: sonnet
---

Run the test suite and `piton build check`. Report what failed and stop; do not fix anything without being asked.

# Checks

- cargo test
- piton build check
- piton format . --check
