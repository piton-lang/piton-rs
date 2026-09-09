# Ui Conventions

What every widget in the editor is expected to do the same way

Everything under this directory draws and nothing under it decides. A drawing function takes the state it needs and the queue it can push an action onto, and it returns nothing. If a function here needs to know why something is happening, the decision belongs in `app` instead.

## Conventions

- One module per band, and one function per panel, named for what it draws
- No widget reads or writes a file, and no widget calls anything blocking
- Colours, spacing, radii and type sizes come from [Tokens](../../.claude/reference/shape/theme/Tokens.md) through the active style, never as literals
- Every interactive widget gets a stable id from its action id, so egui keeps focus across frames
- Anything that can be clicked can be reached by Tab, and shows a focus ring when it is
- A widget that is disabled is still drawn and still has its tooltip; it is never hidden instead
- Text that can be truncated is truncated at the end with an ellipsis, and gets a tooltip with the whole of it

## Testing

- Drawing functions are tested through `egui::__run_test_ui`, one test per state worth distinguishing
- A test asserts on what was queued, not on pixels

Links in this document point at reference files. Read one when the work touches what it describes.
