# Modern Style

## Summary

Quiet, flat and roomy. The text is the only thing with contrast; the chrome around it recedes until it is used. Nothing is decorated that could just be spaced properly instead.

## Parts

- [Tokens](../shape/theme/Tokens.md) - the colours, spacing steps, radii and type scale, as values
- [Theme](../shape/theme/Theme.md) - how the tokens become an egui style, in light and in dark
- [UiConventions](../shape/ui/UiConventions.md) - how a widget is expected to use them

## Rules

- No widget hard-codes a colour, a size or a corner radius; every one of them comes from [Tokens](../shape/theme/Tokens.md)
- Chrome is separated by space and a change of surface, not by borders; there is at most one hairline in the window, under the header
- Hover and focus are a change of surface tint, never a change of size, so nothing moves under the pointer
- Corners are rounded once, at the token radius, and controls sharing a row share their radius
- Icons are line icons at a single weight, and every one of them has a text label somewhere, whether in a menu or a tooltip
- Focus is always visible, on a ring drawn outside the control so it cannot change the layout
- Nothing animates except opacity, and nothing animates for longer than a tenth of a second
- The window works at any size down to four hundred points wide; the toolbar drops its labels before it drops its buttons

Links in this document point at reference files. Read one when the work touches what it describes.
