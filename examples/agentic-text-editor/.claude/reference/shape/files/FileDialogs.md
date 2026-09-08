# File Dialogs

The open and save dialogs, and the unsaved changes confirmation

The only place in the editor that opens a dialog the operating system owns, and the only place that reads or writes a document file. Nothing here draws with egui and nothing here blocks the frame.

## Dialogs

open: Native, filtered to text files with an all files entry, starting in the current document's directory
save: Native, defaulting to the current name or `Untitled.txt`, appending `.txt` when no extension is given
confirm: Three buttons, Save, Discard and Cancel, with Cancel as the default and Escape

## Wording

- The confirmation asks `Save changes to NAME?`, using the file name, or `Untitled` when there is none
- Beneath it, `Your changes will be lost if you do not save them.`
- Nothing else in the editor asks a question, so this wording is the only wording of its kind

## Rules

- A dialog runs off the frame and reports back; the editor keeps drawing, and ignores input for the document until the answer arrives
- Cancel abandons the whole command, not just the dialog
- A write goes to a temporary file beside the target and is renamed over it, so a failed write cannot truncate the original
- A read that is not valid text stops before touching the document and reports what was wrong
- Every failure returns a sentence fit for the footer, naming the file and what could not be done
