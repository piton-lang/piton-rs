# Inlay Hints

## Description

Showing the type of a value only where the text does not already say it

## Rules

- A type hint follows a key whose value is a braced expression and that states no constraint, because only there does the text not show what the value is.
- Prose, literals, lists, and dictionaries never get a hint.
- A hint reads as the constraint the author could have written, such as `:: number`.
- Hints are limited to the range the editor asks about.
