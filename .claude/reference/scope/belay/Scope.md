# Shape Mapping

## Description

The shape tree approximately mirrors the application source tree. Instructions attach to the closest existing implementation scope.

## Requirements

- Determine an instruction's relative directory beneath shapeRoot.
- Resolve that relative directory beneath codeRoot.
- Place scoped guidance there when the corresponding directory exists.
- Otherwise walk upward to the nearest existing corresponding directory.
- Stop fallback at codeRoot.
- Combine all instructions resolving to the same destination scope.
- Preserve the original relative structure in the compiled shape-reference tree.

## Examples

### Matching Directory

```
source: spec/shape/components/button/Button.pi
existingScope: src/components/button
genericOutput: src/components/button/AGENTS.md
```

### Shared Directory

#### Sources

- spec/shape/components/input/Input.pi
- spec/shape/components/input/InputDesign.pi

#### Existing Scope

src/components/input

#### Generic Output

src/components/input/AGENTS.md

### Missing Directory

```
source: spec/shape/components/nonexistent/Component.pi
existingScope: src/components
genericOutput: src/components/AGENTS.md
```

## Qualification

AGENTS.md illustrates generic placement. Each adapter selects its supported filename. Fallback changes scoped placement, not the original location preserved in the shape-reference tree.

# Shape Root Reference

## Description

The special export BELAY_COMPILED_SHAPE points at the compiled shape for the current adapter. Not to be confused with BELAY_SHAPE_ROOT, which is the shape source.

## Requirements

- Make BELAY_COMPILED_SHAPE available as an explicit import from Belay.
- Resolve it during compilation rather than at agent runtime.
- Resolve its path relative to the generated file containing its use.
- Resolve it separately for each adapter and output location.

## Documented Claude Location

.claude/reference/shape

# Anchor References

## Description

A reference connects generated guidance to another anchor's compiled representation without embedding all of its content at the use site.

## Requirements

- Serialize reached, referenced anchors into the target's reference directory.
- Preserve their relative source structure in that directory.
- Render referential interpolation as a Markdown link to the compiled artifact.
- Keep referential links distinct from inline value serialization.
- Do not replace lazy Markdown links with Claude-specific eager import syntax.
- Link a reference to a construct to the construct's own output, like its SKILL.md, and don't copy it into the reference directory.

## Representation

Belay uses the Markdown renderer's references: a relative link from the generated file to wherever the other anchor was compiled.
