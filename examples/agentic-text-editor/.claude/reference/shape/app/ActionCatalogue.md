# Action Catalogue

## File

- file.new - New - Ctrl+N - empties the buffer and forgets the path, after the unsaved check
- file.open - Open - Ctrl+O - native open dialog, after the unsaved check
- file.openRecent - Open Recent - no shortcut - a submenu, one entry per remembered path
- file.save - Save - Ctrl+S - writes to the current path, or becomes Save As when there is none
- file.saveAs - Save As - Ctrl+Shift+S - always opens the save dialog
- file.exit - Exit - Ctrl+Q - closes the window, after the unsaved check

## Edit

- edit.undo - Undo - Ctrl+Z - steps back one group in the undo history
- edit.redo - Redo - Ctrl+Y - steps forward again, until the next edit clears the redo stack
- edit.cut - Cut - Ctrl+X - copies the selection and deletes it; disabled with no selection
- edit.copy - Copy - Ctrl+C - copies the selection; disabled with no selection
- edit.paste - Paste - Ctrl+V - replaces the selection with the clipboard; disabled when it holds no text
- edit.delete - Delete - Delete - deletes the selection, or the character after the caret
- edit.selectAll - Select All - Ctrl+A - selects the whole buffer
- edit.find - Find - Ctrl+F - opens the find panel in find mode with the selection as the query
- edit.findNext - Find Next - F3 - moves to the next match without opening the panel
- edit.findPrevious - Find Previous - Shift+F3 - moves to the previous match
- edit.replace - Replace - Ctrl+H - opens the same panel in replace mode
- edit.goTo - Go To - Ctrl+G - opens the go to line dialog
- edit.insertDateTime - Time and Date - F5 - inserts the local time and date at the caret

## Format

- format.wordWrap - Word Wrap - no shortcut - checkable, toggles wrapping in the editor surface
- format.font - Font - no shortcut - opens the font settings

## View

- view.zoomIn - Zoom In - Ctrl+= - one step up the zoom scale
- view.zoomOut - Zoom Out - Ctrl+- - one step down
- view.zoomReset - Restore Default Zoom - Ctrl+0 - back to one hundred per cent
- view.statusBar - Status Bar - no shortcut - checkable, shows or hides the footer
- view.theme - Theme - no shortcut - a submenu of Light, Dark and System

## Help

- help.about - About - no shortcut - name, version, and the licence

## Row

id: Stable, lower case, `group.verb`, and never reused for something else
label: Title case, no trailing ellipsis in the data; the menu adds one where a dialog follows
group: The menu it belongs to, and the order within the menu is the order here
shortcut: One chord, or none; matched before the editor surface sees the key
enabled: A function of the document, evaluated fresh each frame
run: One function, taking the application state, returning nothing
