# View Settings

## Summary

Word wrap, font, zoom, theme and whether the footer is showing. None of them touch the document, all of them are remembered between runs, and each one is a single value.

## Parts

- [EditorSurface](../shape/ui/body/EditorSurface.md) - where wrap, font and zoom actually take effect
- [StatusBar](../shape/ui/footer/StatusBar.md) - reports the zoom level, and can be switched off
- [Theme](../shape/theme/Theme.md) - light, dark, or whatever the system is set to

## Settings

```
wordWrap: Off by default. Off means the surface scrolls sideways.
fontFamily: A monospace family, chosen from those the system has
fontSize: Points, from eight to seventy-two
zoom: A multiplier over the font size, from a quarter to five times
theme: Light, Dark, or System
showStatusBar: On by default
```

## Rules

- A setting is written when it changes and read once at startup; nothing else reads the file
- Zoom is separate from font size, so zooming out and back lands exactly where it started
- A setting file that is missing, unreadable or from an older version falls back to the defaults without a dialog
- Changing a setting never moves the caret or the scroll position
- What the settings look like once applied is [ModernStyle](ModernStyle.md)

Links in this document point at reference files. Read one when the work touches what it describes.
