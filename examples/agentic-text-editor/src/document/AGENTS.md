# Document

The open document, its path, its dirty flag and its caret

One struct, holding one document. Everything else in the editor reads it and asks it to change; nothing else keeps a second copy of the text.

## Fields

text: The whole buffer as one string
path: Where it came from, or none for an untitled document
dirty: Set by any edit, cleared only by a successful save or a new document
caret: A byte offset into the buffer
selection: The other end of the selection, or none when the caret is alone
encoding: How it was read and how it will be written; see @../../.claude/reference/shape/files/Encoding.md
lineEnding: CRLF or LF, taken from the file and kept for the write

## Rules

- Line and column are derived from the caret when they are asked for, and are one based for display
- Column counts characters, not bytes, so the footer is right in a file that is not all ASCII
- The buffer holds LF only; the line ending is applied on write and stripped on read
- Untitled documents are dirty as soon as they are typed into, and never before
- Replacing the document is one operation that swaps every field at once, so no frame can see half of a load

## Notes

- Edits go through methods that record themselves in the undo history; nothing mutates the string directly
- Insert and delete take byte ranges and are the only two primitives; everything else is written in terms of them


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
