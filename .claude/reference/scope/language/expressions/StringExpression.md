# String Expression

## Description

Evaluates an expression and converts its result to a string.

## Syntax

${}

## Evaluation

- Evaluate the enclosed expression using normal expression rules.
- Return string results unchanged.
- Convert other results according to the stringification rules for their type.
- Report an error when no string representation is defined.

## Conversion

- Convert numbers to their textual representation.
- Convert booleans to lowercase true or false.
- Convert null to the string null.
- Convert anchors to their name, as written in the source.
- Convert a named list or dictionary to its name, like Anchor.propertyName, or just the variable name at the top of a file.
- Report an error for a list or dictionary that has no name.
- Do not reinterpret the resulting string as source syntax or another expression.

## Composition

```
description: The result is a string that can be embedded in surrounding text or participate in an enclosing expression.
```

## Output

```
description: Preserve the string type in structured output. Apply any escaping required by the output format during serialization.
```

## Complex Values

`${}` never pulls in the contents of a list, dictionary, or anchor. It only gives you the name. If you want the contents, or a link to them, use the other two forms. Each one does one job, the same way in every renderer. In a file named Metadata.pi:
```piton
export anchor Metadata:
    name: Piton
    version:: string: 1.0

export byName: Information about Piton ${Metadata}
export byValue: Information about Piton {Metadata}
export byLink: Information about Piton @{Metadata}
```
```json
{
  "byName": "Information about Piton Metadata",
  "byValue": ["Information about Piton", { "name": "Piton", "version": "1.0" }],
  "byLink": "Information about Piton ./Metadata.json:Metadata"
}
```
In Markdown, byValue puts Metadata's content in place under its own headings, and byLink becomes `[Metadata](./Metadata.md#metadata)`.
A reference has to point at an anchor, or a property on one. A plain list or dictionary at the top of a file can't be linked to, so if you want to link to it, put it in an anchor.
