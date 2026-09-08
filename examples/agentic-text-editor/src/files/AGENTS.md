# Encoding

How bytes become the buffer, and how the buffer goes back to bytes

The editor understands UTF-8, with or without a byte order mark, and nothing else. Whatever it read, it writes back the same way, because a text editor that silently converts a file is worse than one that refuses to open it.

## Reading

- A leading byte order mark is recorded and stripped from the buffer
- CRLF, LF and a mixture are all accepted; the buffer keeps LF only
- The line ending recorded is the one that appeared most in the file, and LF when the file has no line breaks
- Bytes that are not valid UTF-8 stop the read and produce a message naming the first offset that failed
- A file with no trailing newline stays that way, and one with a trailing newline keeps it

## Writing

- The recorded byte order mark is written back if it was there, and not added if it was not
- Every LF becomes the recorded line ending
- A new document is UTF-8 without a mark, and uses the platform's line ending

## Rules

- The two readouts in the footer show these two recorded values, and clicking either changes it and marks the document dirty
- Changing the encoding or the line ending is an edit like any other, and is undoable


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


# Recent Files

The list of recently opened paths, and where it is kept

A short list of absolute paths, newest first, kept beside the settings in the platform's configuration directory and written whenever it changes.

## Rules

- At most ten entries, and a path that is opened again moves to the front rather than repeating
- Only a successful open or save adds an entry; a cancelled dialog and a failed read do not
- The menu shows the file name, with the containing directory dimmed beside it, and the whole path as a tooltip
- Two entries with the same file name both show enough of their directories to tell them apart
- An entry whose file has gone is still shown, and choosing it reports that it is missing and drops it from the list
- The list ends with a separator and `Clear Recent`, which empties it after no confirmation, because nothing is lost
