# Undo History

The record of edits, how they are grouped, and how far back it goes

A stack of edits and a position in it. Undo walks back, redo walks forward, and the next edit made after an undo throws away everything in front of it.

## Grouping

- Consecutive typing is one group until the caret moves, a second passes, or the character typed is a newline
- A deletion in one direction is one group, and reversing direction starts a new one
- A paste is always its own group, however small
- Replace All is one group, however many matches it touched
- Loading a file clears the history rather than becoming a group in it

## Rules

- Undo and redo restore the caret and the selection as they were before the edit, not just the text
- The dirty flag is part of what a group restores, so undoing back to the last save leaves the document clean
- The stack is bounded, and drops from the oldest end; the bound is a constant in this module
- An edit that changes nothing is not recorded

## Notes

- Store the text that was removed and the text that was inserted, with the offset, and nothing else; a whole buffer per step is not affordable
