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

Belay serializes resolved construct content as readable Markdown while preserving the structure of the source.

### Requirements

- Render anchor names and prose-bearing property names as word-separated titles.
- Use heading levels to represent the hierarchy of prose-bearing properties.
- Use bold labels when the hierarchy exceeds Markdown's six heading levels.
- Serialize primitive values to their textual representation.
- Render explicit lists as Markdown lists with nested indentation.
- Render pure dictionaries as indentation-based structures inside code fences.
- Preserve the order of prose and structured content within implicit mixed lists.
- Render pure dictionaries embedded in mixed content as fenced structures.
- Apply target-specific frontmatter and body layout around the serialized content.

### Examples

```
propertyTitle:
  sourceName: myProperty
  renderedTitle: My Property
primitiveText:
  booleanValue: false
  renderedBoolean: false
  numericValue: 42
  renderedNumber: 42
```

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
