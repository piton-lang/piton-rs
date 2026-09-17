# Formatting

## Description

Applying `piton format` to a document without disturbing what did not change

## Rules

- Formatting a document applies `piton format`, the same formatter the command line uses.
- In VS Code the extension is the default formatter for Piton files, so Format Document and format on save both apply `piton format`.
- The edits cover only the lines that changed, so the cursor, selections, and folds elsewhere in the file stay where they were.
- A document that is already formatted produces no edits.
- Text the parser could not read is kept exactly as it was written.
- A line of prose that fits in eighty columns, indentation included, keeps the break the author gave it. Where a line breaks carries no meaning to the compiler, since lines inside a paragraph join with a space, but it carries plenty to the person writing, so a formatter that refilled the paragraph would throw that away.
- A line longer than eighty columns is broken between words into as many lines as it takes. Only that line's own words move, so nothing ever crosses a line break that was already there, let alone the end of a paragraph.
- A list item's text that continues onto further lines keeps the breaks it was written with, and each continuation line is lined up under the item's text, two columns past the `-`, however far it was indented or aligned before. Its spacing is tidied like any other prose.
- A fenced code block is content, not prose: nothing inside it is rewrapped, tidied, or reindented relative to its fence. Only the fence moves to the depth it belongs at, and every line inside moves with it by the same amount.
- An escape group is content too: it is one word, so a line never breaks inside it and its own spacing is kept.
- Spaces between words in prose collapse to one, except that two or more after the end of a sentence become exactly two, and spacing inside a `code span` is kept as written because there it is content.
- A line breaks only between words, never inside the two spaces after a sentence, an expression in braces, or a quoted string.
- Breaking a line changes nothing about what a file compiles to except the spaces between words: a break that would make a line read as something other than prose, such as a lone `-` starting it, is not taken, and a line with no safe way to break is left long, its spacing still tidied.
- A word longer than the line has room for sits on a line of its own rather than being split.
- A line of prose that begins like a key, such as `and: more` or a mistyped `x::bad: 1`, keeps its own line, and no line made by breaking a longer one is allowed to begin like one, so a key written inside prose by mistake stays where the author can see it.
