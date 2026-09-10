# App

The application shell, the state it owns, and the order of a frame

This directory holds the `eframe::App` implementation and the one struct that owns the editor's state. It draws nothing itself. Each band asks a function in `ui` to draw it, and those functions get the state and the action table, never the other way round.

## State

```
document: The buffer, path, dirty flag and caret; see the document module
history: The undo stack that belongs to that document
settings: The view preferences, loaded once at startup
search: The current query, its mode, and where the last match was
pending: The command waiting on an unsaved-changes answer, if any
toast: The message the footer is currently showing, and when it expires
```

## Frame Order

- Take the input the window received, and run any shortcut in [ActionCatalogue](ActionCatalogue.md) that matches
- Draw the header, then the footer, then the body, so egui gives the leftover space to the body
- Run whatever the frame's widgets queued, in the order it was queued
- Write settings back to disk if any of them changed this frame

## Notes

- Actions are queued and run after the UI, never in the middle of drawing it; a menu item that mutates the document mid-frame will fight the widget that is rendering it
- The shell requests a repaint only when something is animating or a file operation is outstanding; egui is otherwise left to idle
- The window title is recomputed from the document each frame and set through the viewport command, so the dirty bullet can never go stale
- A file operation that fails becomes a message in the footer and nothing else; the shell has no error dialogs

Links in this document point at reference files. Read one when the work touches what it describes.
