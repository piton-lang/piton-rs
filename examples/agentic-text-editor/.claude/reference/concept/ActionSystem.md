# Action System

## Summary

Every command the editor has is one row in one table. Menus, toolbar buttons and keyboard shortcuts are three renderings of that table, not three lists that have to be kept in step.

## Parts

- @../shape/app/ActionCatalogue.md - the table itself, grouped the way the menus are
- @../shape/ui/header/MenuBar.md - renders the groups as menus
- @../shape/ui/header/Toolbar.md - renders the handful marked as common
- @../shape/ui/footer/StatusBar.md - reports what the last action did, when it has something to say

## Rules

- An action has an id, a label, a group, an optional shortcut, an enabled rule and one function that performs it
- Adding an action means adding a row; if a menu item and a shortcut can disagree, the design is wrong
- Whether an action is enabled is derived from the document, never stored, so a greyed menu item and a dead shortcut cannot drift apart
- A disabled action does nothing at all when its shortcut is pressed; it does not beep and it does not half-run
- Shortcuts are matched before the editor surface sees the key, so typing into the buffer can never swallow one
