# Code Actions

## Description

Fixes for the problems the compiler reports, and whole-file import tidying

## Quick Fixes

- A name the compiler cannot find, in an expression, after `extends`, or in a constraint, offers an import from each module that exports it, the preferred specifier first and marked as preferred, as [ImportSpecifiers](ImportSpecifiers.md) describes.
- A keyword the file cannot see offers a `use` line for each module that exports it.
- A concrete anchor that has not implemented every abstract property offers to write the missing ones, each with a placeholder suited to its constraint, at the end of the anchor's body and at the body's indentation, creating the body when there is none.
- An unused import item or `use` line offers to remove it, and removes the whole line when nothing else is left on it.

## Source Actions

- Remove unused imports removes every unused import item and `use` line in the file, and is offered only when there is one.
- Organize imports removes the unused imports and writes every remaining import the way `piton format` writes it, without reordering the lines.

## Rules

- A fix is offered only for a diagnostic that overlaps the range the editor asks about, and the same fix is offered once however many diagnostics share it.
- Formatting is not a code action, because the editor already has a formatting command, and a lightbulb that is always lit gets ignored.

Links in this document point at reference files. Read one when the work touches what it describes.
