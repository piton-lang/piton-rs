# Responsiveness

## Description

Answering while the author types, however large the project grows

## Rules

- A change marks the analysis stale, and the next request that needs it rebuilds it once, so changes nothing asks about cost nothing.
- Diagnostics are published after the editor has been quiet for 150 milliseconds, so a burst of keystrokes compiles once.
- A file on disk is read again only when its size or modification time has changed since it was last read.
- The symbol model that definition, references, highlighting, hover, rename, and unused imports share is built once per analysis, not once per request.
- On a workspace of 150 files, a request made after a change is answered within 50 milliseconds.
