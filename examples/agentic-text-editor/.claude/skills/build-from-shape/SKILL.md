---
name: build-from-shape
description: Implements a part of the editor from its shape document Use when the user asks to build or change any part of the editor
---

Every directory under `src` has an `AGENTS.md` beside the code, and the same documents are published together under .claude/reference/shape. Read the one for the part you are about to touch before you write anything, then read @../../reference/concept/Application.md for how that part is meant to fit the rest.
Build what the documents describe and stop there. Where they are silent on something you need, say so and ask; do not invent behaviour and leave it for a reviewer to find.

# Order

- Read the shape document for the directory you are about to edit
- Read the concepts it points at, far enough to know what it is for
- Write the code
- Run the tests for that directory, then `piton build check`

# Stack

- Rust, stable, two thousand and twenty-four edition
- `eframe` and `egui` for the window and the widgets, and no second UI crate
- `rfd` for the native dialogs, `serde` for the settings file, and as little else as will do
- No unsafe, and no blocking call inside a frame

# Conventions

- One module per directory, and the directory's shape document names what belongs in it
- Drawing code returns nothing and mutates nothing; it queues actions
- Errors that a user should see become a sentence for the footer, and errors that they should not become a log line
- A change that contradicts a shape document is a change to the Piton source first, and to the code second
