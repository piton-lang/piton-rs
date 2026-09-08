# Menu Bar

The menu bar, rendered from the action table

The top strip of the header. It renders @../../../.claude/reference/shape/app/ActionCatalogue.md as menus, in the order the table gives, and does nothing else. Adding a menu item means adding a row to the table; it does not mean editing this file.

## Structure

- Draw it inside the header panel, above the toolbar, using a horizontal menu bar
- One top level menu per group in the table, labelled File, Edit, Format, View and Help
- Items in a group keep the table's order, and a run of dashes in the table becomes a separator
- An item that opens a dialog shows a trailing ellipsis; the ellipsis is added here, not stored in the label
- A checkable item, such as word wrap or the status bar, draws a check mark from the setting it reflects
- Open Recent is a submenu built from the recent files list, and is disabled when the list is empty

## Behaviour

- Every item shows its shortcut, right aligned and dimmed, from the same row that provides its label
- An item whose enabled rule is false is drawn greyed and does not respond
- Choosing an item queues the action and closes the menu in the same frame
- Alt opens the first menu and the arrow keys walk them; Escape closes without running anything

## Notes

- The bar keeps no state of its own beyond which menu is open, and egui owns even that
- The labels are the only strings this file holds, and they come from the table


# Toolbar

The single row of common actions under the menu bar

The lower strip of the header. It shows the few actions that are worth a button, as icon and label pairs, and nothing that is not already in @../../../.claude/reference/shape/app/ActionCatalogue.md.

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

- Line icons at one weight, sized to the token type scale, tinted with the foreground colour from @../../../.claude/reference/shape/theme/Tokens.md
- The separator is a hairline with the token spacing step either side of it, not a gap alone
- The zoom readout at the right end is a button too, and clicking it resets the zoom
