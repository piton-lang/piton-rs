# Instruction Behavior

## Description

An instruction supplies persistent guidance associated with an application scope. Its placement is part of its meaning.

## Requirements

- Treat description and prompt as optional, and leave them out of the output when missing.
- Map instructions under shapeRoot to the corresponding codeRoot scope.
- Combine instructions assigned to the same scope into one guidance file per target.
- Use the target's supported scoped guidance filename and representation.
- Also preserve shape instructions in the target's compiled shape-reference tree.
- Serialize additional properties according to the common serialization rules.
