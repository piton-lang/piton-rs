# Layout

## Summary

The window is a header, a body and a footer, stacked in that order. The header and the footer are as tall as their contents; the body takes everything that is left over.

## Parts

- Header - [MenuBar](../shape/ui/header/MenuBar.md), and [Toolbar](../shape/ui/header/Toolbar.md) underneath it
- Body - [EditorSurface](../shape/ui/body/EditorSurface.md), with [FindReplacePanel](../shape/ui/body/FindReplacePanel.md) docked above it and [GoToLineDialog](../shape/ui/body/GoToLineDialog.md) floating over it
- Footer - [StatusBar](../shape/ui/footer/StatusBar.md), one line tall

## Rules

- The body is the only band that scrolls, and it scrolls the text, not the layout
- A band that is switched off in [ViewSettings](ViewSettings.md) gives its height to the body and takes it back unchanged when it returns
- Panels inside the body push the editor surface down rather than covering it, so the caret never ends up underneath something
- The window title is the file name, then a space, then the application name; an unsaved buffer puts a bullet in front of the file name
- Every band draws itself from the same tokens, so the seams between them are the only thing that separates them; see [ModernStyle](ModernStyle.md)

## Focus

The editor surface holds focus unless a panel has explicitly taken it. Every panel gives focus back to the surface when it closes, at the caret position the surface had before it opened.

Links in this document point at reference files. Read one when the work touches what it describes.
