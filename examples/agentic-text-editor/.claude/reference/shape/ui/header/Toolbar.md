# Toolbar

The single row of common actions under the menu bar

The lower strip of the header. It shows the few actions that are worth a button, as icon and label pairs, and nothing that is not already in @../../app/ActionCatalogue.md.

## Contents

- New, Open and Save, then a separator
- Undo and Redo, then a separator
- Cut, Copy and Paste, then a separator
- Find and Replace
- Pushed to the right end, the theme toggle and the zoom level

## Behaviour

- Each button is an icon above nothing, with its label beside it while there is room
- Below four hundred points of width the labels drop and the buttons stay, in the order above
- A button whose action is disabled is dimmed and untouchable, and keeps its tooltip
- The tooltip is the label followed by the shortcut in parentheses
- Hover tints the button's surface; it does not grow, shift or gain a border

## Notes

- Line icons at one weight, sized to the token type scale, tinted with the foreground colour from @../../theme/Tokens.md
- The separator is a hairline with the token spacing step either side of it, not a gap alone
- The zoom readout at the right end is a button too, and clicking it resets the zoom
