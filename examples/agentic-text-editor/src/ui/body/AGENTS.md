# Editor Surface

The text area itself, and everything that happens inside it

The surface is a multiline text edit that fills the body, backed by the buffer in @../../../.claude/reference/shape/document/Document.md. It renders the text and reports edits; it does not own the text and it does not decide what an edit means.

## Construction

- A multiline text edit inside a scroll area, sized to fill the available space
- The frame is off; the surface is the same colour as the body, with the token gutter of padding around the text
- Word wrap follows the setting, and with wrap off the scroll area scrolls horizontally too
- The font is the configured monospace family at the configured size, multiplied by the zoom
- The caret and the selection use the accent colour from @../../../.claude/reference/shape/theme/Tokens.md, and the selection stays visible when focus is elsewhere

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


# Find Replace Panel

The docked panel that holds find, and replace when it is asked for

One panel with two modes. Find shows a query field; replace shows the query field and a replacement field underneath it. Switching mode keeps the query, and closing the panel keeps it too.

## Layout

- Docked at the top of the body, above @../../../.claude/reference/shape/ui/body/EditorSurface.md, pushing it down rather than covering it
- The query field takes the width that is left after the buttons
- Find mode buttons, in order, are previous, next, and close
- Replace mode adds Replace and Replace All beside the replacement field
- The match count sits inside the right end of the query field, reading like `3 of 17`, or `No results`

## Behaviour

- Opening with a selection of one line or less fills the query with it and selects the field contents
- The query is applied as it is typed, and the first match at or after the caret becomes the active one
- Enter is next, Shift and Enter is previous, and Escape closes the panel
- Case sensitivity and whole word are two toggles inside the field, remembered for the session
- Wrapping past the last match is allowed, and the footer says that the search wrapped
- Replace All is a single undo step, and the footer reports how many it changed
- Closing puts the caret and the scroll position back where the surface had them when the panel opened


# Go To Line Dialog

The small modal for jumping the caret to a line number

A modal window centred over the body, holding one number field, a Go button and a Cancel button. It exists because a line number is the one piece of navigation that does not deserve a docked panel.

## Behaviour

- Opens with the current line number in the field, selected, and the field focused
- Accepts digits only, and Go is disabled while the field is empty
- Enter is Go and Escape is Cancel
- A number past the end of the document goes to the last line, and the footer says which line it landed on
- Going moves the caret to the start of the line and scrolls @../../../.claude/reference/shape/ui/body/EditorSurface.md so the line sits in the middle third
- The rest of the window is dimmed while it is open and does not respond to the keyboard
