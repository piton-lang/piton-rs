# Reference Expression

## Description

Evaluates an expression and produces a reference to its result, preserving the identity of the referenced anchor rather than embedding its value or converting it to a string.

## Syntax

@{}

## Evaluation

- Evaluate the enclosed expression using normal expression rules.
- Require the result to identify a referenceable anchor.
- Preserve that anchor's identity until output serialization.
- Report an error if the expression cannot resolve to a referenceable anchor.

## Compilation

- Include the referenced anchor in the compilation dependency graph.
- Resolve its output location through the active renderer, or the framework adapter built on it.
- Resolve each reference independently for each configured output target.
- Report an error if the target cannot represent or resolve the reference.

## Markdown

### Description

Render a Markdown link to the referenced anchor's compiled representation, relative to the file containing the reference.

### Requirements

- Use the referenced anchor's display name as the link text.
- Link to the specific anchor when several anchors share an output file.
- Preserve lazy access rather than automatically including the referenced content.

## Other Formats

```
description: Each renderer must define how anchor identity is represented; see the Reference type for the defaults. A reference must not silently become an embedded copy or a plain name string when the target has no defined reference representation.
```
