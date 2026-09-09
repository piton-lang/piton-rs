---
name: add-editor-action
description: Adds an editor action and wires it through the table, the menus, the toolbar and the shortcuts Use when the user asks for a new command, menu item, toolbar button or keyboard shortcut
---

Read [ActionSystem](../../reference/concept/ActionSystem.md) first. An action is one row in one table, and the menus, the buttons and the shortcuts are renderings of that table. If you find yourself editing [MenuBar](../../reference/shape/ui/header/MenuBar.md) to add a menu item, you are doing it the wrong way round.

# Order

- Add the row to [ActionCatalogue](../../reference/shape/app/ActionCatalogue.md), editing the Piton source under `spec/shape/app` rather than the generated Markdown
- Run `piton build`, so the documents beside the code say the action exists
- Add the matching entry to the action table in the code, with its id, label, group, shortcut, enabled rule and function
- Write the function, in the module that owns the state it changes, not in the table
- Add a test for the enabled rule and a test for what the action queues

# Checks

- The id reads `group.verb` and matches its neighbours
- The shortcut is free; check the whole table, not just the group
- The enabled rule is derived from the document and is not stored anywhere
- The action belongs on [Toolbar](../../reference/shape/ui/header/Toolbar.md) only if it is one of the few someone reaches for constantly
- The label is title case, and the ellipsis is added by the menu when a dialog follows, not stored in the label

Links in this document point at reference files. Read one when the work touches what it describes.
