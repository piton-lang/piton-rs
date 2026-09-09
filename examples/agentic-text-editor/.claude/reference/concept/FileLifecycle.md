# File Lifecycle

## Summary

Four commands move a document in and out of the editor, and one question guards all of them. Anything that would discard unsaved work asks first, every time, in the same words.

## Parts

- [FileDialogs](../shape/files/FileDialogs.md) - the native open and save dialogs, and the confirmation
- [Encoding](../shape/files/Encoding.md) - reading and writing bytes, and what to do when they are not text
- [RecentFiles](../shape/files/RecentFiles.md) - the list under the File menu, and where it is kept
- [Document](../shape/document/Document.md) - what actually changes when any of this succeeds

## Guard

New, Open, opening a recent file and quitting all pass through the same check. If the document is dirty the editor asks whether to save, and the answer is Save, Discard or Cancel. Cancel abandons the command entirely and leaves the document exactly as it was.

## Rules

- Save on an untitled document is Save As, and returns to the caller only once a path exists
- A failed write leaves the document dirty and the old file untouched, and says what went wrong in the footer
- Opening a file that is already open reloads it, and still asks first if there are unsaved edits
- A file dropped onto the window is an Open, guard and all
- Quitting is the only command that can be refused; every other failure is reported and survivable
- The document's state after any of this is described by [DocumentModel](DocumentModel.md)

Links in this document point at reference files. Read one when the work touches what it describes.
