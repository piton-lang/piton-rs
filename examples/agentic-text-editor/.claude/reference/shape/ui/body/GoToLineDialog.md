# Go To Line Dialog

The small modal for jumping the caret to a line number

A modal window centred over the body, holding one number field, a Go button and a Cancel button. It exists because a line number is the one piece of navigation that does not deserve a docked panel.

## Behaviour

- Opens with the current line number in the field, selected, and the field focused
- Accepts digits only, and Go is disabled while the field is empty
- Enter is Go and Escape is Cancel
- A number past the end of the document goes to the last line, and the footer says which line it landed on
- Going moves the caret to the start of the line and scrolls [EditorSurface](EditorSurface.md) so the line sits in the middle third
- The rest of the window is dimmed while it is open and does not respond to the keyboard

Links in this document point at reference files. Read one when the work touches what it describes.
