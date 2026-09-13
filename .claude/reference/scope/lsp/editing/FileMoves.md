# File Moves

## Description

Moving files and folders, with every import that names them following

## Summary

When the editor is about to move or rename a file or a folder, the server answers with the edits that keep every specifier pointing where it pointed, and the editor applies them in the same step as the move, so one undo reverts both. Specifiers are written as [ImportSpecifiers](ImportSpecifiers.md) describes.

## Rules

- Every specifier, in any project, that loads a moved file is rewritten, and so is every relative specifier written inside a moved file whose meaning the move changes.
- Files that move together keep the relative specifiers between them unchanged.
- A folder is one move, and every file under it moves with it.
- A move that involves no Piton file produces no edits.
- Unsaved buffers follow the move, so their text is neither lost nor read back from the old path.

## Batches

Editors move a selection of several files by asking about each file in turn, applying each answer to their buffers before asking about the next, and reporting the moves only afterwards. So a request that arrives while earlier moves are still unreported joins them in one batch. Every move in a batch is planned together, against the workspace as it was when the batch began, and each answer holds only what the batch adds, measured against the text the editor already holds.
A batch ends when the editor has reported every move in it, or when no request has joined it for three seconds. A request after a batch ends starts a new batch, planned against the workspace as it is then.

Links in this document point at reference files. Read one when the work touches what it describes.
