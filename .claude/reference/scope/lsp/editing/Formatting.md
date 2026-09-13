# Formatting

## Description

Applying `piton format` to a document without disturbing what did not change

## Rules

- Formatting a document applies `piton format`, the same formatter the command line uses.
- The edits cover only the lines that changed, so the cursor, selections, and folds elsewhere in the file stay where they were.
- A document that is already formatted produces no edits.
- Text the parser could not read is kept exactly as it was written.
