# Status Bar

The one line footer, and what it is allowed to say

A single row across the bottom of the window, showing what is true about the document right now. It reads state and never changes it, apart from the two readouts that are also buttons.

## Fields

- At the left, the transient message, if there is one
- Then, pushed to the right, the caret position, reading `Ln 12, Col 4`
- The selection size when there is one, reading `12 selected` or `3 lines selected`
- The line ending, from @../../files/Encoding.md, reading CRLF or LF, and clicking it offers the other
- The character encoding, reading UTF-8 or UTF-8 with BOM, and clicking it offers the other
- The zoom, as a percentage, and clicking it restores one hundred per cent

## Behaviour

- The message is set by whatever just happened, and clears itself after four seconds or on the next keystroke
- A failure message stays until it is replaced, and is drawn in the danger colour
- The readouts update every frame from @../../document/Document.md; nothing here is cached
- Hiding the footer from the View menu removes the row entirely and gives its height back to the body
- The row is one text line tall plus the token spacing step above and below, and never wraps

## Notes

- Fields are separated by space, not by pipes or borders
- The two clickable readouts look like text until they are hovered, and then take the same surface tint as a toolbar button
