# Document

The open document, its path, its dirty flag and its caret

One struct, holding one document. Everything else in the editor reads it and asks it to change; nothing else keeps a second copy of the text.

## Fields

text: The whole buffer as one string
path: Where it came from, or none for an untitled document
dirty: Set by any edit, cleared only by a successful save or a new document
caret: A byte offset into the buffer
selection: The other end of the selection, or none when the caret is alone
encoding: How it was read and how it will be written; see @../files/Encoding.md
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
