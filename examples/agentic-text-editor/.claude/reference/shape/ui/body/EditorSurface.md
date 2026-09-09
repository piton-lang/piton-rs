# Editor Surface

The text area itself, and everything that happens inside it

The surface is a multiline text edit that fills the body, backed by the buffer in [Document](../../document/Document.md). It renders the text and reports edits; it does not own the text and it does not decide what an edit means.

## Construction

- A multiline text edit inside a scroll area, sized to fill the available space
- The frame is off; the surface is the same colour as the body, with the token gutter of padding around the text
- Word wrap follows the setting, and with wrap off the scroll area scrolls horizontally too
- The font is the configured monospace family at the configured size, multiplied by the zoom
- The caret and the selection use the accent colour from [Tokens](../../theme/Tokens.md), and the selection stays visible when focus is elsewhere

## Responsibilities

- Report the caret line and column every frame, for the footer to show
- Scroll the caret back into view whenever an action moves it, into the middle third rather than just to the edge
- Highlight every match of the current query, with the active match drawn more strongly than the rest
- Accept a dropped file by queueing an open, and never by inserting the path as text
- Insert a literal tab when Tab is pressed with the surface focused; Tab only moves focus when the surface does not have it

## Notes

- The surface never allocates a copy of the buffer per frame; it borrows it
- A file large enough to be slow to lay out is still opened, and the surface degrades by turning off wrapping rather than by refusing
- Undo grouping is the history's business, not the surface's; the surface reports edits as they arrive

Links in this document point at reference files. Read one when the work touches what it describes.
