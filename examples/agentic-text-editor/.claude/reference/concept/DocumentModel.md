# Document Model

## Summary

The editor holds exactly one document. It is a string, a path that may not exist yet, a dirty flag, and the undo history that explains how it got this way.

## Parts

- @../shape/document/Document.md — the buffer, the path, the dirty flag and the caret
- @../shape/document/UndoHistory.md — the record of edits, and how they are grouped
- @../shape/files/Encoding.md — how the bytes on disk became this string, and how they go back

## Rules

- The buffer is the truth; the widget renders it and hands back edits, it does not own the text
- The dirty flag is set by an edit and cleared only by a successful save or by a new document
- A document with no path is untitled, is dirty from its first keystroke, and cannot be saved without asking where
- Reloading, closing and replacing the document all go through the file lifecycle, never straight through the buffer
- The caret and the selection are part of the document's state, so navigating and editing can be reasoned about together
