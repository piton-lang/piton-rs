# Application

## Summary

A single-window plain text editor that does what Notepad does and looks like something written this decade. One window, one open document, no tabs, no projects, no plugins.

## Parts

- @Layout.md — the three bands the window is made of
- @DocumentModel.md — the text being edited, and what is known about it
- @ActionSystem.md — the one table that every menu, button and shortcut reads
- @FileLifecycle.md — new, open, save, save as, and never losing work
- @SearchAndNavigation.md — find, replace and go to line
- @ViewSettings.md — word wrap, font, zoom, and which bands are showing
- @ModernStyle.md — how all of it should look

## Principles

- The document is the only state that matters; every other field is a view of it or a preference about it
- Nothing is reachable by the mouse alone
- No edit is lost without the user being asked first
- A frame does not block; anything that touches the disk finishes before the next frame or reports why it could not
- The editor opens on an empty buffer in under a second and stays that quick with a large file open

## Out Of Scope

- Tabs, split panes and more than one document at a time
- Syntax highlighting, completion and anything that parses the text
- Printing, macros and plugins
- Remote or networked files
