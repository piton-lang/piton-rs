# Compilation

## Description

Resolve a Belay project and emit the artifacts selected by its adapters.

## Requirements

- Obtain the emitted and validated constructs from the Piton compiler.
- Build a target-specific output plan for each configured adapter.
- Resolve instruction placement and reference destinations before rendering.
- Detect shared paths and cross-target discovery conflicts across the entire plan.
- Serialize content using the shared rules and each adapter's native format.
- Validate links, metadata, names, and output ownership before writing.

## Serialization

### Description

Belay serializes resolved construct content as readable Markdown while preserving the structure of the source. Everything ends up as a string, so this is how each kind of value turns into Markdown.

### Requirements

- Render anchor names and property names as word-separated titles.
- Give every property of an anchor a heading.
- Inside a property, give a nested key a heading unless its dictionary is pure.
- Use heading levels to represent the hierarchy of properties.
- Use bold labels when the hierarchy exceeds Markdown's six heading levels.
- Serialize primitive values to their textual representation.
- Render explicit lists as Markdown lists with nested indentation.
- Render pure dictionaries as indentation-based structures inside code fences.
- Preserve the order of prose and structured content within implicit mixed lists.
- Render pure dictionaries embedded in mixed content as fenced structures.
- Apply target-specific frontmatter and body layout around the serialized content.

### Pure Dictionaries

A dictionary is pure when every value in it is a simple value (string, number, boolean, or null) or another pure dictionary. It has no lists and no mixed content anywhere inside. A pure dictionary is written out as-is inside a code fence, and its keys don't become headings.
A dictionary that isn't pure, because it holds a list or mixed content somewhere, gets a heading for each key instead. So does mixed content, where a property holds text and keys together.
Strings count as simple values, even long prose. So a dictionary whose values are all prose still gets fenced. To give each key a heading, start the block with a line of text before the keys, or make at least one value a list.

### Titles

Anchor and property names are split into words and title-cased: `myProperty` becomes `My Property`. Runs of capitals stay together, so `whatIsAType` becomes `What Is AType`.

### Headings

Each level of nesting goes one heading level deeper:
```piton
export anchor MyAnchor:
    first:
        First Text

        second:
            Second Text

            third:
                Third Text
```
```markdown
# My Anchor

## First

First Text

### Second

Second Text

#### Third

Third Text
```
Past the sixth level, a heading becomes a bold label, like `**G**`.

### Primitives

Simple values are written as their text, so `false` becomes false and `42` becomes 42. That's true for a property holding one on its own, too: it still gets a heading, with the value under it. `${MyAnchor}` becomes MyAnchor, the name exactly as written.

### Lists

Lists become Markdown lists, and nesting becomes indentation:
```markdown
- First
  - Second
    - Third
```

### Fenced Dictionaries

A pure dictionary goes in a code fence, keeping its shape:
```piton
export anchor MyAnchor:
    first:
        second:
            third: Hello, World
```
````markdown
# My Anchor

## First

```
second:
  third: Hello, World
```
````

### Kitchen Sink

Headings, a list, and a fenced dictionary all together:
```piton
export anchor MyAnchor:
    first:
        This is the first

        second:
            This is the second

            - List
            - of
            - items

        third:
            This is the third

            with:
                a:
                    nested: dictionary
```
````markdown
# My Anchor

## First

This is the first

### Second

This is the second

- List
- of
- items

### Third

This is the third

```
with:
  a:
    nested: dictionary
```
````
third is a sibling of second, so it gets the same heading level. with is a pure dictionary inside mixed content, so it's fenced.

## Interpolation

### Description

Piton evaluates expressions; the selected output mode determines how their results appear in generated artifacts.

### Requirements

- Follow the current Piton specification for expression evaluation and casts.
- Preserve the distinction between intrinsic values, strings, and references.
- Render Belay references as links to the applicable compiled target.
- Do not infer reference identity from a display heading alone.

### String Interpolation

${Anchor} gives you the anchor's name as written in the source. It isn't turned into a title.
