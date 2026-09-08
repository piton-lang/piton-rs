# Search And Navigation

## Summary

Three ways of moving the caret somewhere it is not. Find and replace share one panel, because they share a query; go to line is a small modal of its own, because it does not.

## Parts

- @../shape/ui/body/FindReplacePanel.md - the docked panel, in either of its two modes
- @../shape/ui/body/GoToLineDialog.md - the modal for jumping to a line number
- @../shape/ui/body/EditorSurface.md - what scrolls, highlights and takes the caret back afterwards
- @../shape/ui/footer/StatusBar.md - where a wrapped search or a count of replacements is reported

## Rules

- The query survives the panel being closed and reopened, and survives switching between find and replace
- Search wraps at the end of the buffer and says so in the footer rather than stopping silently
- A match is scrolled into the middle third of the view, not just barely into it
- Replace All is one undo step, and reports how many it changed
- Go To rejects a line number outside the document by clamping to the nearest end and saying which
- Closing either panel restores the caret and the scroll position the surface had before it opened
